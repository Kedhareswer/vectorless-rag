# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## TGG — Tree-Grounded Generation

A desktop app (Tauri v2 + React) for document Q&A that replaces vector/embedding-based RAG with **deterministic document tree exploration + LLM-powered query enrichment**. Documents are parsed into structured trees; an enrichment pipeline improves retrieval quality, the deterministic fetcher reads the right content, then a single streaming LLM call generates the grounded answer.

**TGG = Tree-Grounded Generation** — answers are grounded in the actual tree structure of documents, not cosine similarity over embeddings.

## Commands

```bash
# Development
npm run tauri dev          # Start dev build (Vite + Rust hot-reload)
npm run dev                # Frontend only (http://localhost:1420)

# Production
npm run tauri build        # Release bundle → src-tauri/target/release/bundle/

# Type checking
npx tsc --noEmit           # TypeScript check (run after every .ts/.tsx change)

# Rust
cargo check                # Fast compile check (run after every .rs change)
cargo test --lib db::      # DB layer tests
cargo test --lib document::parser   # PDF/MD parser tests
cargo test --lib agent::query       # Query preprocessing tests

# Frontend tests (Vitest + Testing Library)
npm test                   # Run all frontend tests once
npm run test:watch         # Watch mode
```

> Tests in `commands.rs` / Tauri-dependent code cannot run via `cargo test` — expected, pre-existing limitation.

## Pipeline (exact current implementation)

Every user query goes through 9 phases in `agent/chat_handler.rs`:

```
Phase 1  Fast heuristic preprocess (sync, ~0ms)
         classify intent → extract search terms

Phase 2  Cancel check

Phase 3  Local-SLM query enrichment (3 sequential calls via candle, in-process)
         3a. Query Rewrite  — search-optimized reformulation
         3b. HyDE           — hypothetical answer passage, extract terms
         3c. StepBack       — broader question, extract terms
         All new terms merged into ProcessedQuery.search_terms

         Cancel check

Phase 4  Deterministic content fetch (sync, no LLM)
         code decides what to read based on intent + enriched terms

         Cancel check

Phase 5  Cross-doc relation discovery (if >1 doc)
         compare entity/topic metadata across trees → persist to DB

Phase 6  Load conversation history (token-windowed, 8000 token budget)

         Cancel check

Phase 7  ONE streaming LLM call (no tools)
         system prompt = doc content + discovered relations
         streamed tokens forwarded to frontend

Phase 8  Save full trace (preprocessing + fetch + LLM steps)
```

**Critical pipeline rules:**
- LLM NEVER gets tools. `provider.chat_stream(messages, None, token_tx)` — tools always `None`.
- Enrichment runs on the local SLM (free, offline) and only when a model is loaded; the final answer uses the selected provider/model — which may itself be the local SLM (`local` provider, see below).
- With the `local` provider, content is cut to 12,000 chars (`FetchedContent::limit_to`) and history to 2,000 tokens, and cost is 0.
- Each SLM generation is capped at 12s (partial text kept) and stops when the user cancels.
- Preprocessing errors are non-fatal — step marked "Skipped", pipeline continues.
- Content budget: **40,000 chars** per query (`CONTENT_BUDGET` in `deterministic.rs`).
- Total UI steps: 3 preprocessing + N fetch steps + 1 `llm_call`.

## Architecture

```
React Frontend ←→ Tauri IPC (invoke) ←→ Rust Backend
                                          ├── Document Engine (parsers, tree builder, cache, metadata, liteparse)
                                          ├── Query Pipeline (query.rs, deterministic.rs, chat_handler.rs)
                                          ├── SLM Engine (candle GGUF inference, in-process, CPU-only)
                                          ├── LLM Provider Layer (10 providers + retry wrapper)
                                          └── SQLite DB (trees, traces, conversation_documents)
```

## Tech Stack
- **Desktop shell**: Tauri v2 (Rust backend + WebView frontend)
- **Frontend**: React 19 + TypeScript 7 + Vite 8 (Vitest 5, Node 24 LTS in CI; Node ≥22.22.2 locally)
- **Backend**: Rust, edition 2024 (Tokio async runtime)
- **Database**: SQLite via `rusqlite` (document trees, traces, settings) — schema at V3
- **Styling**: CSS custom properties + CSS modules, Inter font, JetBrains Mono for code
- **State management**: Zustand stores (chat, documents, settings, theme, localModel)

