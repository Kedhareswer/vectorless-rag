//! Local model management: download and run small GGUF models for enrichment.
//!
//! # Inference architecture
//!
//! Uses a **sidecar process** — `llama-server` from the llama.cpp project —
//! downloaded automatically alongside the GGUF model on first use. No Ollama,
//! no manual setup, no prerequisites. Everything lives in the app data directory.
//!
//! ## What gets downloaded
//!
//! When the user clicks "Download" in the Settings → Local Model dialog:
//!   1. The GGUF model file (e.g. Qwen2.5 0.5B Q4, ~491 MB)
//!   2. The `llama-server` binary for the current platform (~15-30 MB ZIP)
//!
//! Both are saved to `<app-data>/models/` (model) and `<app-data>/bin/` (binary).
//!
//! ## Lifecycle
//!
//!   1. `download_model()` — downloads model + binary, sends progress events.
//!   2. `start_sidecar(model_path)` — spawns llama-server, waits for /health.
//!   3. `chat_inference(system, user, max_tokens)` — POST to /completion.
//!   4. `stop_sidecar()` — kills child process.
//!
//! If no sidecar is running, `is_sidecar_running()` returns false and enrichment
//! is skipped (non-fatal: pipeline continues with heuristic term extraction).

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use tokio::sync::mpsc;

// ── Sidecar process state ─────────────────────────────────────────────────────

struct SidecarState {
    child: std::process::Child,
    port: u16,
}

static SIDECAR: OnceLock<Mutex<Option<SidecarState>>> = OnceLock::new();

fn sidecar_lock() -> &'static Mutex<Option<SidecarState>> {
    SIDECAR.get_or_init(|| Mutex::new(None))
}

/// Returns true if the llama-server sidecar is currently running.
pub fn is_sidecar_running() -> bool {
    sidecar_lock()
        .lock()
        .map(|g| g.is_some())
        .unwrap_or(false)
}

/// Find a free TCP port.
fn find_free_port() -> u16 {
    use std::net::TcpListener;
    TcpListener::bind("127.0.0.1:0")
        .map(|l| l.local_addr().map(|a| a.port()).unwrap_or(8765))
        .unwrap_or(8765)
}

/// Resolve the directory where we store the downloaded llama-server binary.
pub fn bin_dir(app_data: &Path) -> PathBuf {
    app_data.join("bin")
}

/// Resolve the models directory.
pub fn models_dir(app_data: &Path) -> PathBuf {
    app_data.join("models")
}

/// Find the llama-server binary in the app-data bin dir (downloaded at runtime)
/// or next to the executable (bundled by developers during testing).
fn resolve_server_binary(app_data: &Path) -> Option<PathBuf> {
    // 1. Downloaded binary in app-data/bin/ (user install path)
    let bin = bin_dir(app_data);
    #[cfg(target_os = "windows")]
    let server_name = "llama-server.exe";
    #[cfg(not(target_os = "windows"))]
    let server_name = "llama-server";

    let downloaded = bin.join(server_name);
    if downloaded.exists() {
        return Some(downloaded);
    }

    // 2. Developer convenience: binary placed next to the executable
    if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.to_path_buf())) {
        let arch = std::env::consts::ARCH;
        let os = std::env::consts::OS;
        let env = if cfg!(target_env = "msvc") { "msvc" } else { "gnu" };
        let triple = match os {
            "windows" => format!("{}-pc-windows-{}", arch, env),
            "macos"   => format!("{}-apple-darwin", arch),
            _         => format!("{}-unknown-linux-gnu", arch),
        };
        for name in &[
            format!("llama-server-{}", triple),
            "llama-server".to_string(),
        ] {
            #[cfg(target_os = "windows")]
            let p = dir.join(format!("{}.exe", name));
            #[cfg(not(target_os = "windows"))]
            let p = dir.join(name);
            if p.exists() {
                return Some(p);
            }
        }
    }

    None
}

