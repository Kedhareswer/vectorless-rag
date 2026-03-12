# TGG — Tree-Grounded Generation

## Project Overview
A desktop application (Tauri v2 + React) that replaces traditional vector/embedding-based RAG with **deterministic document tree exploration + LLM-powered query enrichment**. Documents are parsed into structured trees; an enrichment pipeline improves retrieval quality, the deterministic fetcher reads the right content, then a single streaming LLM call generates the grounded answer.

**TGG = Tree-Grounded Generation** — answers are grounded in the actual tree structure of documents, not cosine similarity over embeddings.

## Pipeline (the exact current implementation)

Every user query goes through 9 phases in `agent/chat_handler.rs`:

```
Phase 1  Fast heuristic preprocess (sync, ~0ms)
         classify intent → extract search terms

Phase 2  Cancel check

Phase 3  LLM-powered query enrichment (3 sequential LLM calls)
         3a. Query Rewrite  — "find best match key terms" search-optimized form
         3b. HyDE           — generate hypothetical answer passage, extract its terms
         3c. StepBack       — generate broader question, extract its terms
         All new terms merged into ProcessedQuery.search_terms

Phase 4  Deterministic content fetch (sync, no LLM)
         code decides what to read based on intent + enriched terms

Phase 5  Cross-doc relation discovery (if >1 doc)
         compare entity/topic metadata across trees → persist to DB

Phase 6  Load conversation history (token-windowed, 8000 token budget)

Phase 7  Cancel check

Phase 8  ONE streaming LLM call (no tools)
         system prompt = doc content + discovered relations
         streamed tokens forwarded to frontend

Phase 9  Save full trace (preprocessing + fetch + LLM steps)
```

**Critical rules about this pipeline:**
- LLM NEVER gets tools. `provider.chat_stream(messages, None, token_tx)` — tools are always `None`.
- The 3 preprocessing calls use `provider.chat()` (non-streaming), quick 30-80 token responses.
- All 4 LLM calls use the same provider/model — preprocessing is not a separate "cheaper" model.
- Preprocessing errors are non-fatal — if one call fails, the step is marked "Skipped" and the pipeline continues with whatever terms were extracted.
- Total step count in UI: 3 preprocessing + N fetch steps + 1 llm_call.

## Tech Stack
- **Desktop shell**: Tauri v2 (Rust backend + WebView frontend)
- **Frontend**: React 19 + TypeScript + Vite
- **Backend**: Rust (Tokio async runtime)
- **Database**: SQLite via `rusqlite` (document trees, traces, evals, settings) — schema at V3
- **Styling**: CSS custom properties + CSS modules, Inter font, JetBrains Mono for code
- **State management**: Zustand stores (chat, documents, settings)

## Architecture
```
React Frontend ←→ Tauri IPC (invoke) ←→ Rust Backend
                                          ├── Document Engine (parsers, tree builder, cache, metadata)
                                          ├── Query Pipeline (query.rs, deterministic.rs, chat_handler.rs)
                                          ├── LLM Provider Layer (10 providers + retry wrapper)
                                          └── SQLite DB (trees, traces, conversation_documents)
```

## LLM Providers (10 total, all real)
- **Groq** — Llama, Mixtral, Gemma (fast inference)
- **Google AI Studio** — Gemini 2.5 Pro/Flash (vision capable)
- **OpenRouter** — Claude, GPT, Llama, Mistral, DeepSeek, etc.
- **AgentRouter** — Smart LLM routing (GPT-5, DeepSeek, GLM), OpenAI-compatible
- **Anthropic** — Claude 3.5/3.7/4 Sonnet, Opus, Haiku (direct API, real SSE streaming)
- **OpenAI** — GPT-4o, GPT-4.1, o1, o3 mini
- **DeepSeek** — DeepSeek Chat, DeepSeek Reasoner
- **xAI / Grok** — Grok 3, Grok 3 Mini
- **Qwen** — Qwen Max, Qwen Plus, Qwen Turbo
- **Ollama** — Local models (Llama, Mistral, LLaVA for vision); **download works, inference is a stub**
- All wrapped with `RetryProvider` for automatic retry on transient errors.
- Unified `LLMProvider` trait: `chat()` + `chat_stream()` + `capabilities()`

