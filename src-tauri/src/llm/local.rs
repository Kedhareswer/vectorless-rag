//! Local model management: download, load, and run small GGUF models.
//!
//! # Inference architecture
//!
//! Uses **candle** for in-process GGUF inference — no external binaries, no
//! sidecar processes, no network dependencies at runtime. Everything runs
//! inside the Tauri process on the CPU.
//!
//! ## What gets downloaded
//!
//! When the user clicks "Download" in the Settings → Local Model dialog:
//!   1. The GGUF model file (default: Liquid LFM2.5-230M, ~150 MB)
//!   2. Its tokenizer.json from the model's HuggingFace repo, saved as
//!      `<model-stem>.tokenizer.json` so models never share a tokenizer
//!
//! Both are saved to `<app-data>/models/`.
//!
//! ## Lifecycle
//!
//!   1. `download_model()` — downloads GGUF + tokenizer, sends progress events.
//!   2. `load_engine(model_path)` — loads model into candle, ready for inference.
//!   3. `chat_inference(system, user, max_tokens)` — in-process generation.
//!   4. `unload_engine()` — frees model memory.
//!
//! If no engine is loaded, `is_engine_loaded()` returns false and enrichment
//! is skipped (non-fatal: pipeline continues with heuristic term extraction).

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::sync::mpsc;

use super::slm;

// ── Engine delegation ─────────────────────────────────────────────────

/// Returns true if the candle SLM engine is loaded and ready for inference.
pub fn is_engine_loaded() -> bool {
    slm::is_engine_loaded()
}

/// Tokenizer file for a model: `<model-stem>.tokenizer.json` beside the GGUF.
/// Installs from before per-model tokenizers used a shared `tokenizer.json`,
/// which is still accepted when the per-model file is missing.
pub fn tokenizer_path_for(model_path: &Path) -> PathBuf {
    let dir = model_path.parent().unwrap_or(Path::new("."));
    let stem = model_path.file_stem().and_then(|s| s.to_str()).unwrap_or("model");
    let own = dir.join(format!("{}.tokenizer.json", stem));
    let legacy = dir.join("tokenizer.json");
    if !own.exists() && legacy.exists() { legacy } else { own }
}

/// Load the SLM engine from a downloaded GGUF model and its tokenizer.
/// No-op if already loaded.
pub fn load_engine(model_path: &str) -> Result<(), String> {
    let tokenizer_path = tokenizer_path_for(Path::new(model_path));

    if !tokenizer_path.exists() {
        return Err(format!(
            "Tokenizer not found at {}. Re-download the model to fix this.",
            tokenizer_path.display()
        ));
    }

    slm::load_engine(model_path, tokenizer_path.to_str().unwrap_or(""))
}

/// Unload the SLM engine and free model memory.
pub fn unload_engine() {
    slm::unload_engine()
}

/// Replace whatever model is loaded with `model_path` (e.g. after a download).
/// Blocking — reads the weights from disk.
pub fn switch_engine(model_path: &str) -> Result<(), String> {
    slm::unload_engine();
    load_engine(model_path)
}

/// Run a chat-style completion against the loaded SLM.
/// Blocking — must not be called from async context directly.
pub fn chat_inference(system: &str, user: &str, max_tokens: u32) -> Result<String, String> {
    slm::chat_inference(system, user, max_tokens)
}

/// Like `chat_inference`, but also reports token counts and can be cancelled.
/// Blocking — must not be called from async context directly.
pub fn generate(
    system: &str,
    user: &str,
    max_tokens: u32,
    cancel: Option<&std::sync::atomic::AtomicBool>,
) -> Result<slm::Generation, String> {
    slm::generate(system, user, max_tokens, cancel)
}

// ── Directory helpers ─────────────────────────────────────────────────

/// Resolve the legacy binary directory (kept for clear_app_data cleanup).
pub fn bin_dir(app_data: &Path) -> PathBuf {
    app_data.join("bin")
}

/// Resolve the models directory.
pub fn models_dir(app_data: &Path) -> PathBuf {
    app_data.join("models")
}

// ── Model options + status ────────────────────────────────────────────

/// A downloadable model option presented to the user.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ModelOption {
    pub id: String,
    pub name: String,
    pub description: String,
    pub size_bytes: u64,
    pub size_label: String,
    pub url: String,
    pub filename: String,
    pub sha256: Option<String>,
    /// URL for the HuggingFace tokenizer.json (same model family).
    pub tokenizer_url: String,
}