/// Check whether the llama-server binary is available (downloaded or bundled).
pub fn is_server_binary_available(app_data: &Path) -> bool {
    resolve_server_binary(app_data).is_some()
}

/// Start the llama-server sidecar with the given GGUF model.
/// Blocks until the server is healthy (up to 30 s) or returns an error.
/// If a sidecar is already running, returns Ok immediately.
pub fn start_sidecar(app_data: &Path, model_path: &str) -> Result<(), String> {
    if is_sidecar_running() {
        return Ok(());
    }
    if !Path::new(model_path).exists() {
        return Err(format!("Model file not found: {}", model_path));
    }
    let bin = resolve_server_binary(app_data)
        .ok_or("llama-server binary not found. Download it via Settings → Local Model.")?;

    let port = find_free_port();

    // On Windows, llama.dll must be in the same directory as llama-server.exe.
    // We set the working directory to the binary's parent so the DLL is found.
    let bin_parent = bin.parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));

    let mut cmd = std::process::Command::new(&bin);
    cmd.current_dir(&bin_parent)
        .args([
            "--model", model_path,
            "--port", &port.to_string(),
            "--ctx-size", "2048",
            "-ngl", "0",         // CPU-only; no GPU required
            "--threads", "4",
            "--log-disable",
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());

    // Hide the console window on Windows
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }

    let child = cmd.spawn()
        .map_err(|e| format!("Failed to start llama-server: {}", e))?;

    // Poll /health until ready (up to 30 s)
    let health_url = format!("http://127.0.0.1:{}/health", port);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_millis(500))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))?;

    loop {
        if std::time::Instant::now() > deadline {
            return Err("llama-server did not become healthy within 30 seconds".to_string());
        }
        if let Ok(resp) = client.get(&health_url).send() {
            if resp.status().is_success() {
                break;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(300));
    }

    let mut guard = sidecar_lock()
        .lock()
        .map_err(|e| format!("Sidecar lock error: {}", e))?;
    *guard = Some(SidecarState { child, port });
    Ok(())
}

/// Stop the llama-server sidecar process if running.
pub fn stop_sidecar() {
    if let Ok(mut guard) = sidecar_lock().lock() {
        if let Some(mut state) = guard.take() {
            let _ = state.child.kill();
            let _ = state.child.wait();
        }
    }
}

/// Run a chat-style completion against the running sidecar.
/// Blocking — must not be called from async context directly.
pub fn chat_inference(system: &str, user: &str, max_tokens: u32) -> Result<String, String> {
    let port = {
        let guard = sidecar_lock()
            .lock()
            .map_err(|e| format!("Sidecar lock: {}", e))?;
        guard.as_ref().map(|s| s.port).ok_or("Local model not running")?
    };

    // ChatML prompt format (Qwen2.5 instruct)
    let prompt = format!(
        "<|im_start|>system\n{}<|im_end|>\n<|im_start|>user\n{}<|im_end|>\n<|im_start|>assistant\n",
        system, user
    );

    let body = serde_json::json!({
        "prompt": prompt,
        "n_predict": max_tokens,
        "temperature": 0.1,
        "top_p": 0.9,
        "stop": ["<|im_end|>", "<|im_start|>"],
        "stream": false
    });

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    let resp = client
        .post(format!("http://127.0.0.1:{}/completion", port))
        .json(&body)
        .send()
        .map_err(|e| format!("Inference request failed: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("Inference error: HTTP {}", resp.status()));
    }

    #[derive(Deserialize)]
    struct CompletionResp { content: String }

    let parsed: CompletionResp = resp
        .json()
        .map_err(|e| format!("Failed to parse inference response: {}", e))?;

    Ok(parsed.content.replace("<|im_end|>", "").trim().to_string())
}

// ── Model + binary download ───────────────────────────────────────────────────

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
}

