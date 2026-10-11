//! Offline dictation (speech → text) with OpenAI Whisper, run in-process by candle.
//!
//! The frontend records the microphone, resamples to 16 kHz mono f32 PCM and
//! calls `transcribe_audio`. Audio never leaves the machine.
//!
//! ## Models
//! Downloaded on demand into `<app-data>/models/dictation/<model-id>/` as
//! `model.gguf` (8-bit) or `model.safetensors`, plus `config.json` and
//! `tokenizer.json`. Default is Whisper tiny.en at 8-bit (~42 MB).
//!
//! ## Decoding
//! Greedy, English, no timestamps. Long recordings are split into Whisper's
//! 30-second windows; windows the model scores as non-speech are dropped.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use candle_core::{Device, IndexOp, Tensor};
use candle_nn::ops::softmax;
use candle_transformers::models::whisper::{self as wh, audio, Config};
use serde::Serialize;
use tokenizers::Tokenizer;
use tokio::sync::mpsc;

use super::local::{download_file, models_dir, DownloadProgress};

/// Longest recording accepted in one call (5 minutes of 16 kHz audio).
pub const MAX_SAMPLES: usize = 5 * 60 * wh::SAMPLE_RATE;

/// Whisper's 80-bin mel filterbank (from candle's whisper example, MIT/Apache-2.0).
const MEL_FILTERS_80: &[u8] = include_bytes!("whisper_melfilters.bytes");

// ── Model catalog ─────────────────────────────────────────────────────

/// A downloadable dictation model presented to the user.
#[derive(Serialize, Clone, Debug)]
pub struct DictationModelOption {
    pub id: String,
    pub name: String,
    pub description: String,
    pub size_bytes: u64,
    pub size_label: String,
    #[serde(skip)]
    quantized: bool,
    /// (download URL, local file name)
    #[serde(skip)]
    files: Vec<(String, String)>,
}

/// Download state of the dictation model.
#[derive(Serialize, Clone, Debug)]
pub struct DictationStatus {
    pub downloaded: bool,
    pub model_id: Option<String>,
    pub loaded: bool,
}

const SETTING_MODEL_ID: &str = "dictation_model_id";

/// Available dictation models. The first is the recommended default.
pub fn get_options() -> Vec<DictationModelOption> {
    let lmz = "https://huggingface.co/lmz/candle-whisper/resolve/main";
    let base_en = "https://huggingface.co/openai/whisper-base.en/resolve/refs%2Fpr%2F13";
    vec![
        DictationModelOption {
            id: "whisper-tiny-en-q8".to_string(),
            name: "Whisper tiny (English)".to_string(),
            description: "Fast and small. Good for clear speech.".to_string(),
            size_bytes: 42_000_000,
            size_label: "~42 MB".to_string(),
            quantized: true,
            files: vec![
                (format!("{lmz}/model-tiny-en-q80.gguf"), "model.gguf".to_string()),
                (format!("{lmz}/config-tiny-en.json"), "config.json".to_string()),
                (format!("{lmz}/tokenizer-tiny-en.json"), "tokenizer.json".to_string()),
            ],
        },
        DictationModelOption {
            id: "whisper-base-en".to_string(),
            name: "Whisper base (English)".to_string(),
            description: "More accurate, larger download.".to_string(),
            size_bytes: 290_000_000,
            size_label: "~290 MB".to_string(),
            quantized: false,
            files: vec![
                (format!("{base_en}/model.safetensors"), "model.safetensors".to_string()),
                (format!("{base_en}/config.json"), "config.json".to_string()),
                (format!("{base_en}/tokenizer.json"), "tokenizer.json".to_string()),
            ],
        },
    ]
}

fn option(model_id: &str) -> Result<DictationModelOption, String> {
    get_options()
        .into_iter()
        .find(|o| o.id == model_id)
        .ok_or_else(|| format!("Unknown dictation model: {}", model_id))
}

/// Directory holding one dictation model's files.
pub fn model_dir(app_data: &Path, model_id: &str) -> PathBuf {
    models_dir(app_data).join("dictation").join(model_id)
}

fn is_downloaded(app_data: &Path, opt: &DictationModelOption) -> bool {
    let dir = model_dir(app_data, &opt.id);
    opt.files.iter().all(|(_, name)| dir.join(name).exists())
}

