# Codebase Concerns

**Analysis Date:** 2026-03-05

## Tech Debt

**Monolithic `commands.rs` (1000 lines):**
- Issue: The entire Tauri IPC surface lives in a single file `src-tauri/src/commands.rs`. The `chat_with_agent` function alone spans 430+ lines (lines 520-952) with inline event structs, tool execution, message assembly, pre-search logic, streaming, and trace persistence all interleaved.
- Files: `src-tauri/src/commands.rs`
- Impact: Difficult to test, modify, or extend the agent chat flow. Any change to tool execution, streaming, or tracing risks breaking the other concerns.
- Fix approach: Extract `chat_with_agent` into its own module (e.g., `src-tauri/src/agent/chat_handler.rs`). Separate event emission, message building, and trace saving into helper modules.

**Duplicated `safe_truncate` function:**
- Issue: The `safe_truncate` helper is copy-pasted in both `src-tauri/src/commands.rs` (line 11) and `src-tauri/src/agent/runtime.rs` (line 7). A third inline version exists in `src-tauri/src/document/tree.rs` (lines 166-171).
- Files: `src-tauri/src/commands.rs`, `src-tauri/src/agent/runtime.rs`, `src-tauri/src/document/tree.rs`
- Impact: Bug fixes or behavior changes must be applied in three places.
- Fix approach: Move to a shared utility module (e.g., `src-tauri/src/util.rs`) and import everywhere.

**Image extraction is a stub:**
- Issue: `extract_images_from_path` in `src-tauri/src/document/image.rs` returns an empty `Vec` with a comment saying "Currently a stub." Vision-capable LLM providers are listed, but no images are ever extracted from PDFs or DOCX files.
- Files: `src-tauri/src/document/image.rs`
- Impact: The `get_image` agent tool exists but image nodes are only created for Markdown inline images (with URL references). PDF and DOCX embedded images are silently lost.
- Fix approach: Implement PDF image extraction using `pdf_extract` or `lopdf` crate, and DOCX image extraction from the zip archive's `word/media/` folder.

**Fake streaming (simulated word chunking):**
- Issue: All LLM provider implementations have `supports_streaming: false`. The `emit_streaming_response` function in `src-tauri/src/commands.rs` (line 384) splits the already-complete response into word chunks and emits them with `yield_now()` to simulate streaming. This is not true token streaming.
- Files: `src-tauri/src/commands.rs` (lines 382-396), all provider files in `src-tauri/src/llm/`
- Impact: Users experience artificial delay on the final response. The full response is already available but is drip-fed. No benefit during the actual LLM wait.
- Fix approach: Implement actual SSE/streaming in at least the major providers (OpenAI-compat, Anthropic, Google) using their streaming APIs. The `LLMProvider` trait needs a streaming variant.

**No database migration framework:**
- Issue: Migrations are handled by a single `run_migrations` method in `src-tauri/src/db/schema.rs` (line 144) that checks for column existence with `SELECT ... LIMIT 0` and runs raw `ALTER TABLE`. No versioning, no rollback, no migration tracking table.
- Files: `src-tauri/src/db/schema.rs` (lines 144-156)
- Impact: As the schema evolves, this pattern becomes increasingly fragile. Adding more migrations means chaining more ad-hoc column existence checks.
- Fix approach: Add a `schema_version` table and numbered migration functions. Or adopt `refinery` or `rusqlite_migration` crate.

## Security Considerations

**API keys stored in plaintext:**
- Risk: The database column is named `api_key_encrypted` but the code stores and retrieves the raw API key with zero encryption. `save_provider` in `src-tauri/src/db/schema.rs` (line 340) writes `config.api_key` directly to the `api_key_encrypted` column. `get_providers` (line 357) reads it back as plain text.
- Files: `src-tauri/src/db/schema.rs` (lines 338-375), `src-tauri/src/llm/provider.rs` (line 107)
- Current mitigation: None. The SQLite database file sits in the user's app data directory unencrypted.
- Recommendations: Use the OS keychain (Windows Credential Manager / macOS Keychain / Linux Secret Service) via a crate like `keyring`. At minimum, encrypt at rest with a key derived from the machine ID.

**Unsanitized file path in `ingest_document`:**
- Risk: The `ingest_document` command in `src-tauri/src/commands.rs` (line 48) accepts a `file_path: String` directly from the frontend and passes it to parsers that call `std::fs::read_to_string` and `std::fs::read`. No path validation, traversal protection, or allowed-directory checks.
- Files: `src-tauri/src/commands.rs` (lines 48-58), `src-tauri/src/document/parser.rs`
- Current mitigation: Tauri v2 has IPC scope rules, but the command itself does no validation.
- Recommendations: Validate paths against allowed directories or use Tauri's scoped filesystem APIs. Reject paths with `..` traversal patterns.

