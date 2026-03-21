# SLM-Powered Metadata, LiteParse Integration, and Codebase Cleanup

**Date:** 2026-03-21
**Status:** Approved
**Scope:** Replace sidecar with in-process SLM, add LiteParse optional parsing, transparent metadata UI, dead code removal, known issue fixes

---

## 1. Architecture Overview


### Current (broken/burdened)

```
Document → Rust parser → UDT tree → sidecar llama-server (HTTP) → metadata
                                      ↑ downloads EXE at runtime
                                      ↑ manages DLLs, health checks
                                      ↑ broken/unreliable
```

### New

```
Document → [LiteParse OR Rust parser] → raw text
                                          ↓
                                    candle SLM (in-process)
                                          ↓
                            ┌─────────────┼──────────────┐
                            ↓             ↓              ↓
                      Assist parser   Build metadata   Classify headings
                      (heading Y/N?)  (summary,        (level 1/2/3?)
                                      entities,
                                      topics)
                                          ↓
                                    Enriched UDT tree → SQLite
```

### What stays unchanged

- Existing Rust parsers (pdf-extract, pulldown-cmark) — compiled in, zero deps
- Document tree structure (UDT), deterministic fetcher, 9-phase query pipeline
- All 10 cloud LLM providers, frontend Zustand stores, DB schema V3

### What changes

- llama-server sidecar → candle in-process inference (~870 lines replaced with ~200)
- LiteParse added as optional text extractor (runtime-detected Node.js)
- Metadata transparency added to UI
- Dead ReAct scaffolding removed (~1,351 lines)

### Fallback chains

**Text extraction:**
1. LiteParse (if Node.js detected on system) — better layout-aware extraction
2. Existing Rust parsers (always available) — compiled into binary

**Metadata generation:**
1. candle SLM (if model downloaded) — LLM-generated summaries, entities, topics
2. Existing heuristics (always available) — regex entities, TF topics, extractive summary

### Distribution guarantee

Single EXE/MSI installer. No Python, no Node.js, no Java required. Works on any Windows 10/11 machine. LiteParse is an optional enhancement for users who happen to have Node.js installed.

---

## 2. Candle SLM Integration (Replace Sidecar)

### What dies

The entire `start_sidecar()` / `stop_sidecar()` / `chat_inference()` HTTP dance. No more downloading `llama-server.exe`, extracting ZIPs, hunting for DLLs, polling `/health`, managing child processes.

### What replaces it

A Rust module that loads a GGUF model directly into memory using `candle-core` + `candle-transformers`.

```rust
// New: src-tauri/src/llm/slm.rs

pub struct SlmEngine {
    model: QwenForCausalLM,   // candle quantized model
    tokenizer: Tokenizer,     // HF tokenizer
    device: Device,            // CPU (Device::Cpu)
}

impl SlmEngine {
    /// Load GGUF model from disk. Called once after download.
    /// Takes ~2-5 seconds on first load, stays in memory.
    pub fn load(model_path: &Path) -> Result<Self, SlmError> { ... }

    /// Generate text. Blocking — call from spawn_blocking().
    pub fn generate(&self, prompt: &str, max_tokens: u32) -> Result<String, SlmError> { ... }

    /// Unload model, free memory.
    pub fn unload(&mut self) { ... }

    /// Check if a model is loaded.
    pub fn is_loaded(&self) -> bool { ... }
}

// Global singleton, lazy-loaded
static SLM: OnceLock<Mutex<Option<SlmEngine>>> = OnceLock::new();
```

### Lifecycle

- **Download** — user clicks "Download Model" in Settings, GGUF file saved to `<app-data>/models/`. No binary download (no llama-server ZIP).
- **Tokenizer** — download `tokenizer.json` (~2MB) alongside the GGUF. Required by candle.
- **Load** — after download (or on app start if model exists), call `SlmEngine::load()`. Takes ~2-5s for Qwen2.5 0.5B Q4.
- **Inference** — `slm::generate(prompt, max_tokens)` replaces `local::chat_inference()`. Same interface, no HTTP.
- **Unload** — on app exit or when user deletes model.