/// Report whether the selected dictation model is downloaded and loaded.
pub fn status(app_data: &Path, db: &crate::db::Database) -> DictationStatus {
    let model_id = db
        .get_setting(SETTING_MODEL_ID)
        .ok()
        .flatten()
        .filter(|id| !id.is_empty());
    let downloaded = model_id
        .as_deref()
        .and_then(|id| option(id).ok())
        .is_some_and(|opt| is_downloaded(app_data, &opt));
    DictationStatus {
        downloaded,
        model_id,
        loaded: is_loaded(),
    }
}

/// Download every file of a dictation model, then select it.
pub async fn download(
    app_data: &Path,
    db: &crate::db::Database,
    model_id: &str,
    progress_tx: mpsc::UnboundedSender<DownloadProgress>,
) -> Result<(), String> {
    let opt = option(model_id)?;
    let dir = model_dir(app_data, &opt.id);
    std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create model dir: {}", e))?;

    let label = format!("Downloading {}...", opt.name);
    for (url, name) in &opt.files {
        let dest = dir.join(name);
        if dest.metadata().is_ok_and(|m| m.len() > 0) {
            continue;
        }
        download_file(url, &dest, &label, opt.size_bytes, &progress_tx).await?;
    }

    db.set_setting(SETTING_MODEL_ID, &opt.id)
        .map_err(|e| e.to_string())?;
    unload();
    let _ = progress_tx.send(DownloadProgress {
        downloaded_bytes: opt.size_bytes,
        total_bytes: opt.size_bytes,
        percent: 100.0,
        done: true,
        error: None,
        phase: "Complete".to_string(),
    });
    Ok(())
}

/// Remove all downloaded dictation models and clear the selection.
pub fn delete(app_data: &Path, db: &crate::db::Database) -> Result<(), String> {
    unload();
    let dir = models_dir(app_data).join("dictation");
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(|e| format!("Failed to delete models: {}", e))?;
    }
    db.set_setting(SETTING_MODEL_ID, "").map_err(|e| e.to_string())?;
    Ok(())
}

// ── Engine ────────────────────────────────────────────────────────────

enum WhisperModel {
    Normal(wh::model::Whisper),
    Quantized(wh::quantized_model::Whisper),
}

impl WhisperModel {
    fn config(&self) -> &Config {
        match self {
            Self::Normal(m) => &m.config,
            Self::Quantized(m) => &m.config,
        }
    }

    fn encode(&mut self, mel: &Tensor) -> candle_core::Result<Tensor> {
        match self {
            Self::Normal(m) => m.encoder.forward(mel, true),
            Self::Quantized(m) => m.encoder.forward(mel, true),
        }
    }

    fn decode(&mut self, tokens: &Tensor, audio: &Tensor, flush: bool) -> candle_core::Result<Tensor> {
        match self {
            Self::Normal(m) => m.decoder.forward(tokens, audio, flush),
            Self::Quantized(m) => m.decoder.forward(tokens, audio, flush),
        }
    }

    fn final_linear(&self, x: &Tensor) -> candle_core::Result<Tensor> {
        match self {
            Self::Normal(m) => m.decoder.final_linear(x),
            Self::Quantized(m) => m.decoder.final_linear(x),
        }
    }
}

struct DictationEngine {
    model_id: String,
    model: WhisperModel,
    tokenizer: Tokenizer,
    mel_filters: Vec<f32>,
    device: Device,
    /// Additive mask: -inf for the model's suppressed tokens, 0 elsewhere.
    suppress: Tensor,
    sot: u32,
    transcribe: u32,
    no_timestamps: u32,
    eot: u32,
    no_speech: u32,
}

/// One decoded 30-second window.
struct Window {
    text: String,
    avg_logprob: f64,
    no_speech_prob: f64,
}

fn token_id(tokenizer: &Tokenizer, token: &str) -> Result<u32, String> {
    tokenizer
        .token_to_id(token)
        .ok_or_else(|| format!("Tokenizer is missing {}", token))
}

fn mel_filters(num_mel_bins: usize) -> Result<Vec<f32>, String> {
    if num_mel_bins != 80 {
        return Err(format!("Unsupported mel bin count: {}", num_mel_bins));
    }
    Ok(MEL_FILTERS_80
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect())
}

