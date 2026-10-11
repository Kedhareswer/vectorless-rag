//! In-process SLM inference via candle (replaces llama-server sidecar).
//!
//! Loads a GGUF model directly into the process using candle-core and
//! candle-transformers. Zero external binaries needed at runtime.
//!
//! ## Lifecycle
//!   1. `load_engine(model_path, tokenizer_path)` — loads GGUF weights + tokenizer once.
//!   2. `chat_inference(system, user, max_tokens)` — runs inference.
//!   3. `unload_engine()` — frees model memory.
//!
//! Weights stay resident between calls. The KV cache is reset at the start of
//! every generation (candle's quantized models drop it when `index_pos == 0`),
//! so calls are independent without re-reading the GGUF from disk.
//!
//! ## Supported architectures
//! Dispatches on GGUF `general.architecture`:
//!   - `lfm2`  → `quantized_lfm2` (Liquid LFM2 / LFM2.5 — the default models)
//!   - `qwen2` → `quantized_qwen2` (includes the Q/K/V attention biases Qwen needs)
//!   - `llama` → `quantized_llama`
//!
//! All use the ChatML prompt format (`<|im_start|>` / `<|im_end|>`).

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use candle_core::quantized::gguf_file;
use candle_core::{Device, Tensor};
use candle_transformers::generation::{LogitsProcessor, Sampling};
use candle_transformers::models::quantized_lfm2 as qlfm2;
use candle_transformers::models::quantized_llama as qllama;
use candle_transformers::models::quantized_qwen2 as qqwen2;
use candle_transformers::utils::apply_repeat_penalty;
use tokenizers::Tokenizer;

/// Wall-clock cap for a single generation. Decoding stops here and the text
/// produced so far is returned, so one slow call can't stall the pipeline.
const GENERATION_TIME_LIMIT: Duration = Duration::from_secs(12);

/// Penalise recently generated tokens — tiny models loop without it.
const REPEAT_PENALTY: f32 = 1.1;
const REPEAT_LAST_N: usize = 64;

// ── Model weights per architecture ────────────────────────────────────

enum ModelWeights {
    Lfm2(qlfm2::ModelWeights),
    Llama(qllama::ModelWeights),
    Qwen2(qqwen2::ModelWeights),
}

impl ModelWeights {
    fn load(model_path: &str, device: &Device) -> Result<Self, String> {
        let mut file = std::fs::File::open(model_path)
            .map_err(|e| format!("Failed to open model: {}", e))?;
        let content = gguf_file::Content::read(&mut file)
            .map_err(|e| format!("Invalid GGUF file: {}", e))?;
        let arch = gguf_architecture(&content);

        match arch.as_str() {
            "lfm2" => qlfm2::ModelWeights::from_gguf(content, &mut file, device)
                .map(Self::Lfm2)
                .map_err(|e| format!("Failed to load lfm2 weights: {}", e)),
            "qwen2" => qqwen2::ModelWeights::from_gguf(content, &mut file, device)
                .map(Self::Qwen2)
                .map_err(|e| format!("Failed to load qwen2 weights: {}", e)),
            "llama" | "" => qllama::ModelWeights::from_gguf(content, &mut file, device)
                .map(Self::Llama)
                .map_err(|e| format!("Failed to load llama weights: {}", e)),
            other => Err(format!("Unsupported model architecture: {}", other)),
        }
    }

    /// Run the model; returns logits for the last input position.
    ///
    /// State resets when `index_pos == 0`: attention KV caches restart, and
    /// LFM2's short-convolution caches are rebuilt from any multi-token input
    /// (every ChatML prompt is many tokens).
    fn forward(&mut self, input: &Tensor, index_pos: usize) -> candle_core::Result<Tensor> {
        match self {
            Self::Lfm2(m) => m.forward(input, index_pos),
            Self::Llama(m) => m.forward(input, index_pos),
            Self::Qwen2(m) => m.forward(input, index_pos),
        }
    }
}