### Memory impact

Qwen2.5 0.5B Q4 ≈ ~500MB RAM when loaded. Acceptable for a desktop app. Model stays loaded while app is running (no load/unload per call — too slow).

### Cargo.toml additions

```toml
candle-core = { version = "0.8", features = ["default"] }
candle-transformers = "0.8"
tokenizers = "0.20"    # HuggingFace tokenizers (Rust native)
```

### Code changes in local.rs

**Delete:**
- `SidecarState`, `SIDECAR` static, `sidecar_lock()`
- `start_sidecar()`, `stop_sidecar()`, `is_sidecar_running()`
- `chat_inference()` (HTTP POST to /completion)
- `resolve_server_binary()`, `is_server_binary_available()`
- `server_binary_info()`, `download_server_binary()`
- `extract_from_zip()`, `extract_from_tar_gz()`

**Keep:**
- `ModelOption`, `get_model_options()`, `download_model()` (GGUF download part only)
- `check_local_model()`, `LocalModelStatus`, `DownloadProgress`

**Estimated:** ~500 lines deleted, ~370 lines kept, ~200 lines new (SlmEngine in slm.rs).

### Known tradeoffs

- **Build time increases** — candle compiles GGML kernels (~30-60s extra on first build)
- **Inference ~30-50% slower than llama.cpp** — acceptable for short generations (summaries are 1-2 sentences)
- **CPU only initially** — `Device::Cpu`. GPU (CUDA/Metal) possible later but adds build complexity.
- **Tokenizer file needed** — must download `tokenizer.json` alongside the GGUF (~2MB)

---

## 3. LiteParse Optional Integration

### Strategy

LiteParse is an optional enhancement, not a dependency. The app detects whether Node.js + LiteParse are available at runtime and uses them if present. If not, existing Rust parsers handle everything.

### Detection

```rust
// New: src-tauri/src/document/liteparse.rs

pub fn is_liteparse_available() -> bool {
    // 1. Check if npx/node exists on PATH
    // 2. Try: npx @llamaindex/liteparse --version
    // 3. Cache result for session (don't re-check every ingest)
    Command::new("npx")
        .args(["@llamaindex/liteparse", "--version"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn parse_with_liteparse(file_path: &str) -> Result<String, String> {
    let output = Command::new("npx")
        .args(["@llamaindex/liteparse", "parse", file_path, "--format", "json"])
        .output()
        .map_err(|e| format!("LiteParse failed: {}", e))?;

    String::from_utf8(output.stdout)
        .map_err(|e| format!("LiteParse output error: {}", e))
}
```

### Ingest decision tree

```
Document arrives for ingest
  ├─ Is LiteParse available? (cached check)
  │   ├─ YES → Run LiteParse → get layout-aware text + bounding boxes
  │   │         → Feed to Rust tree builder (use spatial data for better heading detection)
  │   │         → If LiteParse fails → fall through to Rust parser
  │   └─ NO  → Use existing Rust parser (pdf-extract / pulldown-cmark)
  │
  ├─ Tree built (either path)
  │
  └─ Is SLM loaded? (candle)
      ├─ YES → SLM enriches metadata (summaries, entities, topics)
      └─ NO  → Heuristic enrichment (existing regex/TF code)
```

### What LiteParse adds when available

- Better text extraction from complex PDFs (spatial grid preserves column layouts, tables)
- OCR for scanned PDFs (Tesseract.js built in) — currently a stub in our app
- Bounding box data that helps heading classification (large text = heading)

### UI indicator

Settings panel shows "LiteParse: Detected" or "LiteParse: Not available" with install instructions. Informational, not blocking.

### Known tradeoffs

- Adds ~1-3s latency per ingest (spawning npx process)
- LiteParse output is flat spatial text, not a tree — still needs Rust code to convert to UDT
- If user has Node.js but outdated version, `npx` might fail silently
- LiteParse's JSON format may change between versions — fragile coupling