## Core Concepts

### Universal Document Tree (UDT)
Every document (PDF, Word, Markdown, code, images) is parsed into a uniform tree:
```rust
Node { id, node_type, content, metadata, children, relations }
```
- Type-specific parsers in `document/parser.rs` output into this same schema
- **PDF parser**: line-by-line processing with `SECTION_KEYWORDS` (~40 common headings) + heading heuristics (all-caps, numbered, title-case ≤5 words). Fixed in latest session to produce structured trees instead of flat blobs.
- Images get placeholder nodes (image extraction returns empty Vec — known stub)
- Metadata fields: `summary`, `entities`, `topics`, `page_number`, `word_count`

### Document Scope: Per-Chat, Not Global
Documents belong to individual conversations via a many-to-many join table:
```sql
conversation_documents (conv_id TEXT, doc_id TEXT, added_at TEXT)
```
- Same document can be in multiple chats (no duplication of file/tree data)
- DB schema at V3 — migration from V2 auto-runs on first launch
- IPC commands: `add_doc_to_conversation`, `remove_doc_from_conversation`, `get_conversation_doc_ids`
- Frontend: `conversationDocIds` in chat store replaces the old global `selectedDocumentIds`
- Sidebar shows docs for the active conversation only

### Query Enrichment Techniques (all real LLM calls)
- **Query Rewrite** (`query_rewrite` step): LLM reformulates user query for better search coverage
- **HyDE** (`hyde` step): LLM writes a hypothetical document passage → terms extracted from it match document vocabulary
- **StepBack** (`stepback` step): LLM generates a broader question → retrieves background context the specific query might miss
- All three are in `agent/query.rs`: `rewrite_query()`, `generate_hyde()`, `stepback_query()`
- Results enrich `ProcessedQuery.search_terms` before deterministic fetch

### Deterministic Content Fetcher
Intent → fetch strategy, all in `agent/deterministic.rs`:
- **Summarize**: expand top-level sections from all docs (uses metadata summaries if available)
- **Factual/Specific**: search enriched terms → expand matching nodes (max 12 per term)
- **Entity**: same as factual; entity terms from search term extraction
- **Comparison**: search + expand; enriched terms find cross-doc matches
- **ListExtract**: find Table/ListItem nodes → expand parent sections
- Content budget: 80,000 chars per query

### Cross-Document Relation Discovery (fully automatic)
- Runs during Phase 5 when `trees.len() > 1`
- In `document/metadata.rs`: `discover_cross_doc_relations()` compares entity/topic metadata across all doc pairs
- Relation types: `shared_entity` (confidence = shared_count / total), `topic_overlap` (≥2 shared topics)
- Persisted to `cross_doc_relations` table; loaded back into system prompt for context
- Frontend refreshes after each query: `relationsVersion` counter in chat store, incremented in `setIsExploring(false)`

### Tracing & Evaluation
Full local trace per query:
- `TraceRecord`: tokens (input + output), cost (from `pricing.json` per-model rates), total latency
- `StepRecord`: one per pipeline step (query_rewrite, hyde, stepback, tree_overview, search, expand, llm_call)
- Preprocessing steps have real token counts and latency
- Stored in SQLite, loaded in Session view (all steps across conversation history)
- Cost computed via `estimate_cost(model_id, input_tokens, output_tokens)` in `chat_handler.rs`

## Dead Code (scaffolding for future ReAct agent — not called by any live path)
These files exist but are NOT part of the active pipeline. Do not resurrect them without full wiring:
- `agent/runtime.rs` — `AgentRuntime`, `build_system_prompt()` — never called
- `agent/context.rs` — `ExplorationContext` — never called
- `agent/tools.rs` — all tool definitions, `get_openai_tool_definitions()`, `get_gemini_tool_definitions()` — never called
- `agent/tools.rs:RecordRelation` — intentional stub that errors

## Known Stubs / Not Yet Implemented
- **Local model inference**: `llm/local.rs` handles download UI + progress only. No inference engine. The download dialog and progress tracking work; calling inference fails.
- **PDF image extraction**: `document/image.rs` returns empty `Vec` (placeholder comment in code)
- **Vision LLM node description**: no code calls image nodes via LLM
- **Per-request cancel flags**: `AtomicBool` cancel flag checked before LLM call only, not between preprocessing steps

