//! In-process SLM inference via candle (replaces llama-server sidecar).
//!
//! Loads a GGUF model directly into the process using candle-core and
//! candle-transformers. Zero external binaries needed at runtime.
//!
//! ## Lifecycle
//!   1. `load_engine(model_path, tokenizer_path)` — loads GGUF + tokenizer.
//!   2. `chat_inference(system, user, max_tokens)` — runs inference.
//!   3. `unload_engine()` — frees model memory.
//!
//! Each `chat_inference` call creates a fresh `ModelWeights` instance to get
//! a clean KV cache. On SSD this adds ~0.3-0.8s overhead per call — acceptable
//! for enrichment (which itself takes 2-5s for token generation).

use std::path::Path;
use std::sync::{Mutex, OnceLock};

use candle_core::quantized::gguf_file;
use candle_core::{Device, Tensor};
use candle_transformers::generation::{LogitsProcessor, Sampling};
use candle_transformers::models::quantized_llama as qlm;
use tokenizers::Tokenizer;

/// Remap GGUF metadata keys so `quantized_llama::ModelWeights::from_gguf` works
/// with non-llama architectures (e.g. Qwen2). The function reads
/// `general.architecture` and copies `{arch}.xxx` keys to `llama.xxx`.
fn remap_gguf_metadata_for_llama(content: &mut gguf_file::Content) {
    let arch = content
        .metadata
        .get("general.architecture")
        .and_then(|v| v.to_string().ok())
        .map(|s| s.to_string())
        .unwrap_or_default();

    if arch == "llama" || arch.is_empty() {
        return;
    }

    let src_prefix = format!("{}.", arch);
    let dst_prefix = "llama.";

    let remapped: Vec<(String, gguf_file::Value)> = content
        .metadata
        .iter()
        .filter(|(k, _)| k.starts_with(&src_prefix))
        .map(|(k, v)| {
            let new_key = format!("{}{}", dst_prefix, &k[src_prefix.len()..]);
            (new_key, v.clone())
        })
        .collect();

    for (k, v) in remapped {
        content.metadata.entry(k).or_insert(v);
    }

    // Synthesize llama.rope.dimension_count if absent — Qwen2 GGUF files
    // don't include this key, but candle's from_gguf requires it.
    // Formula: rope_dim = embedding_length / attention.head_count (= head_dim)
    if !content.metadata.contains_key("llama.rope.dimension_count") {
        let embd = content
            .metadata
            .get("llama.embedding_length")
            .and_then(|v| v.to_u32().ok());
        let heads = content
            .metadata
            .get("llama.attention.head_count")
            .and_then(|v| v.to_u32().ok());
        if let (Some(e), Some(h)) = (embd, heads) {
            if h > 0 {
                content.metadata.insert(
                    "llama.rope.dimension_count".to_string(),
                    gguf_file::Value::U32(e / h),
                );
            }
        }
    }
}

// ── Global engine state ───────────────────────────────────────────────

static ENGINE: OnceLock<Mutex<Option<SlmEngine>>> = OnceLock::new();

fn engine_lock() -> &'static Mutex<Option<SlmEngine>> {
    ENGINE.get_or_init(|| Mutex::new(None))
}

/// In-process small language model engine backed by candle.
#[derive(Debug)]
struct SlmEngine {
    model_path: String,
    tokenizer: Tokenizer,
    device: Device,
}

impl SlmEngine {
    /// Create a new engine, validating the model and tokenizer files.
    fn new(model_path: &str, tokenizer_path: &str) -> Result<Self, String> {
        if !Path::new(model_path).exists() {
            return Err(format!("Model file not found: {}", model_path));
        }
        if !Path::new(tokenizer_path).exists() {
            return Err(format!("Tokenizer file not found: {}", tokenizer_path));
        }

        let device = Device::Cpu;
        let tokenizer = Tokenizer::from_file(tokenizer_path)
            .map_err(|e| format!("Failed to load tokenizer: {}", e))?;

        // Validate GGUF header (fast — reads metadata only, not weights).
        let mut file = std::fs::File::open(model_path)
            .map_err(|e| format!("Failed to open model: {}", e))?;
        let _header = gguf_file::Content::read(&mut file)
            .map_err(|e| format!("Invalid GGUF file: {}", e))?;

        Ok(Self {
            model_path: model_path.to_string(),
            tokenizer,
            device,
        })
    }