---

## 4. SLM-Assisted Parsing and Metadata Generation

### Job 1: Assist the parser (heading classification)

The existing PDF parser uses 6 heuristic strategies to detect headings. The SLM adds a 7th — actual language understanding for ambiguous cases.

```rust
// In parser.rs, after heuristic detection:

fn classify_heading_with_slm(line: &str, context: &str) -> Option<HeadingClassification> {
    // Only called for AMBIGUOUS lines where heuristics disagree or score low
    let prompt = format!(
        "Given this line from a PDF document:\n\
         Line: \"{}\"\n\
         Surrounding text: \"{}\"\n\
         Is this a heading/title or body text? If heading, what level \
         (1=chapter, 2=section, 3=subsection)?\n\
         Reply ONLY: heading:<level> OR body",
        line, context
    );

    let result = slm::generate(&prompt, 10)?; // max 10 tokens
    parse_heading_response(&result)
}
```

**When it fires:** NOT on every line. Only when heuristic confidence is low (e.g., a short line that could be heading OR short paragraph). ~5-10 SLM calls per document, not hundreds.

### Job 2: Generate node metadata

Replaces the broken `llm_summary()` path in metadata.rs. Three separate prompts per top-level node:

```rust
fn slm_enrich_node(content: &str) -> NodeMetadata {
    // 1. Summary (1-2 sentences)
    let summary = slm::generate(
        &format!("Summarize in 1-2 sentences:\n\n{}", truncate(content, 2000)),
        80
    );

    // 2. Entities (names, dates, amounts, orgs)
    let entities = slm::generate(
        &format!(
            "Extract key entities (names, dates, amounts, organizations) \
             as a comma-separated list:\n\n{}",
            truncate(content, 2000)
        ),
        100
    );

    // 3. Topics (3-5 keywords)
    let topics = slm::generate(
        &format!("List 3-5 topic keywords for this text, comma-separated:\n\n{}", truncate(content, 2000)),
        40
    );

    NodeMetadata { summary, entities: parse_csv(entities), topics: parse_csv(topics) }
}
```

### Hybrid entity extraction

SLM entity extraction via prompt is less precise than regex for structured patterns ($, %, dates). Keep heuristic extraction too, merge results:
- Regex finds exact `$1.2M`, `15%`, `Q3 2025`
- SLM finds `ACME Corporation`, `John Smith` that regex misses
- Union of both = best coverage

### Fallback behavior

If SLM is not loaded or any call fails, falls through to existing heuristic functions (`extractive_summary()`, `extract_entities()`, `extract_topics()`). Same non-fatal pattern as current pipeline.

### Performance budget

A document with 15 top-level nodes ≈ ~50 SLM calls total (15×3 metadata + ~5 heading assists). At ~0.5-1s per call on CPU with Qwen 0.5B Q4, that's **25-50 seconds per document ingest**. Runs in background with progress events to UI.

---

## 5. Metadata Transparency UI

### TreeView node metadata panel

When clicking a node in the TreeView, a metadata card appears:

```
┌─────────────────────────────────────────┐
│ § Financial Results for Q3 2025         │
│                                         │
│ Summary: ACME Corporation reported      │
│ $1.2M revenue in Q3 2025, a 15%        │
│ increase YoY.                           │
│                                         │
│ Entities: ACME Corporation, $1.2M,      │
│ Q3 2025, 15%                            │
│                                         │
│ Topics: revenue, finance, quarterly,    │
│ growth                                  │
│                                         │
│ Source: [SLM] / [Heuristic]   [Re-run]  │
└─────────────────────────────────────────┘
```

### Key UI elements

1. **Source badge** — shows whether metadata came from SLM or heuristic fallback. No ambiguity.

2. **Re-run button** — re-generates metadata for this single node. Immediate feedback.