fn gguf_architecture(content: &gguf_file::Content) -> String {
    content
        .metadata
        .get("general.architecture")
        .and_then(|v| v.to_string().ok())
        .cloned()
        .unwrap_or_default()
}

/// Reduce model output to a 1-D `[vocab]` logits vector.
///
/// candle's quantized models already return only the last position, shaped
/// `[batch=1, vocab]`; a `[1, seq, vocab]` tensor is also accepted and its last
/// position used.
fn last_token_logits(logits: &Tensor) -> candle_core::Result<Tensor> {
    let logits = logits.squeeze(0)?;
    match logits.rank() {
        1 => Ok(logits),
        2 => {
            let seq_len = logits.dim(0)?;
            logits.get(seq_len - 1)
        }
        rank => candle_core::bail!("unexpected logits rank {}", rank),
    }
}

// ── Global engine state ───────────────────────────────────────────────

static ENGINE: OnceLock<Mutex<Option<SlmEngine>>> = OnceLock::new();

fn engine_lock() -> &'static Mutex<Option<SlmEngine>> {
    ENGINE.get_or_init(|| Mutex::new(None))
}

/// In-process small language model engine backed by candle.
struct SlmEngine {
    model: ModelWeights,
    tokenizer: Tokenizer,
    device: Device,
    eos_ids: Vec<u32>,
    /// Begin-of-text token the model expects first (LFM2: `<|startoftext|>`).
    bos_id: Option<u32>,
}

/// Output of one generation call.
#[derive(Debug, Clone)]
pub struct Generation {
    pub text: String,
    pub prompt_tokens: u32,
    pub output_tokens: u32,
}

impl SlmEngine {
    /// Load the tokenizer and GGUF weights.
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
        let model = ModelWeights::load(model_path, &device)?;

        // EOS tokens for ChatML (Qwen) and Llama-style vocabularies.
        let eos_ids: Vec<u32> = ["<|im_end|>", "<|endoftext|>", "</s>", "<|eot_id|>"]
            .iter()
            .filter_map(|s| tokenizer.token_to_id(s))
            .collect();

        let bos_id = tokenizer.token_to_id("<|startoftext|>");

        Ok(Self {
            model,
            tokenizer,
            device,
            eos_ids,
            bos_id,
        })
    }

    /// Generate text from a pre-formatted prompt.
    ///
    /// Stops at EOS, `max_tokens`, the time limit, or when `cancel` is set.
    fn generate(
        &mut self,
        prompt: &str,
        max_tokens: u32,
        cancel: Option<&AtomicBool>,
    ) -> Result<Generation, String> {
        let deadline = Instant::now() + GENERATION_TIME_LIMIT;
        let should_stop =
            || Instant::now() >= deadline || cancel.is_some_and(|c| c.load(Ordering::SeqCst));

        let encoded = self
            .tokenizer
            .encode(prompt, true)
            .map_err(|e| format!("Tokenization error: {}", e))?;
        let mut prompt_tokens = encoded.get_ids().to_vec();
        if let Some(bos) = self.bos_id
            && prompt_tokens.first() != Some(&bos)
        {
            prompt_tokens.insert(0, bos);
        }
        if prompt_tokens.is_empty() || max_tokens == 0 {
            return Ok(Generation {
                text: String::new(),
                prompt_tokens: prompt_tokens.len() as u32,
                output_tokens: 0,
            });
        }

        let sampling = Sampling::TopKThenTopP {
            k: 40,
            p: 0.9,
            temperature: 0.1,
        };
        let mut logits_processor = LogitsProcessor::from_sampling(42, sampling);
        let mut output_tokens: Vec<u32> = Vec::with_capacity(max_tokens as usize);

        // Prefill: the whole prompt at positions 0..n. Position 0 also resets the KV cache.
        let mut input = prompt_tokens.clone();
        let mut pos = 0usize;

        while output_tokens.len() < max_tokens as usize {
            let input_tensor = Tensor::new(input.as_slice(), &self.device)
                .and_then(|t| t.unsqueeze(0))
                .map_err(|e| format!("Tensor error: {}", e))?;
            let logits = self
                .model
                .forward(&input_tensor, pos)
                .and_then(|l| last_token_logits(&l))
                .map_err(|e| format!("Forward error: {}", e))?;
            pos += input.len();

            let logits = if output_tokens.is_empty() {
                logits
            } else {
                let start = output_tokens.len().saturating_sub(REPEAT_LAST_N);
                apply_repeat_penalty(&logits, REPEAT_PENALTY, &output_tokens[start..])
                    .map_err(|e| format!("Repeat penalty error: {}", e))?
            };

            let next_token = logits_processor
                .sample(&logits)
                .map_err(|e| format!("Sample error: {}", e))?;
            if self.eos_ids.contains(&next_token) {
                break;
            }
            output_tokens.push(next_token);

            if should_stop() {
                break;
            }
            // Decode: feed the sampled token at the next position.
            input = vec![next_token];
        }

        let text = self
            .tokenizer
            .decode(&output_tokens, true)
            .map_err(|e| format!("Decode error: {}", e))?;

        Ok(Generation {
            text: text.trim().to_string(),
            prompt_tokens: prompt_tokens.len() as u32,
            output_tokens: output_tokens.len() as u32,
        })
    }
}