## LLM Providers (11 total, all real)
- **Groq** — Llama, Mixtral, Gemma (fast inference)
- **Google AI Studio** — Gemini 2.5 Pro/Flash (vision capable)
- **OpenRouter** — Claude, GPT, Llama, Mistral, DeepSeek, etc.
- **AgentRouter** — Smart LLM routing (GPT-5, DeepSeek, GLM), OpenAI-compatible
- **Anthropic** — Claude 3.5/3.7/4 Sonnet, Opus, Haiku (direct API, real SSE streaming)
- **OpenAI** — GPT-4o, GPT-4.1, o1, o3 mini
- **DeepSeek** — DeepSeek Chat, DeepSeek Reasoner
- **xAI / Grok** — Grok 3, Grok 3 Mini
- **Qwen** — Qwen Max, Qwen Plus, Qwen Turbo
- **Ollama** — Any model served by a local Ollama; NDJSON token streaming, 16k context requested
- **LM Studio** — any model loaded in a local LM Studio server (`http://localhost:1234/v1`), OpenAI-compatible, no API key (`OpenAICompatProvider::new_local`)
- **Already-installed models** — `llm/local_servers.rs` probes Ollama (:11434) and LM Studio (:1234) in parallel (1.5 s timeout, no proxy) via `detect_local_models`; the frontend (`stores/localServers.ts`) offers them in the setup checklist, the composer model picker ("Installed on this computer") and Settings. `selectServerModel` in `stores/settings.ts` creates/reuses a keyless provider and activates it
- **Local (offline)** — `llm/local_provider.rs`: answers with the in-process SLM (default LFM2.5-1.2B-Instruct), streamed, no API key, not wrapped in RetryProvider
- All wrapped with `RetryProvider` for automatic retry on transient errors.
- Unified `LLMProvider` trait: `chat()` + `chat_stream()` + `capabilities()`

## Core Concepts

### Universal Document Tree (UDT)
Every document is parsed into a uniform tree:
```rust
Node { id, node_type, content, metadata, children, relations }
```
- Type-specific parsers in `document/parser.rs` output into this same schema
- **PDF parser**: native LiteParse crate (PDFium; layout blocks → Section/Paragraph/ListItem/Table/CodeBlock with page numbers) → fallback to pdf-extract when PDFium is missing or a page has no text (no OCR compiled in). The pdf-extract fallback uses 7 heading strategies: `SECTION_KEYWORDS` (~40 common headings) + all-caps + numbered + title-case + two-pass line splitting + SLM classification for ambiguous lines
- **Image extraction**: lopdf extracts embedded JPEG/PNG/raw images; positioned as ImageNode in tree (no visual analysis)
- Metadata fields: `summary`, `entities`, `topics`, `page_number`, `word_count`

### Document Scope: Per-Chat, Not Global
Documents belong to individual conversations via a many-to-many join table:
```sql
conversation_documents (conv_id TEXT, doc_id TEXT, added_at TEXT)
```
- Same document can be in multiple chats (no duplication of file/tree data)
- IPC commands: `add_doc_to_conversation`, `remove_doc_from_conversation`, `get_conversation_doc_ids`
- Frontend: `conversationDocIds` in chat store — never use global document selection
- DocsPanel (slide-over) shows docs for the active conversation only

### Query Enrichment (candle SLM, in-process)
- **Query Rewrite** — SLM reformulates for better search coverage
- **HyDE** — SLM writes a hypothetical answer passage; terms extracted match document vocabulary
- **StepBack** — SLM generates broader question; retrieves background context
- All three in `agent/query.rs`: `rewrite_query()`, `generate_hyde()`, `stepback_query()`
- Uses `llm/slm.rs` (candle GGUF engine) — no external process, no network
- Results enrich `ProcessedQuery.search_terms` before deterministic fetch

### Deterministic Content Fetcher (`agent/deterministic.rs`)
Intent → fetch strategy:
- **Summarize**: expand top-level sections (uses metadata summaries if available)
- **Factual/Specific**: search enriched terms → expand matching nodes (max 12 per term)
- **Entity**: same as factual
- **Comparison**: search + expand; enriched terms find cross-doc matches
- **ListExtract**: find Table/ListItem nodes → expand parent sections

### Cross-Document Relation Discovery
- Runs during Phase 5 when `trees.len() > 1`
- `document/metadata.rs`: `discover_cross_doc_relations()` compares entity/topic metadata
- Relation types: `shared_entity`, `topic_overlap` (≥2 shared topics)
- Persisted to `cross_doc_relations` table; loaded into system prompt
- Frontend re-fetches via `relationsVersion` counter (incremented in `setIsExploring(false)`)