    /// Generate text from a pre-formatted prompt.
    ///
    /// Loads a fresh `ModelWeights` each call so the KV cache starts clean.
    fn generate(&self, prompt: &str, max_tokens: u32) -> Result<String, String> {
        // Load weights fresh for clean KV cache.
        let mut file = std::fs::File::open(&self.model_path)
            .map_err(|e| format!("Failed to open model: {}", e))?;
        let mut content = gguf_file::Content::read(&mut file)
            .map_err(|e| format!("Failed to read GGUF: {}", e))?;
        // Remap non-llama architectures (Qwen2, etc.) so quantized_llama can load them.
        remap_gguf_metadata_for_llama(&mut content);
        let mut model = qlm::ModelWeights::from_gguf(content, &mut file, &self.device)
            .map_err(|e| format!("Failed to load weights: {}", e))?;

        let encoded = self
            .tokenizer
            .encode(prompt, true)
            .map_err(|e| format!("Tokenization error: {}", e))?;
        let prompt_tokens = encoded.get_ids().to_vec();
        if prompt_tokens.is_empty() {
            return Ok(String::new());
        }

        let sampling = Sampling::TopKThenTopP {
            k: 40,
            p: 0.9,
            temperature: 0.1,
        };
        let mut logits_processor = LogitsProcessor::from_sampling(42, sampling);

        // ── Prefill: process all prompt tokens at once ──────────────
        let input = Tensor::new(prompt_tokens.as_slice(), &self.device)
            .and_then(|t| t.unsqueeze(0))
            .map_err(|e| format!("Tensor error: {}", e))?;
        let logits = model
            .forward(&input, 0)
            .map_err(|e| format!("Forward error: {}", e))?;
        let logits = logits
            .squeeze(0)
            .map_err(|e| format!("Squeeze error: {}", e))?;

        // Sample from last position's logits.
        let seq_len = logits.dim(0).map_err(|e| format!("Dim error: {}", e))?;
        let last_logits = logits
            .get(seq_len - 1)
            .map_err(|e| format!("Get error: {}", e))?;
        let mut next_token = logits_processor
            .sample(&last_logits)
            .map_err(|e| format!("Sample error: {}", e))?;

        // EOS tokens for Qwen2.5 ChatML
        let eos_ids: Vec<u32> = ["<|im_end|>", "<|endoftext|>", "</s>"]
            .iter()
            .filter_map(|s| self.tokenizer.token_to_id(s))
            .collect();

        let mut output_tokens: Vec<u32> = Vec::with_capacity(max_tokens as usize);
        if !eos_ids.contains(&next_token) {
            output_tokens.push(next_token);
        }

        // ── Autoregressive decode ───────────────────────────────────
        for i in 1..max_tokens {
            if eos_ids.contains(&next_token) {
                break;
            }

            let pos = prompt_tokens.len() + i as usize;
            let input = Tensor::new(&[next_token], &self.device)
                .and_then(|t| t.unsqueeze(0))
                .map_err(|e| format!("Tensor error: {}", e))?;
            let logits = model
                .forward(&input, pos)
                .map_err(|e| format!("Forward error: {}", e))?;
            let logits = logits
                .squeeze(0)
                .map_err(|e| format!("Squeeze error: {}", e))?;

            // Single-token input → may return [1, vocab] or [vocab].
            let token_logits = if logits.dims().len() > 1 {
                logits
                    .get(0)
                    .map_err(|e| format!("Get error: {}", e))?
            } else {
                logits
            };

            next_token = logits_processor
                .sample(&token_logits)
                .map_err(|e| format!("Sample error: {}", e))?;

            if !eos_ids.contains(&next_token) {
                output_tokens.push(next_token);
            }
        }

        let text = self
            .tokenizer
            .decode(&output_tokens, true)
            .map_err(|e| format!("Decode error: {}", e))?;

        Ok(text.trim().to_string())
    }
}

// ── Public API (drop-in replacement for sidecar functions) ────────────

