# llama-server sidecar binaries

Place pre-compiled `llama-server` binaries here for each target platform.
Tauri bundles them into the installer via `externalBin` in `tauri.conf.json`.

## Naming convention (Tauri requires the target triple suffix)

| Platform       | Filename                                          |
|----------------|---------------------------------------------------|
| Windows x86_64 | `llama-server-x86_64-pc-windows-msvc.exe`         |
| macOS arm64    | `llama-server-aarch64-apple-darwin`               |
| macOS x86_64   | `llama-server-x86_64-apple-darwin`                |
| Linux x86_64   | `llama-server-x86_64-unknown-linux-gnu`           |

## Where to get the binaries

Download pre-built releases from the official llama.cpp GitHub releases page:
  https://github.com/ggml-org/llama.cpp/releases

Pick the latest release, download the platform ZIP (e.g. `llama-b...-bin-win-avx2-x64.zip`),
extract `llama-server.exe`, and rename it with the correct suffix above.

## Build-time note

These binaries are NOT needed at `cargo build` time — only when running the
packaged app. Developers can run `cargo check` and `cargo test` without them.
The sidecar is optional: if absent, enrichment falls back to heuristic extraction.