impl DictationEngine {
    fn load(model_id: &str, dir: &Path, quantized: bool) -> Result<Self, String> {
        let device = Device::Cpu;
        let config: Config = serde_json::from_slice(
            &std::fs::read(dir.join("config.json")).map_err(|e| format!("Missing config: {}", e))?,
        )
        .map_err(|e| format!("Invalid Whisper config: {}", e))?;
        let tokenizer = Tokenizer::from_file(dir.join("tokenizer.json"))
            .map_err(|e| format!("Failed to load tokenizer: {}", e))?;

        let model = if quantized {
            let vb = candle_transformers::quantized_var_builder::VarBuilder::from_gguf(
                dir.join("model.gguf"),
                &device,
            )
            .map_err(|e| format!("Failed to read Whisper weights: {}", e))?;
            WhisperModel::Quantized(
                wh::quantized_model::Whisper::load(&vb, config)
                    .map_err(|e| format!("Failed to load Whisper: {}", e))?,
            )
        } else {
            // SAFETY: the weights file is memory-mapped read-only; it lives in the
            // app's data directory and isn't modified while the engine is loaded.
            let vb = unsafe {
                candle_nn::VarBuilder::from_mmaped_safetensors(
                    &[dir.join("model.safetensors")],
                    wh::DTYPE,
                    &device,
                )
            }
            .map_err(|e| format!("Failed to read Whisper weights: {}", e))?;
            WhisperModel::Normal(
                wh::model::Whisper::load(&vb, config)
                    .map_err(|e| format!("Failed to load Whisper: {}", e))?,
            )
        };

        Self::from_parts(model_id, model, tokenizer, device)
    }

    fn from_parts(
        model_id: &str,
        model: WhisperModel,
        tokenizer: Tokenizer,
        device: Device,
    ) -> Result<Self, String> {
        let config = model.config().clone();
        let mel_filters = mel_filters(config.num_mel_bins)?;
        let suppress: Vec<f32> = (0..config.vocab_size as u32)
            .map(|i| {
                if config.suppress_tokens.contains(&i) {
                    f32::NEG_INFINITY
                } else {
                    0.0
                }
            })
            .collect();
        let suppress = Tensor::new(suppress.as_slice(), &device).map_err(|e| e.to_string())?;
        let no_speech = wh::NO_SPEECH_TOKENS
            .iter()
            .find_map(|t| tokenizer.token_to_id(t))
            .ok_or("Tokenizer is missing a no-speech token")?;

        Ok(Self {
            model_id: model_id.to_string(),
            sot: token_id(&tokenizer, wh::SOT_TOKEN)?,
            transcribe: token_id(&tokenizer, wh::TRANSCRIBE_TOKEN)?,
            no_timestamps: token_id(&tokenizer, wh::NO_TIMESTAMPS_TOKEN)?,
            eot: token_id(&tokenizer, wh::EOT_TOKEN)?,
            no_speech,
            model,
            tokenizer,
            mel_filters,
            device,
            suppress,
        })
    }

    /// Transcribe 16 kHz mono PCM samples.
    fn transcribe(&mut self, pcm: &[f32]) -> Result<String, String> {
        // Under 0.1 s there's nothing to transcribe.
        if pcm.len() < wh::SAMPLE_RATE / 10 {
            return Ok(String::new());
        }
        let config = self.model.config().clone();
        let mel = audio::pcm_to_mel(&config, pcm, &self.mel_filters);
        let frames = mel.len() / config.num_mel_bins;
        let mel = Tensor::from_vec(mel, (1, config.num_mel_bins, frames), &self.device)
            .map_err(|e| e.to_string())?;

        let mut parts: Vec<String> = Vec::new();
        let mut seek = 0;
        while seek < frames {
            let size = usize::min(frames - seek, wh::N_FRAMES);
            let segment = mel.narrow(2, seek, size).map_err(|e| e.to_string())?;
            seek += size;

            let window = self.decode_window(&segment).map_err(|e| e.to_string())?;
            let silent = window.no_speech_prob > wh::NO_SPEECH_THRESHOLD
                && window.avg_logprob < wh::LOGPROB_THRESHOLD;
            let text = window.text.trim();
            if !silent && !text.is_empty() {
                parts.push(text.to_string());
            }
        }
        Ok(parts.join(" "))
    }