/// Load the SLM engine with a GGUF model and HuggingFace tokenizer.
/// No-op if already loaded.
pub fn load_engine(model_path: &str, tokenizer_path: &str) -> Result<(), String> {
    let mut guard = engine_lock()
        .lock()
        .map_err(|e| format!("Engine lock error: {}", e))?;
    if guard.is_some() {
        return Ok(());
    }
    let engine = SlmEngine::new(model_path, tokenizer_path)?;
    *guard = Some(engine);
    Ok(())
}

/// Unload the engine and free model memory.
pub fn unload_engine() {
    if let Ok(mut guard) = engine_lock().lock() {
        *guard = None;
    }
}

/// Returns true if the SLM engine is loaded and ready for inference.
pub fn is_engine_loaded() -> bool {
    engine_lock()
        .lock()
        .map(|g| g.is_some())
        .unwrap_or(false)
}

/// Run a ChatML-formatted completion against the loaded model.
/// Blocking — must be called from `spawn_blocking` or a sync context.
pub fn chat_inference(system: &str, user: &str, max_tokens: u32) -> Result<String, String> {
    let guard = engine_lock()
        .lock()
        .map_err(|e| format!("Engine lock: {}", e))?;
    let engine = guard.as_ref().ok_or("SLM engine not loaded")?;

    // ChatML prompt format (Qwen2.5 Instruct)
    let prompt = format!(
        "<|im_start|>system\n{system}<|im_end|>\n\
         <|im_start|>user\n{user}<|im_end|>\n\
         <|im_start|>assistant\n"
    );

    engine.generate(&prompt, max_tokens)
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_not_loaded_initially() {
        assert!(!is_engine_loaded());
    }

    #[test]
    fn load_missing_model_returns_error() {
        let result = SlmEngine::new("/nonexistent/model.gguf", "/nonexistent/tokenizer.json");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[test]
    fn chat_inference_without_load_returns_error() {
        // Engine not loaded → should fail gracefully.
        let result = chat_inference("system", "user", 10);
        assert!(result.is_err());
    }

    #[test]
    fn remap_gguf_metadata_copies_qwen2_keys_to_llama() {
        use std::collections::HashMap;
        let mut metadata = HashMap::new();
        metadata.insert(
            "general.architecture".to_string(),
            gguf_file::Value::String("qwen2".to_string()),
        );
        metadata.insert(
            "qwen2.attention.head_count".to_string(),
            gguf_file::Value::U32(16),
        );
        metadata.insert(
            "qwen2.block_count".to_string(),
            gguf_file::Value::U32(24),
        );
        metadata.insert(
            "qwen2.embedding_length".to_string(),
            gguf_file::Value::U32(896),
        );

        let mut content = gguf_file::Content {
            magic: gguf_file::VersionedMagic::GgufV3,
            metadata,
            tensor_infos: HashMap::new(),
            tensor_data_offset: 0,
        };

        remap_gguf_metadata_for_llama(&mut content);

        // Original keys still present
        assert!(content.metadata.contains_key("qwen2.attention.head_count"));
        // Llama keys added
        assert!(content.metadata.contains_key("llama.attention.head_count"));
        assert!(content.metadata.contains_key("llama.block_count"));
        // Values match
        assert_eq!(
            content.metadata["llama.attention.head_count"].to_u32().unwrap(),
            16
        );
        // rope.dimension_count synthesized: 896 / 16 = 56
        assert!(content.metadata.contains_key("llama.rope.dimension_count"));
        assert_eq!(
            content.metadata["llama.rope.dimension_count"].to_u32().unwrap(),
            56 // 896 / 16
        );
    }

    #[test]
    fn remap_gguf_metadata_noop_for_llama_arch() {
        use std::collections::HashMap;
        let mut metadata = HashMap::new();
        metadata.insert(
            "general.architecture".to_string(),
            gguf_file::Value::String("llama".to_string()),
        );
        metadata.insert(
            "llama.attention.head_count".to_string(),
            gguf_file::Value::U32(32),
        );

        let mut content = gguf_file::Content {
            magic: gguf_file::VersionedMagic::GgufV3,
            metadata,
            tensor_infos: HashMap::new(),
            tensor_data_offset: 0,
        };

        remap_gguf_metadata_for_llama(&mut content);

        // No extra keys added
        assert_eq!(content.metadata.len(), 2);
    }
}