/// Status of a downloaded model.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LocalModelStatus {
    pub downloaded: bool,
    pub model_id: Option<String>,
    pub model_path: Option<String>,
    pub size_bytes: Option<u64>,
    /// Whether the tokenizer.json is also present alongside the model.
    pub tokenizer_ready: bool,
}

/// Progress updates sent during download.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DownloadProgress {
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub percent: f64,
    pub done: bool,
    pub error: Option<String>,
    /// Human-readable label for what is currently being downloaded.
    pub phase: String,
}

/// Available models for download. The first entry is the recommended default.
///
/// LFM2.5 (Liquid AI, 2026) replaces Qwen2.5 as the default: smaller and much
/// better at following instructions (IFEval 71.7 for 230M, 77.0 for 350M vs
/// ~42 for Qwen2.5-0.5B, per published model cards). The QAD Q4_0 files are
/// Liquid's quantization-aware-distilled 4-bit checkpoints, which keep ~97% of
/// full-precision accuracy. Qwen2.5 stays available for existing users.
pub fn get_model_options() -> Vec<ModelOption> {
    vec![
        ModelOption {
            id: "lfm2.5-230m-qad-q4".to_string(),
            name: "LFM2.5 230M".to_string(),
            description: "Smallest and fastest. Recommended for query enrichment.".to_string(),
            size_bytes: 150_000_000,
            size_label: "~150 MB".to_string(),
            url: "https://huggingface.co/LiquidAI/LFM2.5-230M-GGUF/resolve/main/LFM2.5-230M-QAD-Q4_0.gguf".to_string(),
            filename: "LFM2.5-230M-QAD-Q4_0.gguf".to_string(),
            sha256: None,
            tokenizer_url: "https://huggingface.co/LiquidAI/LFM2.5-230M/resolve/main/tokenizer.json".to_string(),
        },
        ModelOption {
            id: "lfm2.5-350m-qad-q4".to_string(),
            name: "LFM2.5 350M".to_string(),
            description: "Better summaries and instruction following, still small.".to_string(),
            size_bytes: 220_000_000,
            size_label: "~220 MB".to_string(),
            url: "https://huggingface.co/LiquidAI/LFM2.5-350M-GGUF/resolve/main/LFM2.5-350M-QAD-Q4_0.gguf".to_string(),
            filename: "LFM2.5-350M-QAD-Q4_0.gguf".to_string(),
            sha256: None,
            tokenizer_url: "https://huggingface.co/LiquidAI/LFM2.5-350M/resolve/main/tokenizer.json".to_string(),
        },
        ModelOption {
            id: "qwen2.5-0.5b-q4".to_string(),
            name: "Qwen2.5 0.5B (Q4)".to_string(),
            description: "Previous default. Larger and weaker than LFM2.5.".to_string(),
            size_bytes: 491_400_032,
            size_label: "~491 MB".to_string(),
            url: "https://huggingface.co/Qwen/Qwen2.5-0.5B-Instruct-GGUF/resolve/main/qwen2.5-0.5b-instruct-q4_k_m.gguf".to_string(),
            filename: "qwen2.5-0.5b-instruct-q4_k_m.gguf".to_string(),
            sha256: None,
            tokenizer_url: "https://huggingface.co/Qwen/Qwen2.5-0.5B-Instruct/resolve/main/tokenizer.json".to_string(),
        },
    ]
}

// ── Model download ────────────────────────────────────────────────────

/// Download the model's tokenizer to `dest` if not already present.
async fn download_tokenizer(
    dest: PathBuf,
    tokenizer_url: &str,
    progress_tx: &mpsc::UnboundedSender<DownloadProgress>,
) -> Result<PathBuf, String> {
    if dest.exists() {
        let _ = progress_tx.send(DownloadProgress {
            downloaded_bytes: 0,
            total_bytes: 0,
            percent: 100.0,
            done: false,
            error: None,
            phase: "Tokenizer already available".to_string(),
        });
        return Ok(dest);
    }

    let _ = progress_tx.send(DownloadProgress {
        downloaded_bytes: 0,
        total_bytes: 0,
        percent: 0.0,
        done: false,
        error: None,
        phase: "Downloading tokenizer...".to_string(),
    });

    let client = reqwest::Client::builder()
        .user_agent("TGG-App/1.0")
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    let response = client
        .get(tokenizer_url)
        .send()
        .await
        .map_err(|e| format!("Tokenizer download failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!(
            "Tokenizer download failed (HTTP {})",
            response.status()
        ));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("Tokenizer read error: {}", e))?;

    std::fs::write(&dest, &bytes)
        .map_err(|e| format!("Failed to write tokenizer: {}", e))?;

    let _ = progress_tx.send(DownloadProgress {
        downloaded_bytes: bytes.len() as u64,
        total_bytes: bytes.len() as u64,
        percent: 100.0,
        done: false,
        error: None,
        phase: "Tokenizer downloaded".to_string(),
    });

    Ok(dest)
}

