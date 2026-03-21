# AGENTS.md

This file provides guidance to agents when working with code in this repository.

## Critical Pipeline Rules (Non-Obvious)

- **LLM NEVER gets tools**: The `tools` parameter is always `None` in all LLM calls. This is a deterministic pipeline, not a ReAct agent.
- **Preprocessing errors are non-fatal**: If query_rewrite/hyde/stepback fail, emit "Skipped" StepComplete and continue the pipeline.
- **Content budget**: 40,000 chars per query (`CONTENT_BUDGET` in deterministic.rs) - enforced by fetcher, not LLM context window.
- **History budget**: 8,000 tokens for conversation history (`HISTORY_TOKEN_BUDGET` in chat_handler.rs).
- **All LLM providers auto-wrapped**: `RetryProvider` wraps every provider with exponential backoff - already done in `create_provider()`.
- **Streaming via Tauri Channels**: Use Channel API for high-frequency data (tokens, step events), not `app.emit()` events.

## SLM Engine (candle, in-process)

- `llm/slm.rs` — candle-based GGUF inference, replaces the old llama-server sidecar
- `llm/local.rs` — model download/management, delegates inference to `slm.rs`
- Metadata enrichment runs in the background after ingest (non-blocking)
- GGUF metadata remap handles Qwen2 architecture (synthesizes `rope.dimension_count`)

## Document Scoping (Critical Gotcha)

Documents are scoped per-conversation via `conversation_documents` join table (DB V3), NOT global.

```typescript
// CORRECT — use conversation-scoped doc IDs
const conversationDocIds = useChatStore((s) => s.conversationDocIds);

// WRONG — never use global document selection for queries
// selectedDocumentIds no longer exists in documents store
```

Removing a doc from chat = `removeDocFromConversation()`, NOT delete from library.

## Relations Refresh Pattern

```typescript
const relationsVersion = useChatStore((s) => s.relationsVersion);
useEffect(() => {
  // fetch relations
}, [docIds.join(','), relationsVersion]); // relationsVersion triggers re-fetch after each query
```

`relationsVersion` is incremented in `setIsExploring(false)` after each query completes.

## Known Stubs (Not Implemented)

- **Local model inference**: `llm/local.rs` — download + progress tracking work; inference fails (no llama-cpp-2 or ort integration).
- **PDF image extraction**: `document/image.rs` returns empty `Vec` — no actual image bytes extracted.
- **Per-step cancel**: `AtomicBool` cancel flag only checked before LLM call (Phase 7), not between preprocessing steps.
- **API key encryption**: Column name `api_key_encrypted` is aspirational; actual encryption not yet implemented.

## Cost Estimation

Costs computed from `src-tauri/src/pricing.json` (rates per million tokens). Add entries for new models:
```json
"model-id": { "input": 0.50, "output": 1.50 }
```

## Test Commands (Non-Standard)

```bash
# Rust (run from src-tauri/)
cargo check                          # compilation only
cargo test --lib db::                # DB schema + CRUD tests (15 tests)
cargo test --lib document::parser    # PDF/markdown parser tests (14 tests)
cargo test --lib agent::query        # query intent + enrichment tests (12 tests)

# TypeScript (run from project root)
npx tsc --noEmit                     # type check only
npx vitest run                       # frontend unit tests
```

**Critical**: `cargo test --lib` fails for modules that import `tauri` (e.g., `commands.rs`). This is pre-existing — the Tauri crate is not available in the test target. Run targeted module tests instead.

## Code Placement (Non-Obvious)

| What | Where |
|------|-------|
| New query enrichment technique | `agent/query.rs` — add async fn returning `EnrichmentResult` |
| New fetch strategy | `agent/deterministic.rs` — add fn, add match arm in `fetch_content()` |
| New pipeline phase | `agent/chat_handler.rs` — in `run_agent_chat()` |
| New Tauri IPC command | `commands.rs` (thin) + domain module (logic) + `lib.rs` handler registration + `lib/tauri.ts` wrapper |
| New LLM provider | `llm/{name}.rs` + `llm/mod.rs` re-export + factory match arm in `chat_handler.rs` + `pricing.json` entry |
| Shared Rust utilities | `src-tauri/src/util.rs` — not inline, not duplicated |
| Input validation | `src-tauri/src/validation.rs` |
| Frontend state | `src/stores/{domain}.ts` — one Zustand store per domain |
| Frontend IPC wrappers | `src/lib/tauri.ts` — typed `invoke()` wrappers |
| Step UI labels/icons | `src/components/chat/ThinkingBlock.tsx` — `getStepDisplay()` switch |

## Security Gotchas

- **LLM output sanitization**: Use `rehype-sanitize` only, never `rehype-raw` for LLM output rendering.
- **Path validation**: All file paths validated in `validation.rs` before parsing — rejects `..` traversal patterns.
- **API key storage**: Column named `api_key_encrypted` but actual encryption not implemented (known security gap).

## Adding Pipeline Steps

1. Add async fn in `agent/query.rs` returning `EnrichmentResult`
2. Export in `agent/mod.rs`
3. Add step block in `run_agent_chat()` with: StepStart → LLM call → StepComplete → `trace_steps.push`
4. Add step type → label/icon mapping in `ThinkingBlock.tsx`
5. Increment `step_counter` before each StepStart (global counter for sequential UI step numbers)

## DB Schema (V3)

- Schema managed in `db/schema.rs`
- To add table: add `CREATE TABLE IF NOT EXISTS` in `initialize()` AND add migration block in `migrate()` with version guard
- Always bump `LATEST_VERSION` when adding migration
- Migrations are forward-only — no rollback
- All queries use `?` placeholders (rusqlite positional params)
- All IDs are UUID strings — use `uuid::Uuid::new_v4().to_string()`
- Timestamps are RFC3339 strings — use `chrono::Utc::now().to_rfc3339()`

## Chat Event Order (Critical)

Events arrive via Tauri Channel in this order per query:
1. N× `step-start` / `step-complete` pairs (preprocessing + fetch + llm_call)
2. N× `token` events (streaming tokens, `done: false`)
3. One `token` event with `done: true`
4. One `response` event (complete answer)

The `response` event is the signal to: save assistant message, call `clearSteps()`, call `setIsExploring(false)`.