/// Status of a downloaded model.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LocalModelStatus {
    pub downloaded: bool,
    pub model_id: Option<String>,
    pub model_path: Option<String>,
    pub size_bytes: Option<u64>,
    /// Whether the llama-server binary is also available.
    pub binary_ready: bool,
}

/// Progress updates sent during download. Covers both model and binary phases.
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

/// Available models for download.
pub fn get_model_options() -> Vec<ModelOption> {
    vec![
        ModelOption {
            id: "qwen2.5-0.5b-q4".to_string(),
            name: "Qwen2.5 0.5B (Q4)".to_string(),
            description: "Smallest download, fast enrichment. Recommended.".to_string(),
            size_bytes: 491_400_032,
            size_label: "~491 MB".to_string(),
            url: "https://huggingface.co/Qwen/Qwen2.5-0.5B-Instruct-GGUF/resolve/main/qwen2.5-0.5b-instruct-q4_k_m.gguf".to_string(),
            filename: "qwen2.5-0.5b-instruct-q4_k_m.gguf".to_string(),
            sha256: None,
        },
        ModelOption {
            id: "qwen2.5-0.5b-q8".to_string(),
            name: "Qwen2.5 0.5B (Q8)".to_string(),
            description: "Better quality summaries, moderate download.".to_string(),
            size_bytes: 675_710_816,
            size_label: "~676 MB".to_string(),
            url: "https://huggingface.co/Qwen/Qwen2.5-0.5B-Instruct-GGUF/resolve/main/qwen2.5-0.5b-instruct-q8_0.gguf".to_string(),
            filename: "qwen2.5-0.5b-instruct-q8_0.gguf".to_string(),
            sha256: None,
        },
        ModelOption {
            id: "qwen2.5-1.5b-q4".to_string(),
            name: "Qwen2.5 1.5B (Q4)".to_string(),
            description: "Highest quality enrichment, larger download.".to_string(),
            size_bytes: 1_117_320_736,
            size_label: "~1.1 GB".to_string(),
            url: "https://huggingface.co/Qwen/Qwen2.5-1.5B-Instruct-GGUF/resolve/main/qwen2.5-1.5b-instruct-q4_k_m.gguf".to_string(),
            filename: "qwen2.5-1.5b-instruct-q4_k_m.gguf".to_string(),
            sha256: None,
        },
    ]
}

/// Platform-specific download info for the llama-server binary.
struct ServerBinaryInfo {
    /// Full URL to a ZIP or tar.gz archive containing llama-server.
    url: String,
    /// Filename of the archive to save locally.
    archive_name: String,
    /// Path of llama-server inside the extracted archive (relative).
    server_path_in_archive: String,
    /// Any additional files needed from the archive (e.g. llama.dll on Windows).
    extra_files: Vec<String>,
}

fn server_binary_info() -> Option<ServerBinaryInfo> {
    // Pinned to a recent stable build. CPU-only for maximum compatibility.
    let tag = "b8234";

    match std::env::consts::OS {
        "windows" => Some(ServerBinaryInfo {
            url: format!(
                "https://github.com/ggml-org/llama.cpp/releases/download/{tag}/llama-{tag}-bin-win-cpu-x64.zip"
            ),
            archive_name: format!("llama-{tag}-bin-win-cpu-x64.zip"),
            server_path_in_archive: "llama-server.exe".to_string(),
            // llama.dll is required on Windows — must live next to the executable
            extra_files: vec!["llama.dll".to_string()],
        }),
        "macos" => {
            let arch = std::env::consts::ARCH;
            let suffix = if arch == "aarch64" { "arm64" } else { "x64" };
            Some(ServerBinaryInfo {
                url: format!(
                    "https://github.com/ggml-org/llama.cpp/releases/download/{tag}/llama-{tag}-bin-macos-{suffix}.tar.gz"
                ),
                archive_name: format!("llama-{tag}-bin-macos-{suffix}.tar.gz"),
                server_path_in_archive: "llama-server".to_string(),
                extra_files: vec![],
            })
        }
        "linux" => Some(ServerBinaryInfo {
            url: format!(
                "https://github.com/ggml-org/llama.cpp/releases/download/{tag}/llama-{tag}-bin-ubuntu-x64.tar.gz"
            ),
            archive_name: format!("llama-{tag}-bin-ubuntu-x64.tar.gz"),
            server_path_in_archive: "llama-server".to_string(),
            extra_files: vec![],
        }),
        _ => None,
    }
}