/// Check if a local model is already downloaded and return its status.
pub fn check_local_model(app_data: &Path, db: &crate::db::Database) -> LocalModelStatus {
    let model_id = db.get_setting("local_model_id").ok().flatten();
    let model_path = db.get_setting("local_model_path").ok().flatten();

    if let (Some(id), Some(path)) = (&model_id, &model_path) {
        let p = Path::new(path);
        if p.exists() {
            let size = std::fs::metadata(p).map(|m| m.len()).ok();
            let tokenizer_ready = tokenizer_path_for(p).exists();
            return LocalModelStatus {
                downloaded: true,
                model_id: Some(id.clone()),
                model_path: Some(path.clone()),
                size_bytes: size,
                tokenizer_ready,
            };
        }
    }

    // Scan models dir for any .gguf files (in case settings got cleared)
    let dir = models_dir(app_data);
    if dir.exists()
        && let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("gguf") {
                    let size = std::fs::metadata(&path).map(|m| m.len()).ok();
                    let found_id = path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("unknown")
                        .to_string();
                    let tokenizer_ready = tokenizer_path_for(&path).exists();
                    return LocalModelStatus {
                        downloaded: true,
                        model_id: Some(found_id),
                        model_path: Some(path.to_string_lossy().to_string()),
                        size_bytes: size,
                        tokenizer_ready,
                    };
                }
            }
        }

    LocalModelStatus {
        downloaded: false,
        model_id: None,
        model_path: None,
        size_bytes: None,
        tokenizer_ready: false,
    }
}