**`rehype-raw` allows raw HTML in chat responses:**
- Risk: `ChatPanel.tsx` uses `rehypeRaw` plugin with ReactMarkdown (line 283), which renders raw HTML from LLM responses. If an LLM is tricked into returning malicious HTML/JS, it could execute in the WebView.
- Files: `src/components/chat/ChatPanel.tsx` (line 283)
- Current mitigation: None.
- Recommendations: Remove `rehype-raw` or add `rehype-sanitize` to strip dangerous elements. In a desktop app the WebView has filesystem access, making XSS particularly dangerous.

## Performance Bottlenecks

**Synchronous file I/O on the Tauri command thread:**
- Problem: `ingest_document` is a synchronous `#[tauri::command]` (not `async`). It calls `std::fs::read_to_string` and `std::fs::read` which block the thread. For large PDFs or XLSX files, this blocks the entire IPC handler.
- Files: `src-tauri/src/commands.rs` (lines 48-58), `src-tauri/src/document/parser.rs` (lines 36, 169, 193, 256)
- Cause: Parsers use synchronous `std::fs` operations. The command is not marked `async`.
- Improvement path: Make `ingest_document` async, use `tokio::task::spawn_blocking` for file I/O and parsing. This frees the IPC thread for other commands during parsing.

**Global `Mutex<Database>` blocks all commands during queries:**
- Problem: A single `Mutex<Database>` wraps the entire SQLite connection. During `chat_with_agent`, the mutex is acquired multiple times (initial setup, history loading, trace saving). While the LLM call itself runs outside the lock, any other command (list_documents, get_providers, etc.) blocks if the mutex is held.
- Files: `src-tauri/src/lib.rs` (line 57), `src-tauri/src/commands.rs` (every command)
- Cause: `rusqlite::Connection` is not `Send + Sync`, so it must be wrapped in a mutex.
- Improvement path: Use `r2d2` connection pooling with `rusqlite`, or switch to `tokio-rusqlite` for async database access. Alternatively, use separate connections for reads vs writes.

**Full document tree loaded from JSON for every operation:**
- Problem: `get_document` deserializes the entire `tree_json` blob from SQLite on every call. During `chat_with_agent`, all document trees are loaded at once (line 548). For large documents with thousands of nodes, this means deserializing megabytes of JSON per query.
- Files: `src-tauri/src/db/schema.rs` (lines 179-196), `src-tauri/src/commands.rs` (lines 542-561)
- Cause: The entire tree is stored as a single JSON blob rather than normalized into rows.
- Improvement path: Cache parsed `DocumentTree` objects in memory (LRU cache keyed by doc ID). Invalidate on document update/delete.

**Linear search in `ExplorationContext.has_explored`:**
- Problem: `explored_nodes` is a `Vec<String>` checked with `.contains()` (O(n) linear scan) instead of a `HashSet`.
- Files: `src-tauri/src/agent/context.rs` (lines 30, 39-41)
- Cause: Simple initial implementation.
- Improvement path: Change `explored_nodes` to `HashSet<String>` or maintain both a Vec (for ordering) and a HashSet (for lookups).

**CSV/XLSX parser creates a tree node per cell:**
- Problem: The CSV and XLSX parsers create individual `TreeNode` objects for every cell in every row (up to 500 rows). A 500-row, 20-column spreadsheet generates 10,000+ tree nodes, each with a UUID, metadata HashMap, and serialization overhead.
- Files: `src-tauri/src/document/parser.rs` (lines 556-568 for CSV, lines 620-629 for XLSX)
- Cause: The Universal Document Tree design treats every cell as a node.
- Improvement path: Represent rows as single nodes with cell data in metadata or structured content, rather than individual cell nodes.

## Fragile Areas

**Anthropic message format conversion:**
- Files: `src-tauri/src/llm/anthropic.rs` (lines 50-119)
- Why fragile: The code manually converts between OpenAI-format tool call storage (used internally) and Anthropic's native `tool_use`/`tool_result` content block format. The conversion involves parsing `raw_tool_calls` stored in OpenAI format, extracting function name/arguments, and restructuring into Anthropic blocks. Any change to the internal message format or either API's schema breaks this silently.
- Safe modification: Add integration tests with recorded API responses. Consider using a unified internal message format that doesn't assume OpenAI structure.
- Test coverage: None.

**Google Gemini special-case tool definitions:**
- Files: `src-tauri/src/commands.rs` (lines 436-458)
- Why fragile: `build_llm_tools` has a special branch for Google provider that swaps parameter schemas with `get_gemini_tool_definitions()`. This means Google tool definitions can silently diverge from other providers if someone updates tool definitions but forgets the Gemini variant.
- Safe modification: Unify tool definition generation so Gemini-specific transformations are applied automatically rather than maintained as a parallel set.
- Test coverage: None.