/// Download the llama-server binary for the current platform if not already present.
/// Sends progress events with `phase = "Downloading llama-server"`.
/// Returns the path to the extracted binary.
async fn download_server_binary(
    app_data: &Path,
    progress_tx: &mpsc::UnboundedSender<DownloadProgress>,
) -> Result<PathBuf, String> {
    let info = server_binary_info()
        .ok_or("Unsupported platform for automatic binary download")?;

    let bin = bin_dir(app_data);
    std::fs::create_dir_all(&bin)
        .map_err(|e| format!("Failed to create bin directory: {}", e))?;

    #[cfg(target_os = "windows")]
    let server_name = "llama-server.exe";
    #[cfg(not(target_os = "windows"))]
    let server_name = "llama-server";

    let dest = bin.join(server_name);

    // Skip if already downloaded AND required DLLs are present
    let dlls_ok = {
        #[cfg(target_os = "windows")]
        { bin.join("ggml.dll").exists() || bin.join("ggml-base.dll").exists() }
        #[cfg(not(target_os = "windows"))]
        { true }
    };
    if dest.exists() && dlls_ok {
        let _ = progress_tx.send(DownloadProgress {
            downloaded_bytes: 0, total_bytes: 0, percent: 100.0,
            done: false, error: None,
            phase: "llama-server already available".to_string(),
        });
        return Ok(dest);
    }
    // If binary exists but DLLs are missing, remove stale binary to force re-download
    if dest.exists() && !dlls_ok {
        let _ = std::fs::remove_file(&dest);
    }

    let _ = progress_tx.send(DownloadProgress {
        downloaded_bytes: 0, total_bytes: 0, percent: 0.0,
        done: false, error: None,
        phase: "Downloading llama-server binary...".to_string(),
    });

    // Download the archive
    let client = reqwest::Client::builder()
        .user_agent("TGG-App/1.0")
        .build()
        .map_err(|e| format!("HTTP client error: {}", e))?;

    let response = client
        .get(&info.url)
        .send()
        .await
        .map_err(|e| format!("Binary download failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!(
            "Binary download failed (HTTP {}). Check your internet connection.",
            response.status()
        ));
    }

    let total = response.content_length().unwrap_or(0);
    let mut downloaded: u64 = 0;
    let archive_path = bin.join(&info.archive_name);
    let mut file = tokio::fs::File::create(&archive_path)
        .await
        .map_err(|e| format!("Failed to create archive file: {}", e))?;

    use futures_util::StreamExt;
    use tokio::io::AsyncWriteExt;

    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("Download error: {}", e))?;
        file.write_all(&chunk).await
            .map_err(|e| format!("Write error: {}", e))?;
        downloaded += chunk.len() as u64;
        let percent = if total > 0 { (downloaded as f64 / total as f64) * 100.0 } else { 0.0 };
        let _ = progress_tx.send(DownloadProgress {
            downloaded_bytes: downloaded, total_bytes: total, percent,
            done: false, error: None,
            phase: "Downloading llama-server binary...".to_string(),
        });
    }
    file.flush().await.map_err(|e| format!("Flush error: {}", e))?;
    drop(file);

    // Extract llama-server (and any extra files like llama.dll) from the ZIP
    let _ = progress_tx.send(DownloadProgress {
        downloaded_bytes: downloaded, total_bytes: total, percent: 100.0,
        done: false, error: None,
        phase: "Extracting llama-server...".to_string(),
    });

    if info.archive_name.ends_with(".tar.gz") {
        extract_from_tar_gz(&archive_path, &bin, &info.server_path_in_archive, &info.extra_files)?;
    } else {
        extract_from_zip(&archive_path, &bin, &info.server_path_in_archive, &info.extra_files)?;
    }

    // Make the binary executable on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if dest.exists() {
            let mut perms = std::fs::metadata(&dest)
                .map_err(|e| format!("Permission read error: {}", e))?.permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&dest, perms)
                .map_err(|e| format!("Permission set error: {}", e))?;
        }
    }

    // Remove the archive to save disk space
    let _ = std::fs::remove_file(&archive_path);

    Ok(dest)
}