/// Download a model by ID plus its tokenizer.
/// Sends progress updates through the channel.
/// Returns the path to the downloaded GGUF file on success.
pub async fn download_model(
    app_data: &Path,
    model_id: &str,
    progress_tx: mpsc::UnboundedSender<DownloadProgress>,
) -> Result<PathBuf, String> {
    let options = get_model_options();
    let model = options
        .iter()
        .find(|m| m.id == model_id)
        .ok_or_else(|| format!("Unknown model ID: {}", model_id))?;

    let dir = models_dir(app_data);
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Failed to create models directory: {}", e))?;

    let dest = dir.join(&model.filename);

    // ── Phase 1: Download the model's tokenizer ──
    let tokenizer_dest = dir.join(format!(
        "{}.tokenizer.json",
        dest.file_stem().and_then(|s| s.to_str()).unwrap_or("model")
    ));
    download_tokenizer(tokenizer_dest, &model.tokenizer_url, &progress_tx)
        .await
        .map_err(|e| {
            let _ = progress_tx.send(DownloadProgress {
                downloaded_bytes: 0,
                total_bytes: 0,
                percent: 0.0,
                done: false,
                error: Some(format!("Tokenizer download failed: {}", e)),
                phase: "Error".to_string(),
            });
            e
        })?;

    // ── Phase 2: Download the GGUF model file ──

    // Skip if already exists and non-empty
    if dest.exists()
        && let Ok(meta) = std::fs::metadata(&dest)
            && meta.len() > 0 {
                let _ = progress_tx.send(DownloadProgress {
                    downloaded_bytes: meta.len(),
                    total_bytes: meta.len(),
                    percent: 100.0,
                    done: true,
                    error: None,
                    phase: "Complete".to_string(),
                });
                return Ok(dest);
            }

    let _ = progress_tx.send(DownloadProgress {
        downloaded_bytes: 0,
        total_bytes: 0,
        percent: 0.0,
        done: false,
        error: None,
        phase: format!("Downloading {}...", model.name),
    });

    let client = reqwest::Client::new();
    let response = client
        .get(&model.url)
        .send()
        .await
        .map_err(|e| format!("Download request failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!(
            "Download failed with status: {}",
            response.status()
        ));
    }

    let total = response.content_length().unwrap_or(model.size_bytes);
    let mut downloaded: u64 = 0;

    let tmp_dest = dir.join(format!("{}.tmp", model.filename));
    let mut file = tokio::fs::File::create(&tmp_dest)
        .await
        .map_err(|e| format!("Failed to create file: {}", e))?;

    use futures_util::StreamExt;
    use tokio::io::AsyncWriteExt;

    let mut stream = response.bytes_stream();
    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result.map_err(|e| format!("Download error: {}", e))?;
        file.write_all(&chunk)
            .await
            .map_err(|e| format!("Write error: {}", e))?;
        downloaded += chunk.len() as u64;
        let percent = if total > 0 {
            (downloaded as f64 / total as f64) * 100.0
        } else {
            0.0
        };
        let _ = progress_tx.send(DownloadProgress {
            downloaded_bytes: downloaded,
            total_bytes: total,
            percent,
            done: false,
            error: None,
            phase: format!("Downloading {}...", model.name),
        });
    }

    file.flush()
        .await
        .map_err(|e| format!("Flush error: {}", e))?;
    file.shutdown()
        .await
        .map_err(|e| format!("Shutdown error: {}", e))?;
    drop(file);

    // On Windows, the OS may not release the file handle immediately.
    #[cfg(target_os = "windows")]
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Rename tmp -> final. Fallback to copy+delete on Windows if rename fails.
    if let Err(rename_err) = tokio::fs::rename(&tmp_dest, &dest).await {
        eprintln!(
            "Rename failed ({}), falling back to copy+delete",
            rename_err
        );
        tokio::fs::copy(&tmp_dest, &dest)
            .await
            .map_err(|e| {
                format!(
                    "Failed to copy downloaded file: {} (original rename error: {})",
                    e, rename_err
                )
            })?;
        let _ = tokio::fs::remove_file(&tmp_dest).await;
    }

    let _ = progress_tx.send(DownloadProgress {
        downloaded_bytes: downloaded,
        total_bytes: total,
        percent: 100.0,
        done: true,
        error: None,
        phase: "Complete".to_string(),
    });

    Ok(dest)
}

