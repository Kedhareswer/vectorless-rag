# AGENTS.md — AI Agent Guidelines for TGG

## Must Read First

Before touching any code, read:
- `CLAUDE.md` — complete project overview, pipeline, tech stack, architecture, current state
- `.planning/roadmap/OVERVIEW.md` — milestone map (M1-M5)
- `.planning/roadmap/M5-SMART-PIPELINE.md` — current active milestone

The most important thing to understand: **this is a deterministic pipeline, not a ReAct agent**.

---

## The Actual Pipeline (memorise this)

```
User query
  → Phase 1:  heuristic intent classification + search terms (sync, ~0ms)
  → Phase 3a: Query Rewrite LLM call (real, non-streaming)
  → Phase 3b: HyDE LLM call (real, non-streaming)
  → Phase 3c: StepBack LLM call (real, non-streaming)
  → Phase 4:  deterministic fetch (code reads doc trees, no LLM)
  → Phase 5:  cross-doc relation discovery (if >1 doc, persists to DB)
  → Phase 6:  load conversation history
  → Phase 8:  ONE streaming LLM call (no tools, system prompt = doc content)
  → Phase 9:  save full trace to SQLite
```

All code for this lives in: `src-tauri/src/agent/chat_handler.rs`

---

## Absolute Rules (never violate these)

### DO NOT
- Send tools to the LLM. Ever. The `tools` parameter is **always `None`**.
- Add a loop around the LLM call. One call. Done.
- Write placeholder, fake, demo, or simulated responses anywhere in the pipeline.
- Treat documents as global state — they are scoped per conversation via `conversation_documents` table.
- Add code to `agent/runtime.rs`, `agent/context.rs`, or `agent/tools.rs` — they are dead code scaffolding.
- Copy-paste utilities — add shared code to `src-tauri/src/util.rs`.
- Add new code to `commands.rs` beyond thin delegation — all logic goes in domain modules.
- Skip `cargo check` after Rust changes or `tsc --noEmit` after TypeScript changes.
- Use `rehype-raw` — only `rehype-sanitize` for LLM output rendering.
- Store API keys in plaintext — use OS keychain.
- Delete documents from the library when removing from a conversation — only detach from the join table.

### DO
- Run `cargo check` after every Rust change.
- Run `npx tsc --noEmit` after every TypeScript change.
- Handle preprocessing step errors (query_rewrite / hyde / stepback) gracefully — emit "Skipped" StepComplete and continue the pipeline.
- Add every new pipeline step to `ThinkingBlock.tsx` step display map.
- Persist all pipeline steps (including preprocessing) to the `steps` table in SQLite.
- Use `conversationDocIds` from the chat store, not any global document selection.
- Enrich `processed.search_terms` with terms from preprocessing LLM outputs via `extract_terms_from_text()`.
- Use `relationsVersion` in `useChatStore` to trigger relation refreshes after queries.
- Keep `step_counter` incrementing globally across all phases so UI step numbers are sequential.

---

## Current State Summary

### What is fully implemented and working
- All 9 pipeline phases in `chat_handler.rs`
- Query Rewrite, HyDE, StepBack — real LLM calls, real token/latency/cost tracking
- Deterministic fetcher with 5 intent strategies in `deterministic.rs`
- All 10 LLM providers (Anthropic, OpenAI, Groq, Google, OpenRouter, AgentRouter, DeepSeek, xAI, Qwen, Ollama)
- `RetryProvider` wrapping all providers with exponential backoff
- Anthropic real SSE streaming
- Streaming via Tauri Channels (not events)
- Per-chat document scoping via `conversation_documents` join table (DB V3)
- Cross-doc relation discovery: metadata-based, automatic, persisted, refreshed post-query
- Full trace persistence: TraceRecord + StepRecord for every pipeline step
- PDF parser: line-by-line processing, SECTION_KEYWORDS (~40 headings), heading heuristics
- Conversation persistence (no new chat per prompt)
- Onboarding dialog on first launch
- `relationsVersion` counter for live relation refresh
- ModelDownloadDialog for local model download

