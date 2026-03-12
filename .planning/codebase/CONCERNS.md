# Codebase Concerns

**Analysis Date:** 2026-03-12

---

## Active Tech Debt

**API keys stored in plaintext:**
- Risk: The DB column is named `api_key_encrypted` but the code stores the raw API key with zero encryption. `save_provider` in `db/schema.rs` writes `config.api_key` directly.
- Current mitigation: None. The SQLite database file sits in the user's app data directory unencrypted.
- Fix: Use the OS keychain via the `keyring` crate (Windows Credential Manager / macOS Keychain / Linux Secret Service).

**PDF image extraction is a stub:**
- `document/image.rs` returns an empty `Vec`. PDF and DOCX embedded images are silently lost.
- Fix: Implement PDF image extraction using `lopdf` or shell out to Poppler. For DOCX, extract from `word/media/` in the zip archive.

**Local model inference is a stub:**
- `llm/local.rs` handles download + progress tracking only. Calling inference fails.
- Fix: Integrate `llama.cpp` or `candle` for GGUF model inference.

**Per-request cancel flags:**
- A single global `AtomicBool` cancel flag is checked at 3 points but NOT between preprocessing steps. Cancellation is approximate.
- Fix: Associate cancel flags with request IDs rather than using a global singleton.

**No database migration framework:**
- Migrations are handled by ad-hoc `ALTER TABLE` checks in `db/schema.rs`. No versioning, no rollback.
- Fix: Add a `schema_version` table and numbered migration functions, or adopt `rusqlite_migration`.

**CSV/XLSX parser creates a tree node per cell:**
- A 500-row, 20-column spreadsheet generates 10,000+ tree nodes with UUID + metadata overhead.
- Fix: Represent rows as single nodes with cell data in metadata/structured content.

---

## Known Stubs Documented

These are not bugs — they are intentional stubs with known scope:
- `document/image.rs` — image extraction returns empty Vec (no PDF/DOCX image support)
- `llm/local.rs` — Ollama download works; inference call is a stub
- `agent/runtime.rs`, `agent/context.rs`, `agent/tools.rs` — dead code scaffolding for a future ReAct agent; intentionally inert

---

## Fragile Areas

**PDF heading detection heuristic (`document/parser.rs`):**
- `detect_heading()` uses string heuristics (ALL CAPS, line length, ending punctuation, title-case ≤5 words). Produces false positives on short text lines.
- `split_fused_heading()` and `split_leading_keyword()` handle two known pdf_extract artefacts (no-space fusion and keyword+space+content merging) but cannot handle all PDF layout pathologies.
- Safe to modify: extend `SECTION_KEYWORDS`, adjust the title-case length threshold, or add new splitting patterns. Tests exist in `parser.rs`.

**Cancel flag race condition:**
- A single global cancel flag is shared across all queries. If two queries ran concurrently (unlikely but possible via rapid UI interaction), cancelling one cancels the other.

---

## Performance Bottlenecks

**Full document tree loaded from JSON per query:**
- `get_document` deserializes the entire `tree_json` blob from SQLite on every call. An LRU cache (`document/cache.rs`) mitigates this for repeated access.
- Limit: Documents with 10,000+ nodes produce multi-megabyte JSON blobs (~100ms+ deserialization).

**Global `Mutex<Database>` blocks all commands:**
- A single `Mutex<Database>` wraps the entire SQLite connection. During `run_agent_chat`, the mutex is acquired multiple times. Other commands block while the lock is held.
- Fix: Use `r2d2` connection pooling or `tokio-rusqlite` for async database access.

---

## Resolved (from previous audit)

These concerns from the 2026-03-05 audit have been fixed:

| Concern | Resolution |
| :--- | :--- |
| Monolithic `commands.rs` (430+ line chat function) | Extracted into `agent/chat_handler.rs` |
| Duplicated `safe_truncate` | Moved to `util.rs`, imported everywhere |
| Fake streaming (simulated word chunking) | Real SSE streaming in all major providers |
| Zero test coverage | Tests added for `db::`, `document::parser`, `agent::query` |
| `rehype-raw` allows raw HTML in chat responses | Switched to `rehype-sanitize` |
| Tool loop making 11+ LLM calls per query (rate limit exhaustion) | Tool loop removed; pipeline makes 4 calls max (3 enrichment + 1 streaming) |
| `ExplorationContext.has_explored` O(n) linear scan | Context module is now dead code (not called by pipeline) |

---

*Updated: 2026-03-12*