    /// Greedy-decode one mel window (English, transcribe task, no timestamps).
    fn decode_window(&mut self, mel: &Tensor) -> candle_core::Result<Window> {
        let audio_features = self.model.encode(mel)?;
        let max_positions = self.model.config().max_target_positions;
        let sample_len = max_positions / 2;
        let mut tokens = vec![self.sot, self.transcribe, self.no_timestamps];
        let prompt_len = tokens.len();
        let mut sum_logprob = 0f64;
        let mut no_speech_prob = f64::NAN;

        for i in 0..sample_len {
            let tokens_t = Tensor::new(tokens.as_slice(), &self.device)?.unsqueeze(0)?;
            let ys = self.model.decode(&tokens_t, &audio_features, i == 0)?;

            if i == 0 {
                let logits = self.model.final_linear(&ys.i(..1)?)?.i(0)?.i(0)?;
                no_speech_prob = softmax(&logits, 0)?
                    .i(self.no_speech as usize)?
                    .to_scalar::<f32>()? as f64;
            }

            let (_, seq_len, _) = ys.dims3()?;
            let logits = self
                .model
                .final_linear(&ys.i((..1, seq_len - 1..))?)?
                .i(0)?
                .i(0)?
                .broadcast_add(&self.suppress)?;
            let logits_v: Vec<f32> = logits.to_vec1()?;
            let next = logits_v
                .iter()
                .enumerate()
                .max_by(|(_, a), (_, b)| a.total_cmp(b))
                .map(|(i, _)| i as u32)
                .unwrap_or(self.eot);
            tokens.push(next);
            if next == self.eot || tokens.len() > max_positions {
                break;
            }
            let prob = softmax(&logits, candle_core::D::Minus1)?
                .i(next as usize)?
                .to_scalar::<f32>()? as f64;
            sum_logprob += prob.ln();
        }

        let text = self
            .tokenizer
            .decode(&tokens[prompt_len..], true)
            .map_err(candle_core::Error::msg)?;
        Ok(Window {
            text,
            avg_logprob: sum_logprob / tokens.len() as f64,
            no_speech_prob,
        })
    }
}

static ENGINE: OnceLock<Mutex<Option<DictationEngine>>> = OnceLock::new();

fn engine_lock() -> &'static Mutex<Option<DictationEngine>> {
    ENGINE.get_or_init(|| Mutex::new(None))
}

pub fn is_loaded() -> bool {
    engine_lock().lock().map(|g| g.is_some()).unwrap_or(false)
}

pub fn unload() {
    if let Ok(mut guard) = engine_lock().lock() {
        *guard = None;
    }
}

/// Transcribe 16 kHz mono PCM with the selected model, loading it on first use.
/// Blocking — call from `spawn_blocking`.
pub fn transcribe(app_data: &Path, model_id: &str, pcm: &[f32]) -> Result<String, String> {
    if pcm.len() > MAX_SAMPLES {
        return Err("Recording is too long (max 5 minutes).".to_string());
    }
    let opt = option(model_id)?;
    if !is_downloaded(app_data, &opt) {
        return Err("Dictation model is not downloaded.".to_string());
    }

    let mut guard = engine_lock()
        .lock()
        .map_err(|e| format!("Dictation engine lock: {}", e))?;
    if guard.as_ref().is_none_or(|e| e.model_id != opt.id) {
        *guard = None;
        *guard = Some(DictationEngine::load(
            &opt.id,
            &model_dir(app_data, &opt.id),
            opt.quantized,
        )?);
    }
    guard
        .as_mut()
        .ok_or("Dictation engine not loaded")?
        .transcribe(pcm)
}