### What is a stub / not implemented
- **Local model inference**: `llm/local.rs` only handles download, no inference engine. `llama-cpp-2` or `ort` not yet integrated.
- **PDF image extraction**: `document/image.rs` returns empty `Vec`. No actual image bytes extracted.
- **Vision LLM node description**: nothing calls an LLM to describe image nodes.
- **Per-step cancel**: cancel flag only checked before LLM call (Phase 7), not between preprocessing steps.

### Dead code (do not call, do not delete without plan)
- `agent/runtime.rs` — `AgentRuntime`, `build_system_prompt()`, entire file
- `agent/context.rs` — `ExplorationContext`, entire file
- `agent/tools.rs` — all tool definitions and format functions, entire file

---

## Code Placement Rules

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

---

## Adding a New Pipeline Step (step-by-step)

1. In `agent/query.rs`: add `pub async fn my_step(provider: &dyn LLMProvider, query: &str) -> Result<EnrichmentResult, String>`
2. In `agent/mod.rs`: add to `pub use query::{..., my_step}` export list
3. In `agent/chat_handler.rs` inside `run_agent_chat()`:
   ```rust
   step_counter += 1;
   let _ = channel.send(ChatEvent::StepStart {
       step_number: step_counter,
       tool: "my_step".to_string(),
       input_summary: format!("...", &message),
   });
   let start = tokio::time::Instant::now();
   match my_step(provider.as_ref(), &message).await {
       Ok(result) => {
           // merge result.text terms into processed.search_terms
           total_input_tokens += result.input_tokens;
           total_output_tokens += result.output_tokens;
           let latency = start.elapsed().as_millis() as u64;
           let cost = estimate_cost(&model_id, result.input_tokens, result.output_tokens);
           let _ = channel.send(ChatEvent::StepComplete { step_number: step_counter, output_summary: result.text.clone(), tokens_used: result.input_tokens + result.output_tokens, latency_ms: latency, cost, node_ids: vec![] });
           trace_steps.push(PipelineStep { tool: "my_step".to_string(), input: message.clone(), output: result.text, tokens: (result.input_tokens + result.output_tokens) as i64, latency_ms: latency as i64 });
       }
       Err(e) => { /* emit Skipped StepComplete, push error trace_step */ }
   }
   ```
4. In `ThinkingBlock.tsx` `getStepDisplay()`: add `case 'my_step': return { icon: SomeIcon, label: 'Human label' };`

---

## Adding a New LLM Provider

1. Create `src-tauri/src/llm/{provider}.rs` — implement `LLMProvider` trait:
   - `chat()` — non-streaming, returns `LLMResponse`
   - `chat_stream()` — streaming, sends tokens via `token_tx: UnboundedSender<String>`
   - `capabilities()` — fill in `supports_streaming`, `supports_vision`, etc.
   - `name()` — return provider display name
2. Add `pub mod {provider};` and `pub use {provider}::{ProviderType};` in `llm/mod.rs`
3. Add match arm in `create_provider()` in `chat_handler.rs`
4. Wrap with `RetryProvider::new(inner)` — already done by `create_provider()`
5. Add pricing entry in `src-tauri/src/pricing.json`:
   ```json
   "model-id": { "input": 0.50, "output": 1.50 }
   ```
   (rates per million tokens)
6. Add to settings UI provider dropdown in `SettingsModal.tsx`

---

## DB Schema Rules

Schema is managed in `db/schema.rs`. Current version: **V3**.