## UI Design

### Layout: 3-panel adaptive
```
┌──────────┬─────────────────────┬──────────────────┐
│ Sidebar  │    Main Chat        │  Preview Panel   │
│  200px   │    flexible         │   360px          │
│  Chats   │  + ThinkingBlocks   │  • Doc Tree      │
│  Docs    │    (steps visible)  │  • Relations     │
│  Config  │                     │  • Trace/Eval    │
└──────────┴─────────────────────┴──────────────────┘
```
- **Sidebar default tab: Chats** (not Documents)
- Conversations show doc count badge; clicking conversation loads its docs
- Documents tab shows docs for active conversation only; "Add Document" auto-attaches to active chat
- ThinkingBlock shows each pipeline step with icon, label, output preview, token badge, latency badge

### ThinkingBlock Step Labels
| tool key | label | icon |
|---|---|---|
| `query_rewrite` | Rewriting query | PenLine |
| `hyde` | Hypothetical answer (HyDE) | Sparkles |
| `stepback` | Broadening question (StepBack) | ArrowUpRight |
| `tree_overview` | Reading structure | FileText |
| `search` | Searching content | Search |
| `expand` | Reading section | BookOpen |
| `scan_lists` | Scanning lists & tables | List |
| `llm_call` | Generating answer | Cpu |