// ── Public API ────────────────────────────────────────────────────────

/// Load the SLM engine with a GGUF model and HuggingFace tokenizer.
/// No-op if already loaded. Weights are read outside the lock so a slow
/// load does not block `is_engine_loaded()` checks.
pub fn load_engine(model_path: &str, tokenizer_path: &str) -> Result<(), String> {
    if is_engine_loaded() {
        return Ok(());
    }
    let engine = SlmEngine::new(model_path, tokenizer_path)?;
    let mut guard = engine_lock()
        .lock()
        .map_err(|e| format!("Engine lock error: {}", e))?;
    if guard.is_none() {
        *guard = Some(engine);
    }
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

/// Format a ChatML prompt (LFM2 / Qwen2.5 Instruct template).
fn chatml_prompt(system: &str, user: &str) -> String {
    format!(
        "<|im_start|>system\n{system}<|im_end|>\n\
         <|im_start|>user\n{user}<|im_end|>\n\
         <|im_start|>assistant\n"
    )
}

/// Run a ChatML-formatted completion against the loaded model.
/// Blocking — must be called from `spawn_blocking` or a sync context.
/// Calls are serialised: the model has a single KV cache.
pub fn generate(
    system: &str,
    user: &str,
    max_tokens: u32,
    cancel: Option<&AtomicBool>,
) -> Result<Generation, String> {
    let mut guard = engine_lock()
        .lock()
        .map_err(|e| format!("Engine lock: {}", e))?;
    let engine = guard.as_mut().ok_or("SLM engine not loaded")?;
    engine.generate(&chatml_prompt(system, user), max_tokens, cancel)
}

/// Convenience wrapper returning only the generated text.
pub fn chat_inference(system: &str, user: &str, max_tokens: u32) -> Result<String, String> {
    generate(system, user, max_tokens, None).map(|g| g.text)
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
        assert!(result.err().unwrap().contains("not found"));
    }

    #[test]
    fn chat_inference_without_load_returns_error() {
        // Engine not loaded → should fail gracefully.
        let result = chat_inference("system", "user", 10);
        assert!(result.is_err());
    }

    #[test]
    fn chatml_prompt_wraps_roles() {
        let p = chatml_prompt("sys", "hi");
        assert_eq!(
            p,
            "<|im_start|>system\nsys<|im_end|>\n<|im_start|>user\nhi<|im_end|>\n<|im_start|>assistant\n"
        );
    }

    #[test]
    fn last_token_logits_accepts_model_output_shape() {
        // Quantized models return [batch=1, vocab] for the last position.
        let logits = Tensor::new(&[[0.1f32, 0.9, 0.0, 0.2]], &Device::Cpu).unwrap();
        let out = last_token_logits(&logits).unwrap();
        assert_eq!(out.dims(), &[4]);

        // The sampler needs that 1-D vector; greedy sampling picks the max.
        let mut lp = LogitsProcessor::from_sampling(0, Sampling::ArgMax);
        assert_eq!(lp.sample(&out).unwrap(), 1);
    }

    #[test]
    fn last_token_logits_uses_last_position_of_full_sequence() {
        let logits = Tensor::new(
            &[[[1.0f32, 0.0, 0.0], [0.0, 0.0, 1.0]]],
            &Device::Cpu,
        )
        .unwrap();
        let out = last_token_logits(&logits).unwrap();
        assert_eq!(out.to_vec1::<f32>().unwrap(), vec![0.0, 0.0, 1.0]);
    }

    #[test]
    fn indexing_vocab_dim_yields_unsampleable_scalar() {
        // Regression: the previous implementation squeezed [1, vocab] to [vocab]
        // and then took `.get(len - 1)`, i.e. one scalar logit — sampling that fails,
        // so every SLM call errored.
        let logits = Tensor::new(&[[0.1f32, 0.9, 0.0, 0.2]], &Device::Cpu)
            .unwrap()
            .squeeze(0)
            .unwrap();
        let scalar = logits.get(logits.dim(0).unwrap() - 1).unwrap();
        let mut lp = LogitsProcessor::from_sampling(0, Sampling::ArgMax);
        assert!(lp.sample(&scalar).is_err());
    }

    const VOCAB: usize = 32;
    const EMBD: usize = 8;
    const KV: usize = 4; // 1 kv head × head_dim 4
    const FFN: usize = 16;

    fn rand_q(shape: &[usize]) -> candle_core::quantized::QTensor {
        use candle_core::quantized::{GgmlDType, QTensor};
        let t = Tensor::randn(0f32, 0.5, shape, &Device::Cpu).unwrap();
        QTensor::quantize(&t, GgmlDType::F32).unwrap()
    }

    fn write_gguf(
        path: &Path,
        metadata: &[(&str, gguf_file::Value)],
        tensors: &[(String, candle_core::quantized::QTensor)],
    ) {
        let mut file = std::fs::File::create(path).unwrap();
        let md: Vec<(&str, &gguf_file::Value)> = metadata.iter().map(|(k, v)| (*k, v)).collect();
        let ts: Vec<(&str, &candle_core::quantized::QTensor)> =
            tensors.iter().map(|(k, v)| (k.as_str(), v)).collect();
        gguf_file::write(&mut file, &md, &ts).unwrap();
    }

    /// Word-level tokenizer over `w2..w31`, with `<|im_end|>` (EOS) and
    /// optionally `<|startoftext|>` (BOS) as special tokens.
    fn write_tokenizer(path: &Path, with_bos: bool) {
        use std::collections::HashMap;
        use tokenizers::models::wordlevel::WordLevel;
        use tokenizers::pre_tokenizers::whitespace::Whitespace;

        let first = if with_bos { "<|startoftext|>" } else { "[UNK]" };
        let mut words: Vec<String> = vec![first.to_string(), "<|im_end|>".to_string()];
        words.extend((words.len()..VOCAB).map(|i| format!("w{i}")));
        let vocab: HashMap<String, u32> =
            words.into_iter().enumerate().map(|(i, w)| (w, i as u32)).collect();
        let wordlevel = WordLevel::builder()
            .vocab(vocab.into_iter().collect())
            .unk_token("w2".to_string())
            .build()
            .unwrap();
        let mut tokenizer = Tokenizer::new(wordlevel);
        tokenizer.with_pre_tokenizer(Some(Whitespace {}));
        tokenizer.save(path, false).unwrap();
    }

    /// Write a tiny random-weight Qwen2 GGUF plus a word-level tokenizer.
    fn write_tiny_qwen2(dir: &Path) -> (String, String) {
        let tensors: Vec<(String, _)> = [
            ("token_embd.weight", vec![VOCAB, EMBD]),
            ("output_norm.weight", vec![EMBD]),
            ("output.weight", vec![VOCAB, EMBD]),
            ("blk.0.attn_q.weight", vec![EMBD, EMBD]),
            ("blk.0.attn_k.weight", vec![KV, EMBD]),
            ("blk.0.attn_v.weight", vec![KV, EMBD]),
            ("blk.0.attn_q.bias", vec![EMBD]),
            ("blk.0.attn_k.bias", vec![KV]),
            ("blk.0.attn_v.bias", vec![KV]),
            ("blk.0.attn_output.weight", vec![EMBD, EMBD]),
            ("blk.0.ffn_gate.weight", vec![FFN, EMBD]),
            ("blk.0.ffn_up.weight", vec![FFN, EMBD]),
            ("blk.0.ffn_down.weight", vec![EMBD, FFN]),
            ("blk.0.attn_norm.weight", vec![EMBD]),
            ("blk.0.ffn_norm.weight", vec![EMBD]),
        ]
        .into_iter()
        .map(|(name, shape)| (name.to_string(), rand_q(&shape)))
        .collect();
        let metadata = [
            ("general.architecture", gguf_file::Value::String("qwen2".to_string())),
            ("qwen2.attention.head_count", gguf_file::Value::U32(2)),
            ("qwen2.attention.head_count_kv", gguf_file::Value::U32(1)),
            ("qwen2.embedding_length", gguf_file::Value::U32(EMBD as u32)),
            ("qwen2.context_length", gguf_file::Value::U32(256)),
            ("qwen2.block_count", gguf_file::Value::U32(1)),
            ("qwen2.attention.layer_norm_rms_epsilon", gguf_file::Value::F32(1e-6)),
        ];

        let model_path = dir.join("tiny.gguf");
        let tokenizer_path = dir.join("tokenizer.json");
        write_gguf(&model_path, &metadata, &tensors);
        write_tokenizer(&tokenizer_path, false);
        (
            model_path.to_string_lossy().into_owned(),
            tokenizer_path.to_string_lossy().into_owned(),
        )
    }

    /// Write a tiny LFM2 GGUF: block 0 is a short-convolution block, block 1
    /// is attention (selected by a per-layer `head_count_kv` of 0 / 1).
    fn write_tiny_lfm2(dir: &Path) -> (String, String) {
        const L_CACHE: usize = 3;
        let mut shapes: Vec<(String, Vec<usize>)> = vec![
            ("token_embd.weight".into(), vec![VOCAB, EMBD]),
            ("output_norm.weight".into(), vec![EMBD]),
            ("blk.0.shortconv.in_proj.weight".into(), vec![3 * EMBD, EMBD]),
            ("blk.0.shortconv.out_proj.weight".into(), vec![EMBD, EMBD]),
            ("blk.0.shortconv.conv.weight".into(), vec![EMBD, L_CACHE]),
            ("blk.1.attn_q.weight".into(), vec![EMBD, EMBD]),
            ("blk.1.attn_k.weight".into(), vec![KV, EMBD]),
            ("blk.1.attn_v.weight".into(), vec![KV, EMBD]),
            ("blk.1.attn_output.weight".into(), vec![EMBD, EMBD]),
            ("blk.1.attn_q_norm.weight".into(), vec![KV]),
            ("blk.1.attn_k_norm.weight".into(), vec![KV]),
        ];
        for b in 0..2 {
            shapes.push((format!("blk.{b}.attn_norm.weight"), vec![EMBD]));
            shapes.push((format!("blk.{b}.ffn_norm.weight"), vec![EMBD]));
            shapes.push((format!("blk.{b}.ffn_gate.weight"), vec![FFN, EMBD]));
            shapes.push((format!("blk.{b}.ffn_up.weight"), vec![FFN, EMBD]));
            shapes.push((format!("blk.{b}.ffn_down.weight"), vec![EMBD, FFN]));
        }
        let tensors: Vec<(String, _)> =
            shapes.into_iter().map(|(n, shape)| (n, rand_q(&shape))).collect();
        let metadata = [
            ("general.architecture", gguf_file::Value::String("lfm2".to_string())),
            ("lfm2.attention.head_count", gguf_file::Value::U32(2)),
            (
                "lfm2.attention.head_count_kv",
                gguf_file::Value::Array(vec![gguf_file::Value::U32(0), gguf_file::Value::U32(1)]),
            ),
            ("lfm2.embedding_length", gguf_file::Value::U32(EMBD as u32)),
            ("lfm2.context_length", gguf_file::Value::U32(256)),
            ("lfm2.block_count", gguf_file::Value::U32(2)),
            ("lfm2.attention.layer_norm_rms_epsilon", gguf_file::Value::F32(1e-5)),
            ("lfm2.shortconv.l_cache", gguf_file::Value::U32(L_CACHE as u32)),
        ];

        let model_path = dir.join("tiny-lfm2.gguf");
        let tokenizer_path = dir.join("tiny-lfm2.tokenizer.json");
        write_gguf(&model_path, &metadata, &tensors);
        write_tokenizer(&tokenizer_path, true);
        (
            model_path.to_string_lossy().into_owned(),
            tokenizer_path.to_string_lossy().into_owned(),
        )
    }

    #[test]
    fn generates_end_to_end_with_lfm2_gguf() {
        let tmp = tempfile::tempdir().unwrap();
        let (model, tokenizer) = write_tiny_lfm2(tmp.path());
        let mut engine = SlmEngine::new(&model, &tokenizer).unwrap();
        assert!(matches!(engine.model, ModelWeights::Lfm2(_)));
        assert_eq!(engine.bos_id, Some(0));

        let prompt = chatml_prompt("w5 w6", "w7 w8 w9");
        let first = engine.generate(&prompt, 12, None).unwrap();
        assert!(first.prompt_tokens > 0);
        assert!(first.output_tokens <= 12);

        // A different prompt in between must not leak conv/KV state into the rerun.
        engine.generate(&chatml_prompt("w20", "w21 w22 w23 w24"), 8, None).unwrap();
        let again = engine.generate(&prompt, 12, None).unwrap();
        assert_eq!(first.text, again.text);
        assert_eq!(first.output_tokens, again.output_tokens);
    }

    #[test]
    fn generates_end_to_end_with_qwen2_gguf() {
        let tmp = tempfile::tempdir().unwrap();
        let (model, tokenizer) = write_tiny_qwen2(tmp.path());
        let mut engine = SlmEngine::new(&model, &tokenizer).unwrap();
        assert!(matches!(engine.model, ModelWeights::Qwen2(_)));

        let prompt = chatml_prompt("w5 w6", "w7 w8 w9");
        let first = engine.generate(&prompt, 12, None).unwrap();
        assert!(first.prompt_tokens > 0);
        assert!(first.output_tokens <= 12);

        // Same prompt again: KV cache must reset, so output is identical.
        let second = engine.generate(&prompt, 12, None).unwrap();
        assert_eq!(first.text, second.text);
        assert_eq!(first.output_tokens, second.output_tokens);
    }

    #[test]
    fn generation_stops_immediately_when_cancelled() {
        let tmp = tempfile::tempdir().unwrap();
        let (model, tokenizer) = write_tiny_qwen2(tmp.path());
        let mut engine = SlmEngine::new(&model, &tokenizer).unwrap();
        let cancel = AtomicBool::new(true);
        let out = engine
            .generate(&chatml_prompt("w5", "w7"), 50, Some(&cancel))
            .unwrap();
        assert!(out.output_tokens <= 1);
    }

    #[test]
    fn gguf_architecture_reads_metadata() {
        use std::collections::HashMap;
        let mut metadata = HashMap::new();
        metadata.insert(
            "general.architecture".to_string(),
            gguf_file::Value::String("qwen2".to_string()),
        );
        let content = gguf_file::Content {
            magic: gguf_file::VersionedMagic::GgufV3,
            metadata,
            tensor_infos: HashMap::new(),
            tensor_data_offset: 0,
        };
        assert_eq!(gguf_architecture(&content), "qwen2");
    }
}