**PDF heading detection heuristic:**
- Files: `src-tauri/src/document/parser.rs` (lines 221-251)
- Why fragile: The `detect_heading` function uses string heuristics (ALL CAPS check, line length, ending punctuation) to guess if a line is a heading. This produces false positives on short text lines and false negatives on actual headings that don't match the patterns.
- Safe modification: Consider providing a fallback flat structure (paragraphs only) with an option for heuristic sectioning. Allow users to re-parse with different heuristic sensitivity.
- Test coverage: None.

**Cancel flag race condition:**
- Files: `src-tauri/src/commands.rs` (lines 8, 288-292, 531, 706-716)
- Why fragile: A single global `CancelFlag` is shared across all potential queries. If two queries somehow run concurrently (unlikely but possible via rapid UI interaction), cancelling one cancels the other. The flag is reset at the start of `chat_with_agent` (line 531) but there's no query-specific cancellation token.
- Safe modification: Associate cancel flags with request IDs rather than using a global singleton.
- Test coverage: None.

## Scaling Limits

**SQLite single-writer limitation:**
- Current capacity: Single user, single concurrent writer.
- Limit: WAL mode helps concurrent reads but only one write transaction can proceed at a time. The `Mutex<Database>` makes this even more restrictive by serializing all access.
- Scaling path: For a desktop app this is likely sufficient. If multi-window or plugin support is added, switch to connection pooling.

**Document tree JSON blob storage:**
- Current capacity: Works for documents with hundreds of nodes.
- Limit: Documents with 10,000+ nodes produce multi-megabyte JSON blobs. SQLite handles it but deserialization becomes the bottleneck (~100ms+ for 5MB JSON).
- Scaling path: Normalize node storage into a `nodes` table, or implement in-memory caching with lazy loading.

**Conversation history truncation (last 10 messages):**
- Current capacity: Short conversations work well.
- Limit: The hard-coded limit of 10 recent history messages (line 659 in `commands.rs`) means long conversations lose context. The context window trimming at 60 messages (line 702) is also a hard limit.
- Scaling path: Implement sliding window with summarization of older messages, or use the LLM's full context window more intelligently based on the model's actual token limit.

## Dependencies at Risk

**`pdf_extract` crate:**
- Risk: Limited PDF support. Returns empty string for scanned/image-only PDFs. Does not extract images, tables, or structured content. The crate is not actively maintained.
- Impact: PDF parsing quality is low for complex documents. Users get placeholder messages for scanned PDFs.
- Migration plan: Consider `lopdf` + `pdf` crate combination, or shell out to `pdftotext`/`Poppler` for better extraction. For scanned PDFs, integrate OCR (Tesseract).

## Missing Critical Features

**No error retry or rate limiting for LLM API calls:**
- Problem: If an LLM API returns a 429 (rate limit) or 500 (server error), the entire agent loop fails immediately. No exponential backoff, no retry logic.
- Blocks: Reliable operation with rate-limited free tiers (Groq, Google AI Studio).
- Files: All provider files in `src-tauri/src/llm/`, `src-tauri/src/commands.rs` (lines 730-745)

**No input validation on provider configuration:**
- Problem: Users can save providers with empty model names, invalid URLs, or malformed API keys. No validation happens until the first chat attempt fails.
- Blocks: Good user experience; errors surface late and are confusing.
- Files: `src-tauri/src/commands.rs` (lines 129-135), `src/components/settings/SettingsModal.tsx`

## Test Coverage Gaps

**Zero test coverage across the entire codebase:**
- What's not tested: Everything. There are no Rust `#[test]` blocks, no `#[cfg(test)]` modules, no frontend test files (`.test.ts`, `.spec.ts`), no test configuration (jest/vitest).
- Files: All files in `src-tauri/src/` and `src/`
- Risk: Any refactoring or feature change can silently break existing functionality. The agent loop, message format conversions, parser logic, database queries, and frontend state management are all untested.
- Priority: **High**. Critical areas to test first:
  1. Document parsers (`src-tauri/src/document/parser.rs`) - input/output validation for each format
  2. Agent tool execution (`src-tauri/src/agent/runtime.rs`) - tool routing, budget enforcement, edge cases
  3. LLM message format conversion (`src-tauri/src/llm/anthropic.rs`, `src-tauri/src/llm/google.rs`) - API format compatibility
  4. Database CRUD operations (`src-tauri/src/db/schema.rs`, `src-tauri/src/db/traces.rs`) - schema integrity
  5. Query preprocessing (`src-tauri/src/agent/query.rs`) - intent classification, search term extraction

---

*Concerns audit: 2026-03-05*