### Theme: Claude Desktop-inspired
- Light mode: Warm cream/pampas backgrounds (#F4F3EE), peach accent (#DE7356)
- Dark mode: Warm dark backgrounds, same accent system
- System auto-detect for theme switching
- Font: Inter (UI), JetBrains Mono (code/traces)
- Subtle shadows instead of borders, 4px border-radius on cards

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

## Code Conventions
- Rust: snake_case, modules in separate files, `thiserror` for errors, `serde` for serialization
- TypeScript: camelCase for variables/functions, PascalCase for components/types
- CSS: BEM-like with CSS modules, design tokens via custom properties
- All Tauri commands return `Result<T, String>` and are async
- Use `#[tauri::command]` for IPC endpoints
- Frontend state in Zustand stores, one store per domain (chat, documents, settings)
- No `#[allow(dead_code)]` on active code — only on scaffolding in runtime.rs/tools.rs/context.rs

## File Structure (complete, current)
```
src-tauri/src/
  main.rs, lib.rs
  pricing.json              — per-model token rates for cost estimation
  util.rs                   — shared utilities (safe_truncate, etc.)
  validation.rs             — path validation, input sanitization
  commands.rs               — Tauri IPC handlers (thin delegation only)

  document/
    mod.rs                  — re-exports
    parser.rs               — PDF, markdown, plaintext, code parsers; SECTION_KEYWORDS
    tree.rs                 — DocumentTree, TreeNode, NodeType
    image.rs                — image extraction (stub — returns empty Vec)
    cache.rs                — LRU tree cache (deserialization cache)
    metadata.rs             — heuristic entity/topic extraction, cross-doc relation discovery

  agent/
    mod.rs                  — re-exports
    query.rs                — heuristic preprocess + rewrite_query/generate_hyde/stepback_query
    deterministic.rs        — fetch_content() per-intent strategies, FetchedContent
    chat_handler.rs         — run_agent_chat() — THE ONLY ACTIVE PIPELINE FILE
    events.rs               — ChatEvent enum (StepStart, StepComplete, Token, Response, Error)
    runtime.rs              — DEAD CODE (future ReAct scaffold)
    context.rs              — DEAD CODE (future ReAct scaffold)
    tools.rs                — DEAD CODE (future ReAct scaffold)

  llm/
    mod.rs                  — re-exports
    provider.rs             — LLMProvider trait, Message, LLMResponse, ProviderConfig
    retry.rs                — RetryProvider wraps all providers with exponential backoff
    anthropic.rs            — Anthropic (real SSE streaming)
    openai_compat.rs        — OpenAI-compatible (OpenAI, DeepSeek, xAI, Qwen, AgentRouter)
    groq.rs                 — Groq
    google.rs               — Google AI Studio (Gemini)
    openrouter.rs           — OpenRouter
    local.rs                — Local GGUF download (inference stub)

  db/
    mod.rs                  — re-exports + CrossDocRelation, TraceRecord, StepRecord types
    schema.rs               — SQLite schema V3, migrations, all CRUD queries
    traces.rs               — trace-specific queries

src/
  App.tsx                   — root, theme init, layout
  stores/
    chat.ts                 — conversations, messages, explorationSteps, conversationDocIds, relationsVersion
    documents.ts            — document library (global), activeDocumentId
    settings.ts             — provider configs, active provider
  lib/
    tauri.ts                — typed invoke wrappers for all Tauri commands

  components/
    sidebar/
      Sidebar.tsx           — chats tab (default), docs tab (per-conversation), settings tab
    chat/
      ChatPanel.tsx         — message input, handleSend, event handler, ThinkingBlock list
      ThinkingBlock.tsx     — renders one pipeline step (running/complete states)
      ChatPanel.module.css
      ThinkingBlock.module.css
    preview/
      PreviewPanel.tsx      — right panel container with collapsible sections
      CanvasView.tsx        — SVG document tree visualization (zoom/pan)
      TraceView.tsx         — session-level token/cost/latency totals + step timeline
      RelationsView.tsx     — cross-doc relation list (refreshes via relationsVersion)
    common/
      ModelDownloadDialog.tsx — local model download UI + progress bar
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

## Key Decisions
- **No vector database, no embeddings** — deterministic tree navigation + LLM-powered query enrichment
- **Local-first**: all data in SQLite + filesystem; no cloud sync
- **Documents are per-chat**: `conversation_documents` join table; same doc reusable across chats
- **Streaming via Tauri Channels** (not events) — channels are high-throughput and ordered
- **3 preprocessing LLM calls** per query (Query Rewrite, HyDE, StepBack) — real calls, same provider
- **LLM never gets tools** — `tools: None` always; LLM only reads and answers
- **Preprocessing failures are non-fatal** — pipeline continues with partial enrichment
- **Cancel flag checked before LLM call** — checked 3 times during pipeline
- **Per-model cost tracking** — `pricing.json` lookup; falls back to `_default` rates
- **relationsVersion counter** — increments on `setIsExploring(false)`; triggers RelationsView re-fetch
- **Multi-document cross-referencing** is the north star differentiator

## Roadmap Status

| Milestone | Name | Status |
|-----------|------|--------|
| M1 | Solid Core | complete |
| M2 | Reliable Engine | complete |
| M3 | Quality Content | complete |
| M4 | Smooth Operation | complete |
| M5 | Smart Pipeline | **in progress** — deterministic pipeline done; local model inference stub |

Full roadmap: `.planning/roadmap/`

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
- New Rust utilities go in `util.rs` — never duplicated inline
- New Tauri commands go in `commands.rs` (thin) + domain module (logic)
- No business logic in `commands.rs` — it delegates only
- New LLM providers: one file in `llm/`, implement `LLMProvider` trait, add to factory in `chat_handler.rs`, add pricing entry
- Frontend IPC wrappers go in `lib/tauri.ts` — one per Tauri command

### Compilation / Test Gates
- `cargo check` must pass after every Rust change before moving on
- `npx tsc --noEmit` must pass after every TypeScript change
- Tests run with: `cargo test --lib db::`, `cargo test --lib document::parser`, `cargo test --lib agent::query`
- Tests in `commands.rs` / Tauri-dependent code cannot run in `cargo test` (no Tauri crate in test target) — this is expected and pre-existing

### Adding New Pipeline Steps
1. Add async function in `agent/query.rs` following `rewrite_query()` pattern (returns `EnrichmentResult`)
2. Export in `agent/mod.rs`
3. Add step block in `run_agent_chat()` with: StepStart → LLM call → StepComplete → trace_steps.push
4. Add step type → label/icon mapping in `ThinkingBlock.tsx`
5. Increment `step_counter` before each StepStart

### Security
- API keys encrypted at rest via OS keychain
- File paths validated before parsing — reject traversal patterns (`validation.rs`)
- No `rehype-raw` — use `rehype-sanitize` for safe HTML rendering
- LLM response content treated as untrusted text (no eval, no raw HTML injection)