### Tracing
- `TraceRecord`: tokens, cost (from `pricing.json` per-model rates), latency
- `StepRecord`: one per pipeline step
- Cost via `estimate_cost(model_id, input_tokens, output_tokens)` in `chat_handler.rs`

### Dictation (offline Whisper)
- Mic button (`chat/DictationButton.tsx`) records via `lib/audio.ts` (Web Audio → 16 kHz mono f32), store `stores/dictation.ts`
- `transcribe_audio` → `llm/dictation.rs`: candle Whisper, greedy English decode, 30 s windows, non-speech windows dropped, 5 min cap
- Models download on first use to `<app-data>/models/dictation/<id>/`: Whisper tiny.en Q8 (~42 MB, default) or base.en (~290 MB)
- Not part of the query pipeline: it only fills the chat input

## Known Limitations
- **Local model**: ONE download does all local work (enrichment, metadata, and answers via the `local` provider): Liquid LFM2.5-1.2B-Instruct QAD Q4_0 (~700 MB; Liquid's recommended successor to the deprecated LFM2-1.2B-RAG). LFM2.5-230M (~150 MB) is offered for low-memory PCs — enrichment only, too small for reliable answers. Local answers are slower (CPU) and simpler than large cloud models.
- **PDF image analysis**: Images are extracted and positioned in the tree (lopdf), but their visual content is not analyzed (would require a multimodal model).
- **LiteParse / PDFium**: PDFium is a shared library downloaded by `liteparse-pdfium-sys` at build time; `build.rs` copies it to `src-tauri/resources/` (gitignored) and Tauri bundles it. Scanned PDFs need OCR, which is not compiled in (tesseract / oar-ocr features are heavy).

- **Dictation platforms**: release builds target Windows (WebView2 asks for mic permission). macOS would need `NSMicrophoneUsageDescription`; Linux WebKitGTK may not expose `getUserMedia`.

## UI Design

### Layout: TopBar + slide-over panels
```
┌──────────────────────────────────────────────────────────────┐
│ [Chat title ▾] [✎ new]          [Documents n] [Trace] | [⚙]  │
├──────────────────────────────────────────────────────────────┤
│   Empty chat: setup checklist → starter questions            │
│   Question → ProcessSummary (steps, folds when done) → answer│
│   ┌ composer ───────────────────────────────────────────┐    │
│   │ [doc chip] [+ Add]                 [model picker ▾] │    │
│   │ Ask about your documents…                [mic] [➤]  │    │
│   └─────────────────────────────────────────────────────┘    │
│                          SlidePanel (Docs / Trace) from right │
└──────────────────────────────────────────────────────────────┘
```
- Chat title is the ConversationSwitcher trigger (search, ↑/↓/Enter, rename, delete with confirm)
- Launch resumes the last chat (`resumeLastConversation`); `startNewChat` reuses an empty chat; untitled chats are named after the first question
- Add documents ONLY via `hooks/useAddDocument` (ingest → attach, creating a chat if none is open)
- Composer ModelPicker switches the active provider (`activateProvider` persists both rows)
- SettingsModal: two-pane, sections from `SETTINGS_SECTIONS` (Providers, Local AI, Appearance, Data); model is free text with suggestions
- Answers have a Copy action (`MessageActions`)

### Pipeline step display
Step labels/icons live in ONE registry: `src/lib/pipelineSteps.ts` (`PIPELINE_STEPS`, `getStepMeta`). Used by ThinkingBlock (chat) and TraceView. A new backend step = one entry there.

| tool key | label | icon |
|---|---|---|
| `query_rewrite` | Rewrote query | PenLine |
| `hyde` | Hypothetical answer (HyDE) | Sparkles |
| `stepback` | Broadened question (StepBack) | ArrowUpRight |
| `tree_overview` | Read structure | FileText |
| `search` | Searched content | Search |
| `expand` | Read section | BookOpen |
| `scan_lists` | Scanned lists & tables | List |
| `llm_call` | Generated answer | Cpu |

### Keyboard shortcuts (`src/lib/shortcuts.ts` → `useShortcuts`)
`mod` = ⌘ on macOS, Ctrl elsewhere: mod+N new chat · mod+K switch chat · mod+D documents · mod+J trace · mod+, settings · `/` focus input. Tooltips get hints from the same registry (`withShortcut`).

### Color Tokens
```
Light:
  --bg-primary: #F4F3EE      --text-primary: #1C1917
  --bg-secondary: #FFFFFF     --text-secondary: #78716C
  --bg-sidebar: #EDEAE3      --accent: #DE7356
  --border: #E7E5E0          --accent-deep: #C15F3C

Dark:
  --bg-primary: #1C1917      --text-primary: #F4F3EE
  --bg-secondary: #282420    --text-secondary: #A8A29E
  --bg-sidebar: #231F1B      --accent: #DE7356
  --border: #3D3730          --accent-deep: #E8845E
```

## File Structure
```
src-tauri/src/
  main.rs, lib.rs
  pricing.json              — per-model token rates for cost estimation
  util.rs                   — shared utilities (safe_truncate, etc.)
  validation.rs             — path validation, input sanitization
  commands.rs               — Tauri IPC handlers (thin delegation only)

  document/
    parser.rs               — PDF (LiteParse + pdf-extract), markdown, plaintext, code parsers
    tree.rs                 — DocumentTree, TreeNode, NodeType
    image.rs                — PDF/DOCX image extraction (lopdf + zip)
    cache.rs                — LRU tree cache
    metadata.rs             — SLM + heuristic entity/topic/summary extraction, cross-doc relations
    liteparse.rs            — native LiteParse (PDFium) parsing → document tree

  agent/
    query.rs                — heuristic preprocess + rewrite_query/generate_hyde/stepback_query
    deterministic.rs        — fetch_content() per-intent strategies, FetchedContent
    chat_handler.rs         — run_agent_chat() — THE ONLY ACTIVE PIPELINE FILE
    events.rs               — ChatEvent enum

  llm/
    provider.rs             — LLMProvider trait, Message, LLMResponse, ProviderConfig
    retry.rs                — RetryProvider (exponential backoff)
    slm.rs                  — candle in-process GGUF inference (lfm2/qwen2/llama arch; weights loaded once; streaming)
    local_provider.rs       — `local` answer provider over slm.rs (offline answers)
    local_servers.rs        — detects models already installed in Ollama / LM Studio
    local.rs                — model download/management, delegates inference to slm.rs
    dictation.rs            — offline Whisper speech-to-text (model catalog, download, transcribe)
    anthropic.rs            — Anthropic (real SSE streaming)
    openai_compat.rs        — OpenAI, DeepSeek, xAI, Qwen, AgentRouter
    groq.rs, google.rs, openrouter.rs

  db/
    schema.rs               — SQLite schema V3, migrations, all CRUD queries
    traces.rs               — trace-specific queries

src/
  stores/
    chat.ts                 — conversations, messages, explorationSteps, conversationDocIds, relationsVersion
    documents.ts            — document library (global), activeDocumentId
    settings.ts             — provider configs, active provider
    theme.ts                — light/dark theme management
    localModel.ts           — local model download state
    localServers.ts         — detected Ollama / LM Studio servers and their models
    dictation.ts            — dictation model status, recording/transcribing phase
  lib/tauri.ts              — typed invoke wrappers for all Tauri commands
  lib/audio.ts              — mic recording + downsampling to 16 kHz
  lib/pipelineSteps.ts      — step label/icon registry
  lib/shortcuts.ts          — keyboard shortcut registry
  lib/format.ts             — latency/cost/count formatting
  lib/providers.ts          — provider display names (e.g. "Local (offline)")
  hooks/useShortcuts.ts, hooks/useAddDocument.ts
  components/
    common/TopBar.tsx         — top navigation bar with ConversationSwitcher + icon buttons
    common/ConversationSwitcher.tsx — conversation dropdown (replaces sidebar chats tab)
    common/SlidePanel.tsx     — reusable slide-over panel wrapper
    common/IconButton.tsx     — shared icon button component
    common/ModelDownloadDialog.tsx
    chat/ChatPanel.tsx        — message input, handleSend, event handler, ThinkingBlock list
    chat/ProcessSummary.tsx   — collapsible group of pipeline steps under a question
    chat/ThinkingBlock.tsx    — renders one pipeline step (labels from lib/pipelineSteps.ts)
    chat/ChatEmptyState.tsx   — setup checklist / starter questions
    chat/ModelPicker.tsx      — composer provider switcher
    chat/MessageActions.tsx   — copy (and future) answer actions
    chat/DictationButton.tsx  — mic button + first-use model download prompt
    preview/DocsPanel.tsx     — per-conversation document list (slide-over)
    preview/TracePanel.tsx    — trace/cost panel (slide-over)
    preview/TreeView.tsx, CanvasView.tsx, TraceView.tsx, RelationsView.tsx
    settings/SettingsModal.tsx — tabbed settings (Providers, Local AI, Appearance, Data)
    settings/LocalAISettings.tsx — enrichment + dictation model management
```

## DB Schema (V3 — current)
```sql
documents (id, name, doc_type, tree_json, created_at)
conversations (id, title, doc_id, created_at)
conversation_documents (conv_id, doc_id, added_at)   -- V3: per-chat doc scoping
messages (id, conv_id, role, content, created_at)
traces (id, conv_id, provider_name, total_tokens, total_cost, total_latency_ms,
        steps_count, created_at, input_tokens, output_tokens)
steps (id, msg_id, tool_name, input_json, output_json, tokens_used, latency_ms)
cross_doc_relations (id, source_doc_id, source_node_id, target_doc_id, target_node_id,
                     relation_type, confidence, description, created_at)
settings (key, value)
providers (id, name, api_key_encrypted, base_url, model, is_active)
```

## Code Conventions
- Rust: snake_case, modules in separate files, `thiserror` for errors, `serde` for serialization
- TypeScript: camelCase for variables/functions, PascalCase for components/types
- CSS: BEM-like with CSS modules, design tokens via custom properties
- All Tauri commands return `Result<T, String>` and are async
- Frontend state in Zustand stores, one store per domain
- No `#[allow(dead_code)]` on active code — only on scaffolding in runtime.rs/tools.rs/context.rs
- Git: [Conventional Commits](https://www.conventionalcommits.org/) — `feat:`, `fix:`, `docs:`, `refactor:`, `chore:`
- All Tauri IPC calls go through wrapper functions in `src/lib/tauri.ts` — never call `invoke()` directly from components

## Development Rules

### The Pipeline Is Sacred
- NEVER send tools to the LLM. The `tools` parameter is always `None`.
- NEVER add a ReAct-style loop. The pipeline is linear: enrich → fetch → one LLM call.
- NEVER add fake/simulated/placeholder responses anywhere in the pipeline.
- NEVER skip the enrichment steps unless there is an explicit user setting to disable them.
- Preprocessing errors must be caught per-step and allow the pipeline to continue.

### Documents Are Per-Chat
- NEVER treat documents as global state across conversations.
- ALWAYS use `conversationDocIds` from chat store, not a global document selection.
- Adding a doc = ingest to library + `addDocToConversation(convId, docId)`.
- Removing a doc from chat = `removeDocFromConversation(convId, docId)`, NOT delete from library.

### Code Placement Rules
- New Rust utilities → `util.rs`
- New Tauri commands → `commands.rs` (thin) + domain module (logic); no business logic in `commands.rs`
- New LLM providers → one file in `llm/`, implement `LLMProvider` trait, add to factory in `chat_handler.rs`, add pricing entry
- Frontend IPC wrappers → `lib/tauri.ts` — one per Tauri command

### Compilation / Test Gates
- `cargo check` must pass after every Rust change before moving on
- `npx tsc --noEmit` must pass after every TypeScript change

### Adding New Pipeline Steps
1. Add async function in `agent/query.rs` following `rewrite_query()` pattern (returns `EnrichmentResult`)
2. Export in `agent/mod.rs`
3. Add step block in `run_agent_chat()` with: StepStart → LLM call → StepComplete → `trace_steps.push`
4. Add the tool key → label/icon entry in `src/lib/pipelineSteps.ts`
5. Increment `step_counter` before each StepStart

### Security
- File paths validated before parsing — reject traversal patterns (`validation.rs`)
- LLM response content treated as untrusted text (no eval, no raw HTML injection)
- API keys stored in OS keychain (Windows Credential Manager / macOS Keychain / Linux Secret Service) via `keyring` crate. DB stores `__keychain__` placeholder, not the actual secret.

## Roadmap Status

| Milestone | Name | Status |
|-----------|------|--------|
| M1 | Solid Core | complete |
| M2 | Reliable Engine | complete |
| M3 | Quality Content | complete |
| M4 | Smooth Operation | complete |
| M5 | Smart Pipeline | **complete** — core pipeline fully working; local model inference is optional enhancement |

Full roadmap: `.planning/roadmap/`

## CI

GitHub Actions (`.github/workflows/ci.yml`) — 3 jobs:
- **Rust**: `cargo clippy --all-targets -- -D warnings` + `cargo test --lib`
- **Frontend** (Node 24): `tsc --noEmit` + `vitest run` + `vite build` + `npm audit --omit=dev --audit-level=high`
- **Release** (push to main only, the only job with write permission): Windows MSI/EXE via git tags (auto-versioning)