/// Delete the downloaded model, unload the engine, and clear settings.
pub fn delete_local_model(app_data: &Path, db: &crate::db::Database) -> Result<(), String> {
    unload_engine();

    // Delete GGUF model files
    if let Ok(Some(path)) = db.get_setting("local_model_path") {
        let p = Path::new(&path);
        if p.exists() {
            std::fs::remove_file(p).map_err(|e| format!("Failed to delete model: {}", e))?;
        }
    }
    let mdir = models_dir(app_data);
    if let Ok(entries) = std::fs::read_dir(&mdir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            // Models, their tokenizers, and the legacy shared tokenizer.json.
            if name.ends_with(".gguf") || name.ends_with("tokenizer.json") {
                let _ = std::fs::remove_file(&path);
            }
        }
    }

    let _ = db.set_setting("local_model_id", "");
    let _ = db.set_setting("local_model_path", "");
    Ok(())
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_model_options_returns_valid_entries() {
        let options = get_model_options();
        assert!(options.len() >= 2);
        for opt in &options {
            assert!(!opt.id.is_empty());
            assert!(!opt.url.is_empty());
            assert!(!opt.filename.is_empty());
            assert!(opt.size_bytes > 0);
            assert!(opt.filename.ends_with(".gguf"));
            assert!(!opt.tokenizer_url.is_empty());
        }
    }

    #[test]
    fn default_model_is_small_lfm2_5() {
        let first = &get_model_options()[0];
        assert_eq!(first.id, "lfm2.5-230m-qad-q4");
        assert!(first.size_bytes < 200_000_000);
    }

    #[test]
    fn model_options_have_unique_ids_and_filenames() {
        let options = get_model_options();
        let ids: std::collections::HashSet<_> = options.iter().map(|o| &o.id).collect();
        let files: std::collections::HashSet<_> = options.iter().map(|o| &o.filename).collect();
        assert_eq!(ids.len(), options.len());
        assert_eq!(files.len(), options.len());
    }

    #[test]
    fn tokenizer_path_prefers_per_model_file_over_legacy() {
        let tmp = tempfile::tempdir().unwrap();
        let model = tmp.path().join("LFM2.5-230M-QAD-Q4_0.gguf");
        let own = tmp.path().join("LFM2.5-230M-QAD-Q4_0.tokenizer.json");
        let legacy = tmp.path().join("tokenizer.json");

        // Nothing on disk: points at the per-model file (where downloads go).
        assert_eq!(tokenizer_path_for(&model), own);
        // Only the legacy shared file: used for older installs.
        std::fs::write(&legacy, b"{}").unwrap();
        assert_eq!(tokenizer_path_for(&model), legacy);
        // Both: the model's own tokenizer wins.
        std::fs::write(&own, b"{}").unwrap();
        assert_eq!(tokenizer_path_for(&model), own);
    }

    #[test]
    fn delete_removes_per_model_tokenizers() {
        let tmp = tempfile::tempdir().unwrap();
        let db =
            crate::db::Database::new(tmp.path().join("test.db").to_str().unwrap()).unwrap();
        db.initialize().unwrap();
        let models = models_dir(tmp.path());
        std::fs::create_dir_all(&models).unwrap();
        std::fs::write(models.join("a.gguf"), b"x").unwrap();
        std::fs::write(models.join("a.tokenizer.json"), b"{}").unwrap();
        std::fs::write(models.join("notes.txt"), b"keep").unwrap();

        delete_local_model(tmp.path(), &db).unwrap();

        assert!(!models.join("a.gguf").exists());
        assert!(!models.join("a.tokenizer.json").exists());
        assert!(models.join("notes.txt").exists());
    }

    #[test]
    fn is_engine_loaded_false_initially() {
        assert!(!is_engine_loaded());
    }

    #[test]
    fn check_local_model_no_model() {
        let tmp = tempfile::tempdir().unwrap();
        let db =
            crate::db::Database::new(tmp.path().join("test.db").to_str().unwrap()).unwrap();
        db.initialize().unwrap();
        let status = check_local_model(tmp.path(), &db);
        assert!(!status.downloaded);
        assert!(status.model_id.is_none());
        assert!(status.model_path.is_none());
    }

    #[test]
    fn check_local_model_finds_gguf_file() {
        let tmp = tempfile::tempdir().unwrap();
        let db =
            crate::db::Database::new(tmp.path().join("test.db").to_str().unwrap()).unwrap();
        db.initialize().unwrap();
        let models = models_dir(tmp.path());
        std::fs::create_dir_all(&models).unwrap();
        std::fs::write(models.join("test-model.gguf"), b"fake model data").unwrap();
        let status = check_local_model(tmp.path(), &db);
        assert!(status.downloaded);
        assert!(status.model_path.is_some());
        assert!(!status.tokenizer_ready);
    }

    #[test]
    fn check_local_model_detects_tokenizer() {
        let tmp = tempfile::tempdir().unwrap();
        let db =
            crate::db::Database::new(tmp.path().join("test.db").to_str().unwrap()).unwrap();
        db.initialize().unwrap();
        let models = models_dir(tmp.path());
        std::fs::create_dir_all(&models).unwrap();
        std::fs::write(models.join("test-model.gguf"), b"fake").unwrap();
        std::fs::write(models.join("tokenizer.json"), b"{}").unwrap();
        let status = check_local_model(tmp.path(), &db);
        assert!(status.downloaded);
        assert!(status.tokenizer_ready);
    }

    #[test]
    fn delete_local_model_removes_file_and_clears_settings() {
        let tmp = tempfile::tempdir().unwrap();
        let db =
            crate::db::Database::new(tmp.path().join("test.db").to_str().unwrap()).unwrap();
        db.initialize().unwrap();
        let models = models_dir(tmp.path());
        std::fs::create_dir_all(&models).unwrap();
        let model_path = models.join("test-model.gguf");
        std::fs::write(&model_path, b"fake").unwrap();
        std::fs::write(models.join("tokenizer.json"), b"{}").unwrap();
        db.set_setting("local_model_path", model_path.to_str().unwrap())
            .unwrap();
        db.set_setting("local_model_id", "test-model").unwrap();

        delete_local_model(tmp.path(), &db).unwrap();

        assert!(!model_path.exists());
        assert!(!models.join("tokenizer.json").exists());
    }
}