/// Extract specific files from a ZIP archive into `dest_dir`.
/// Searches all entries for filenames matching (ignoring directory prefix).
fn extract_from_zip(
    archive_path: &Path,
    dest_dir: &Path,
    primary_file: &str,
    extra_files: &[String],
) -> Result<(), String> {
    let file = std::fs::File::open(archive_path)
        .map_err(|e| format!("Failed to open archive: {}", e))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| format!("Failed to read ZIP: {}", e))?;

    // Build set of filenames to extract (basename only)
    let mut targets: Vec<&str> = vec![primary_file];
    for f in extra_files { targets.push(f.as_str()); }

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)
            .map_err(|e| format!("ZIP entry error: {}", e))?;
        let entry_name = entry.name().to_string();
        // Match by basename
        let basename = entry_name.rsplit('/').next().unwrap_or(&entry_name);
        // Extract named targets + ALL .dll files (llama-server needs them at runtime)
        let should_extract = targets.contains(&basename)
            || basename.to_lowercase().ends_with(".dll");
        if should_extract {
            let dest = dest_dir.join(basename);
            let mut out = std::fs::File::create(&dest)
                .map_err(|e| format!("Failed to create {}: {}", basename, e))?;
            std::io::copy(&mut entry, &mut out)
                .map_err(|e| format!("Failed to extract {}: {}", basename, e))?;
        }
    }

    // Verify primary file was extracted
    #[cfg(target_os = "windows")]
    let server_name = "llama-server.exe";
    #[cfg(not(target_os = "windows"))]
    let server_name = "llama-server";

    if !dest_dir.join(server_name).exists() {
        return Err(format!(
            "llama-server not found in archive. The release format may have changed. \
             Try downloading manually from https://github.com/ggml-org/llama.cpp/releases"
        ));
    }

    Ok(())
}

/// Extract specific files from a `.tar.gz` archive into `dest_dir`.
/// Searches all entries for filenames matching by basename.
fn extract_from_tar_gz(
    archive_path: &Path,
    dest_dir: &Path,
    primary_file: &str,
    extra_files: &[String],
) -> Result<(), String> {
    let file = std::fs::File::open(archive_path)
        .map_err(|e| format!("Failed to open archive: {}", e))?;
    let gz = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(gz);

    let mut targets: Vec<&str> = vec![primary_file];
    for f in extra_files { targets.push(f.as_str()); }

    for entry in archive.entries().map_err(|e| format!("tar entries error: {}", e))? {
        let mut entry = entry.map_err(|e| format!("tar entry error: {}", e))?;
        let entry_path = entry.path().map_err(|e| format!("tar path error: {}", e))?;
        let entry_name = entry_path.to_string_lossy().to_string();
        let basename = entry_name.rsplit('/').next().unwrap_or(&entry_name);
        // Extract named targets + ALL shared libraries (.dll/.so/.dylib)
        let should_extract = targets.contains(&basename)
            || basename.to_lowercase().ends_with(".dll")
            || basename.to_lowercase().ends_with(".so")
            || basename.to_lowercase().ends_with(".dylib");
        if should_extract {
            let dest = dest_dir.join(basename);
            entry.unpack(&dest)
                .map_err(|e| format!("Failed to extract {}: {}", basename, e))?;
        }
    }

    #[cfg(not(target_os = "windows"))]
    let server_name = "llama-server";
    #[cfg(target_os = "windows")]
    let server_name = "llama-server.exe";

    if !dest_dir.join(server_name).exists() {
        return Err(
            "llama-server not found in archive. The release format may have changed. \
             Try downloading manually from https://github.com/ggml-org/llama.cpp/releases"
            .to_string()
        );
    }

    Ok(())
}