/// The selected dictation model id, if any.
pub fn selected_model(db: &crate::db::Database) -> Option<String> {
    db.get_setting(SETTING_MODEL_ID)
        .ok()
        .flatten()
        .filter(|id| !id.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mel_filters_match_whisper_shape() {
        // 80 mel bins × (N_FFT / 2 + 1) frequency bins.
        let filters = mel_filters(80).unwrap();
        assert_eq!(filters.len(), 80 * (wh::N_FFT / 2 + 1));
        assert!(filters.iter().all(|v| v.is_finite() && *v >= 0.0));
        assert!(mel_filters(128).is_err());
    }

    #[test]
    fn catalog_default_is_small_and_files_are_complete() {
        let options = get_options();
        assert_eq!(options[0].id, "whisper-tiny-en-q8");
        assert!(options[0].size_bytes < 100_000_000);
        for opt in &options {
            let names: Vec<&str> = opt.files.iter().map(|(_, n)| n.as_str()).collect();
            assert!(names.contains(&"config.json"));
            assert!(names.contains(&"tokenizer.json"));
            let weights = if opt.quantized { "model.gguf" } else { "model.safetensors" };
            assert!(names.contains(&weights), "{} lacks {}", opt.id, weights);
        }
    }

    #[test]
    fn transcribe_requires_a_downloaded_model() {
        let tmp = tempfile::tempdir().unwrap();
        let err = transcribe(tmp.path(), "whisper-tiny-en-q8", &[0.0; 16_000]).unwrap_err();
        assert!(err.contains("not downloaded"));
        assert!(transcribe(tmp.path(), "nope", &[]).is_err());
        let too_long = vec![0.0; MAX_SAMPLES + 1];
        assert!(transcribe(tmp.path(), "whisper-tiny-en-q8", &too_long).is_err());
    }

    #[test]
    fn status_and_delete_track_the_selected_model() {
        let tmp = tempfile::tempdir().unwrap();
        let db = crate::db::Database::new(tmp.path().join("t.db").to_str().unwrap()).unwrap();
        db.initialize().unwrap();
        assert!(!status(tmp.path(), &db).downloaded);

        let opt = option("whisper-tiny-en-q8").unwrap();
        let dir = model_dir(tmp.path(), &opt.id);
        std::fs::create_dir_all(&dir).unwrap();
        for (_, name) in &opt.files {
            std::fs::write(dir.join(name), b"x").unwrap();
        }
        db.set_setting(SETTING_MODEL_ID, &opt.id).unwrap();
        let s = status(tmp.path(), &db);
        assert!(s.downloaded);
        assert_eq!(s.model_id.as_deref(), Some("whisper-tiny-en-q8"));

        delete(tmp.path(), &db).unwrap();
        assert!(!dir.exists());
        assert!(selected_model(&db).is_none());
    }

    /// A tiny random-weight Whisper with a word-level tokenizer holding the
    /// special tokens, to run the real encode → greedy-decode → detokenize path.
    fn tiny_engine() -> DictationEngine {
        use candle_nn::{VarBuilder, VarMap};
        use std::collections::HashMap;
        use tokenizers::models::wordlevel::WordLevel;
        use tokenizers::pre_tokenizers::whitespace::Whitespace;

        let specials = [
            wh::EOT_TOKEN,
            wh::SOT_TOKEN,
            wh::TRANSCRIBE_TOKEN,
            wh::NO_TIMESTAMPS_TOKEN,
            "<|nospeech|>",
        ];
        let vocab_size = 40;
        let mut words: Vec<String> = specials.iter().map(|s| s.to_string()).collect();
        words.extend((words.len()..vocab_size).map(|i| format!("w{i}")));
        let vocab: HashMap<String, u32> =
            words.into_iter().enumerate().map(|(i, w)| (w, i as u32)).collect();
        let wordlevel = WordLevel::builder()
            .vocab(vocab.into_iter().collect())
            .unk_token("w5".to_string())
            .build()
            .unwrap();
        let mut tokenizer = Tokenizer::new(wordlevel);
        tokenizer.with_pre_tokenizer(Some(Whitespace {}));

        let config = Config {
            num_mel_bins: 80,
            max_source_positions: 1500,
            d_model: 16,
            encoder_attention_heads: 2,
            encoder_layers: 1,
            vocab_size,
            max_target_positions: 16,
            decoder_attention_heads: 2,
            decoder_layers: 1,
            suppress_tokens: vec![],
        };
        let device = Device::Cpu;
        let varmap = VarMap::new();
        let vb = VarBuilder::from_varmap(&varmap, wh::DTYPE, &device);
        let model = wh::model::Whisper::load(&vb, config).unwrap();
        DictationEngine::from_parts("test", WhisperModel::Normal(model), tokenizer, device).unwrap()
    }

    #[test]
    fn transcribes_end_to_end_with_tiny_whisper() {
        let mut engine = tiny_engine();
        // 3 s of a 440 Hz tone at 16 kHz.
        let pcm: Vec<f32> = (0..48_000)
            .map(|i| (i as f32 * 440.0 * std::f32::consts::TAU / 16_000.0).sin() * 0.3)
            .collect();
        let first = engine.transcribe(&pcm).unwrap();
        let second = engine.transcribe(&pcm).unwrap();
        // Random weights produce arbitrary words, but decoding must run and be deterministic.
        assert_eq!(first, second);
        assert!(!first.contains("<|"), "special tokens must be stripped: {first}");
    }

    #[test]
    fn very_short_audio_yields_empty_text() {
        let mut engine = tiny_engine();
        assert_eq!(engine.transcribe(&[0.0; 800]).unwrap(), "");
    }
}