- To add a new table: add `CREATE TABLE IF NOT EXISTS` in `initialize()` AND add a migration block in `migrate()` with version guard.
- Always bump `LATEST_VERSION` when adding a migration.
- Migrations are forward-only — no rollback.
- All queries use `?` placeholders (rusqlite positional params).
- All IDs are UUID strings — use `uuid::Uuid::new_v4().to_string()`.
- Timestamps are RFC3339 strings — use `chrono::Utc::now().to_rfc3339()`.

---

## Frontend Rules

### State Management
- `useChatStore` — conversations, messages, explorationSteps, conversationDocIds, relationsVersion, sessionTotals
- `useDocumentsStore` — document library (global), activeDocumentId
- `useSettingsStore` — provider configs, active provider
- Do NOT add cross-store subscriptions in component renders — use store actions that call other stores.

### Document Scoping (critical)
```typescript
// CORRECT — use conversation-scoped doc IDs
const conversationDocIds = useChatStore((s) => s.conversationDocIds);

// WRONG — never use this for query context
const { selectedDocumentIds } = useDocumentsStore(); // this field no longer exists
```

### Relations Refresh Pattern
```typescript
// In RelationsView and anywhere showing cross-doc relations:
const relationsVersion = useChatStore((s) => s.relationsVersion);
useEffect(() => {
  // fetch relations
}, [docIds.join(','), relationsVersion]); // relationsVersion triggers re-fetch after each query
```

### Chat Event Handling
Events arrive via Tauri Channel in this order per query:
1. N× `step-start` / `step-complete` pairs (preprocessing + fetch + llm_call)
2. N× `token` events (streaming tokens, `done: false`)
3. One `token` event with `done: true`
4. One `response` event (complete answer)

The `response` event is the signal to: save assistant message, call `clearSteps()`, call `setIsExploring(false)`.

---

## Test Commands

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

Note: `cargo test --lib` fails for modules that import `tauri` (e.g. `commands.rs`). This is pre-existing — the Tauri crate is not available in the test target. Run targeted module tests instead.

---

## Architecture Principles

### Multi-Document Cross-Referencing (North Star)
- All fetch strategies must work across multiple document trees
- `fetch_content()` iterates `trees: &[DocumentTree]` — always plural
- Relations are discovered, persisted, and fed back into the system prompt
- The system prompt includes a "Known relations" section when >1 doc is loaded
- UI shows relations panel which refreshes after each query

### Streaming
- Use Tauri v2 Channel API for all high-frequency data (tokens, step events)
- `app.emit()` events are for low-frequency notifications only
- Channel is ordered and fast — never use for one-shot signals

### Error Handling
- Rust: `thiserror` enums in domain code, converted to `String` at Tauri command boundary
- All Tauri commands return `Result<T, String>`
- Frontend: `try/catch` with `console.warn` for non-critical, store error state for user-visible
- LLM errors: `RetryProvider` handles transient errors; fatal errors emitted as `ChatEvent::Error`

### Security
- Never store secrets in plaintext — OS keychain via `tauri-plugin-store` or equivalent
- Validate file paths before filesystem access (`validation.rs`)
- Sanitize all LLM output — no `rehype-raw`, only `rehype-sanitize`
- Validate all user input at the Tauri command boundary

---

## Research References

- [PageIndex](https://github.com/VectifyAI/PageIndex) — Vectorless RAG with tree search, 98.7% on FinanceBench. Python library, single-doc, no UI. TGG differentiates via multi-doc, desktop-native, full pipeline transparency.
- [HyDE](https://arxiv.org/abs/2212.10496) — Hypothetical Document Embeddings. TGG uses the concept without embeddings: hypothetical passage → term extraction → tree search.
- [StepBack Prompting](https://arxiv.org/abs/2310.06117) — Generalize before retrieving. Implemented as Phase 3c.
- [A-RAG](https://arxiv.org/abs/2602.03442) — Hierarchical retrieval interfaces. Validates multi-granularity approach.
- [Tauri v2 Channels](https://v2.tauri.app/develop/calling-frontend/) — Official docs for streaming from Rust to frontend.