/// Check if a local model is already downloaded and return its status.
pub fn check_local_model(app_data: &Path, db: &crate::db::Database) -> LocalModelStatus {
    let binary_ready = is_server_binary_available(app_data);
    let model_id = db.get_setting("local_model_id").ok().flatten();
    let model_path = db.get_setting("local_model_path").ok().flatten();

    if let (Some(ref id), Some(ref path)) = (&model_id, &model_path) {
        let p = Path::new(path);
        if p.exists() {
            let size = std::fs::metadata(p).map(|m| m.len()).ok();
            return LocalModelStatus {
                downloaded: true,
                model_id: Some(id.clone()),
                model_path: Some(path.clone()),
                size_bytes: size,
                binary_ready,
            };
        }
    }

    // Scan models dir for any .gguf files (in case settings got cleared)
    let dir = models_dir(app_data);
    if dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("gguf") {
                    let size = std::fs::metadata(&path).map(|m| m.len()).ok();
                    let found_id = path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("unknown")
                        .to_string();
                    return LocalModelStatus {
                        downloaded: true,
                        model_id: Some(found_id),
                        model_path: Some(path.to_string_lossy().to_string()),
                        size_bytes: size,
                        binary_ready,
                    };
                }
            }
        }
    }

    LocalModelStatus {
        downloaded: false,
        model_id: None,
        model_path: None,
        size_bytes: None,
        binary_ready,
    }
}

/// Download a model by ID plus the llama-server binary.
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

    // ── Phase 1: Download llama-server binary ──
    download_server_binary(app_data, &progress_tx).await
        .map_err(|e| {
            let _ = progress_tx.send(DownloadProgress {
                downloaded_bytes: 0, total_bytes: 0, percent: 0.0,
                done: false,
                error: Some(format!("Binary download failed: {}", e)),
                phase: "Error".to_string(),
            });
            e
        })?;

    // ── Phase 2: Download the GGUF model file ──

    // Skip if already exists and non-empty
    if dest.exists() {
        if let Ok(meta) = std::fs::metadata(&dest) {
            if meta.len() > 0 {
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
        }
    }

    let _ = progress_tx.send(DownloadProgress {
        downloaded_bytes: 0, total_bytes: 0, percent: 0.0,
        done: false, error: None,
        phase: format!("Downloading {}...", model.name),
    });

    let client = reqwest::Client::new();
    let response = client
        .get(&model.url)
        .send()
        .await
        .map_err(|e| format!("Download request failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Download failed with status: {}", response.status()));
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
        file.write_all(&chunk).await
            .map_err(|e| format!("Write error: {}", e))?;
        downloaded += chunk.len() as u64;
        let percent = if total > 0 { (downloaded as f64 / total as f64) * 100.0 } else { 0.0 };
        let _ = progress_tx.send(DownloadProgress {
            downloaded_bytes: downloaded, total_bytes: total, percent,
            done: false, error: None,
            phase: format!("Downloading {}...", model.name),
        });
    }

    file.flush().await.map_err(|e| format!("Flush error: {}", e))?;
    // Explicitly shutdown the async writer to ensure the OS file handle is fully released
    file.shutdown().await.map_err(|e| format!("Shutdown error: {}", e))?;
    drop(file);

    // On Windows, the OS may not release the file handle immediately after drop.
    // Give it a moment before attempting the rename.
    #[cfg(target_os = "windows")]
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Rename tmp -> final. If rename fails (common on Windows due to antivirus locks),
    // fall back to copy + delete.
    if let Err(rename_err) = tokio::fs::rename(&tmp_dest, &dest).await {
        eprintln!("Rename failed ({}), falling back to copy+delete", rename_err);
        tokio::fs::copy(&tmp_dest, &dest)
            .await
            .map_err(|e| format!("Failed to copy downloaded file: {} (original rename error: {})", e, rename_err))?;
        let _ = tokio::fs::remove_file(&tmp_dest).await;
    }

    let _ = progress_tx.send(DownloadProgress {
        downloaded_bytes: downloaded, total_bytes: total, percent: 100.0,
        done: true, error: None,
        phase: "Complete".to_string(),
    });

    Ok(dest)
}