3. **Ingest progress overlay** — during document ingest with SLM enabled, DocsPanel shows progress bar: "Enriching metadata... 7/15 nodes" with per-node status.

4. **Enrichment status per document** — in document list, badge shows:
   - "SLM enriched" (green) — all nodes processed by SLM
   - "Heuristic" (gray) — no SLM available at ingest time
   - "Partial" (yellow) — some nodes enriched, some fell back
   - "Re-enrich" button for heuristic/partial docs when SLM becomes available

5. **Settings → Local Model section updated:**
   - Model status: "Qwen2.5 0.5B Q4 — Loaded (491 MB)"
   - Download/Delete buttons (existing)
   - "LiteParse: Detected / Not available"
   - "Enrichment mode: SLM / Heuristic / Auto"

---

## 6. Dead Code Cleanup

### Category 1: ReAct scaffolding (DELETE entirely)

| File | Lines | Contents | Action |
|------|-------|----------|--------|
| `agent/runtime.rs` | ~502 | `AgentRuntime`, `build_system_prompt()`, `ExplorationStep` | Delete file |
| `agent/context.rs` | ~100 | `ExplorationContext` | Delete file |
| `agent/tools.rs` | ~749 | 9 tool definitions, `get_provider_tools()`, `execute_tool()` | Delete file |

**Total: ~1,351 lines removed.** Update `agent/mod.rs` to remove module declarations.

### Category 2: Sidecar code (REPLACE in local.rs)

| Code | Action |
|------|--------|
| `SidecarState`, `SIDECAR` static, `sidecar_lock()` | Delete — replaced by `SlmEngine` |
| `start_sidecar()`, `stop_sidecar()`, `is_sidecar_running()` | Delete — replaced by `SlmEngine::load()`/`is_loaded()` |
| `chat_inference()` (HTTP POST to /completion) | Delete — replaced by `SlmEngine::generate()` |
| `resolve_server_binary()`, `is_server_binary_available()` | Delete — no binary to find |
| `server_binary_info()`, `download_server_binary()` | Delete — no binary download |
| `extract_from_zip()`, `extract_from_tar_gz()` | Delete — no archive extraction |
| `ModelOption`, `get_model_options()`, `download_model()` (GGUF) | **KEEP** |
| `check_local_model()`, `LocalModelStatus`, `DownloadProgress` | **KEEP** |

### Category 3: Stale references (CLEAN)

- Remove `#[allow(dead_code)]` annotations protecting deleted files
- Update `commands.rs` — remove IPC commands referencing sidecar
- Update `lib/tauri.ts` — remove corresponding frontend wrappers
- Update `localModel.ts` store — simplify (no more "binary_ready")
- Update CLAUDE.md — remove dead code section, update local model section

---

## 7. Known Issue Fixes

### Fix 1: API key encryption

**Current:** Column named `api_key_encrypted` stores plaintext. `keyring` crate in Cargo.toml but not wired.

**Fix:** Wire the `keyring` crate. On save, store to OS keychain (Windows Credential Manager). DB column stores a reference key, not the actual secret. Fallback: if keyring fails (e.g., headless/CI), warn user and store as-is with visible "Unencrypted" badge in Settings.

### Fix 2: PDF image extraction

**Current:** `image.rs` returns `Vec::new()`. Images silently lost.

**Fix:** Use `lopdf` (already in Cargo.toml) to extract embedded image streams from PDF objects. Create `ImageNode` entries in the tree with image dimensions and byte hash. Not sending images to SLM (Qwen 0.5B has no vision) — but nodes exist in tree so deterministic fetcher can reference them ("Figure 3 appears in Section 2.1").

**Honest limitation:** Extracted images won't be analyzed for content. They'll be placeholder nodes with position info. Full vision analysis requires multimodal cloud provider — out of scope for local-only.

### Fix 3: Cancel flags between preprocessing steps

**Current:** `AtomicBool` checked before LLM call only. Can't cancel mid-enrichment.

**Fix:** Check cancel flag between each SLM call in metadata enrichment loop and between each preprocessing step in pipeline. Add cancel check between Phase 3a/3b/3c (rewrite/HyDE/stepback).

### Fix 4: Metadata not verifiable

**Fixed by Section 5** — metadata transparency UI with source badges, re-run buttons, and enrichment status per document.

### Fix 5: Stale docs and CLAUDE.md

Update CLAUDE.md to reflect:
- New candle-based SLM architecture (remove sidecar references)
- Remove "Dead Code" section (it's deleted)
- Update "Known Stubs" (image extraction partial, not stub)
- Add LiteParse optional integration
- Update pipeline description with SLM-assisted parsing
- Update file structure

---

## 8. Implementation Order

| Phase | What | Risk | Dependencies | Parallel? |
|-------|------|------|--------------|-----------|
| **P1** | Delete dead code (runtime.rs, context.rs, tools.rs) | Low | None | — |
| **P2** | Add candle to Cargo.toml, build `SlmEngine` in `slm.rs` | Medium | None | — |
| **P3** | Replace sidecar code in local.rs with SlmEngine | Medium | P2 | — |
| **P4** | Wire SLM into metadata.rs (replace `chat_inference` calls) | Low | P3 | — |
| **P5** | Add SLM-assisted heading classification in parser.rs | Low | P3 | — |
| **P6** | Add LiteParse optional detection + integration | Low | None | Yes (parallel with P2-P5) |
| **P7** | Metadata transparency UI (TreeView cards, badges, re-run) | Low | P4 | — |
| **P8** | Fix API key encryption (wire keyring crate) | Medium | None | Yes (parallel) |
| **P9** | Fix PDF image extraction (lopdf streams) | Medium | None | Yes (parallel) |
| **P10** | Fix cancel flags (add checks between steps) | Low | None | Yes (parallel) |
| **P11** | Update CLAUDE.md and docs | Low | All above | — |

**P6, P8, P9, P10 can run in parallel with the main P1→P5 chain.**

---

## 9. Inventory: What Is Currently Broken, Incomplete, or Stubbed

### Broken (does not function)

| Item | Location | Problem |
|------|----------|---------|
| SLM metadata generation | `metadata.rs:40-42` → `local.rs` | Sidecar architecture unreliable; `chat_inference()` fails if sidecar not running; no transparency |
| Local model inference | `local.rs` sidecar | Downloads work, but inference via HTTP to llama-server is fragile (DLL issues, startup timeouts, platform differences) |

### Stubbed (returns empty/placeholder)

| Item | Location | Problem |
|------|----------|---------|
| PDF image extraction | `document/image.rs` | Returns `Vec::new()`. Images in documents silently lost. |
| Ollama inference | `llm/local.rs` via Ollama provider | Download + binary work. Actual model inference fails. |

### Incomplete (partially working)

| Item | Location | Problem |
|------|----------|---------|
| API key encryption | `db/schema.rs` column `api_key_encrypted` | Column exists, `keyring` crate in Cargo.toml, but encryption not wired. Keys stored plaintext. |
| Cancel flags | `chat_handler.rs` | `AtomicBool` checked before LLM call only, not between preprocessing steps or SLM enrichment calls. |
| Node metadata quality | `metadata.rs` | Heuristic-only: extractive summary (first 2 sentences), regex entities, TF topics. Functional but low quality. |
| PDF heading detection | `parser.rs` | 6 heuristic strategies work but produce false positives on short text, ambiguous lines. |
| Cross-doc relations | `metadata.rs` | Works but quality limited by heuristic entity/topic extraction feeding it. |

### Dead code (confirmed unused, safe to delete)

| Item | Location | Lines |
|------|----------|-------|
| ReAct agent runtime | `agent/runtime.rs` | ~502 |
| Exploration context | `agent/context.rs` | ~100 |
| Tool definitions | `agent/tools.rs` | ~749 |
| Sidecar management | `local.rs` (sidecar portions) | ~500 |