/// Delete the downloaded model and binary, stop any running sidecar, clear settings.
pub fn delete_local_model(app_data: &Path, db: &crate::db::Database) -> Result<(), String> {
    stop_sidecar();

    // Delete GGUF model files
    if let Ok(Some(path)) = db.get_setting("local_model_path") {
        let p = Path::new(&path);
        if p.exists() {
            std::fs::remove_file(p).map_err(|e| format!("Failed to delete model: {}", e))?;
        }
    }
    let mdir = models_dir(app_data);
    if mdir.exists() {
        if let Ok(entries) = std::fs::read_dir(&mdir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("gguf") {
                    let _ = std::fs::remove_file(&path);
                }
            }
        }
    }

    // Keep the llama-server binary — no need to re-download it when switching models.
    // It is only removed by clear_app_data (full reset).

    let _ = db.set_setting("local_model_id", "");
    let _ = db.set_setting("local_model_path", "");
    Ok(())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

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
        }
    }

    #[test]
    fn is_sidecar_running_false_in_tests() {
        assert!(!is_sidecar_running());
    }

    #[test]
    fn find_free_port_returns_nonzero() {
        let port = find_free_port();
        assert!(port > 0);
    }

    #[test]
    fn check_local_model_no_model() {
        let tmp = tempfile::tempdir().unwrap();
        let db = crate::db::Database::new(tmp.path().join("test.db").to_str().unwrap()).unwrap();
        db.initialize().unwrap();
        let status = check_local_model(tmp.path(), &db);
        assert!(!status.downloaded);
        assert!(status.model_id.is_none());
        assert!(status.model_path.is_none());
    }

    #[test]
    fn check_local_model_finds_gguf_file() {
        let tmp = tempfile::tempdir().unwrap();
        let db = crate::db::Database::new(tmp.path().join("test.db").to_str().unwrap()).unwrap();
        db.initialize().unwrap();
        let models = models_dir(tmp.path());
        std::fs::create_dir_all(&models).unwrap();
        std::fs::write(models.join("test-model.gguf"), b"fake model data").unwrap();
        let status = check_local_model(tmp.path(), &db);
        assert!(status.downloaded);
        assert!(status.model_path.is_some());
    }

    #[test]
    fn delete_local_model_removes_file_and_clears_settings() {
        let tmp = tempfile::tempdir().unwrap();
        let db = crate::db::Database::new(tmp.path().join("test.db").to_str().unwrap()).unwrap();
        db.initialize().unwrap();
        let models = models_dir(tmp.path());
        std::fs::create_dir_all(&models).unwrap();
        let model_path = models.join("test-model.gguf");
        std::fs::write(&model_path, b"fake").unwrap();
        db.set_setting("local_model_id", "test-model").unwrap();
        db.set_setting("local_model_path", model_path.to_str().unwrap()).unwrap();
        delete_local_model(tmp.path(), &db).unwrap();
        assert!(!model_path.exists());
    }
}
