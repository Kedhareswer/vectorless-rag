# Architecture

**Analysis Date:** 2026-03-05

## Pattern Overview

**Overall:** Tauri v2 Desktop Application with IPC-based Client-Server Architecture

**Key Characteristics:**
- React frontend communicates with Rust backend exclusively through Tauri IPC `invoke` calls
- Backend uses a single-threaded `Mutex<Database>` for all SQLite access
- Real-time events flow from backend to frontend via Tauri `app.emit()` (event bus pattern)
- Agent loop runs as an async Tauri command, emitting streaming events per tool call and per token
- No REST API or HTTP server; all communication is in-process via Tauri's IPC bridge

## Layers

**Presentation Layer (React Frontend):**
- Purpose: UI rendering, user interaction, local state management
- Location: `src/`
- Contains: React components (TSX), Zustand stores, CSS modules, Tauri invoke wrappers
- Depends on: Tauri IPC (`@tauri-apps/api/core` invoke), Tauri events (`listen`)
- Used by: End user via WebView

**IPC Bridge (Tauri Commands):**
- Purpose: Translate frontend requests into backend function calls; serialize results as JSON
- Location: `src-tauri/src/commands.rs`
- Contains: All `#[tauri::command]` handler functions, event structs, provider factory, agent loop orchestration
- Depends on: All backend modules (agent, document, llm, db)
- Used by: Frontend via `invoke()` calls defined in `src/lib/tauri.ts`

**Document Engine:**
- Purpose: Parse files into Universal Document Trees (UDT)
- Location: `src-tauri/src/document/`
- Contains: Parser trait + implementations (Markdown, PDF, DOCX, CSV, XLSX, Code, PlainText), tree data structures, image extraction
- Depends on: `pulldown-cmark`, `pdf-extract`, `calamine`, `csv`, `zip`, `quick-xml`
- Used by: Commands layer (ingest_document)

**Agent Runtime:**
- Purpose: Execute document exploration tools, manage exploration context and budgets
- Location: `src-tauri/src/agent/`
- Contains: Tool definitions, tool execution logic, exploration context tracking, query preprocessing/classification
- Depends on: Document Engine (reads DocumentTree)
- Used by: Commands layer (chat_with_agent)

**LLM Provider Layer:**
- Purpose: Unified interface to 10+ LLM providers with tool-calling support
- Location: `src-tauri/src/llm/`
- Contains: `LLMProvider` async trait, provider implementations (one file per provider)
- Depends on: `reqwest` (HTTP), `async-trait`
- Used by: Commands layer (chat_with_agent calls `provider.chat()`)

**Persistence Layer:**
- Purpose: Store documents, conversations, traces, providers, settings, bookmarks
- Location: `src-tauri/src/db/`
- Contains: SQLite schema, CRUD operations, trace/eval storage, migrations
- Depends on: `rusqlite`
- Used by: Commands layer (all commands access `State<Mutex<Database>>`)

## Data Flow

**Document Ingestion:**

1. User clicks "Add Document" in Sidebar, triggers `open_file_dialog` Tauri command
2. File dialog returns a path; frontend calls `ingest_document(filePath)` via IPC
3. `commands::ingest_document` dispatches to `get_parser_for_file()` based on file extension
4. Parser reads file, builds a `DocumentTree` with nodes in a `HashMap<String, TreeNode>`
5. Tree is serialized to JSON and stored in SQLite `documents.tree_json`
6. Full `DocumentTree` returned to frontend; `useDocumentsStore` updates state

**Agent Chat Query:**

1. User sends message; frontend calls `chatWithAgent(message, docIds, providerId, convId)` via IPC
2. `commands::chat_with_agent` loads document trees and provider config from DB (inside Mutex lock, then released)
3. Query is preprocessed: `preprocess_query()` classifies intent, extracts search terms, computes adaptive step budget
4. System prompt is built with tree overview and exploration hints
5. Pre-search/pre-expand runs against primary tree for targeted queries (separate throwaway runtime)
6. Agent loop begins (up to `adaptive_max_steps` iterations):
   a. Check `CancelFlag` AtomicBool; abort if set
   b. Call `provider.chat(messages, tools)` with conversation history + tool definitions
   c. If no tool calls returned: check minimum tool call threshold, nudge agent if under minimum, else emit final answer
   d. If tool calls returned: execute each tool via `AgentRuntime::execute_tool()` against document trees
   e. Emit `exploration-step-start` and `exploration-step-complete` events per tool call
   f. Append tool results to message history
7. If loop exhausts budget, force a final synthesis call without tools
8. Final answer is streamed token-by-token via `chat-token` events, then full response via `chat-response`
9. Trace data (tokens, cost, latency, steps) saved to SQLite

**State Management:**
- Frontend: Four Zustand stores, each managing a domain slice:
  - `useChatStore` (`src/stores/chat.ts`): conversations, messages, exploration steps, session totals, visited node IDs
  - `useDocumentsStore` (`src/stores/documents.ts`): document list, active document, active tree, multi-selection
  - `useSettingsStore` (`src/stores/settings.ts`): LLM providers, active provider
  - `useThemeStore` (`src/stores/theme.ts`): light/dark/system theme with localStorage persistence
- Backend: `Mutex<Database>` as Tauri managed state; `CancelFlag(Arc<AtomicBool>)` for query cancellation
- Cross-store communication: `useChatStore.setActiveConversation` calls `useDocumentsStore.getState().setActiveDocument` directly

## Key Abstractions

**Universal Document Tree (UDT):**
- Purpose: Uniform representation of any document type as a navigable tree
- Examples: `src-tauri/src/document/tree.rs` (DocumentTree, TreeNode, NodeType, Relation)
- Pattern: HashMap-based tree where nodes reference children by ID string. Root node established on creation. Nodes have type (Section, Paragraph, Table, CodeBlock, Image, etc.), content string, metadata map, and relations.

**DocumentParser Trait:**
- Purpose: Pluggable document parsing with one trait, many implementations
- Examples: `src-tauri/src/document/parser.rs` (MarkdownParser, PdfParser, DocxParser, CsvParser, XlsxParser, CodeParser, PlainTextParser)
- Pattern: `trait DocumentParser { fn parse(&self, file_path: &str) -> Result<DocumentTree, ParseError>; }` — dispatched by `get_parser_for_file()` based on file extension

**LLMProvider Trait:**
- Purpose: Uniform async interface for all LLM providers with tool-calling support
- Examples: `src-tauri/src/llm/provider.rs` (trait), `src-tauri/src/llm/anthropic.rs`, `src-tauri/src/llm/google.rs`, `src-tauri/src/llm/openai_compat.rs`
- Pattern: `#[async_trait] trait LLMProvider: Send + Sync` with `chat(messages, tools) -> Result<LLMResponse, LLMError>` and `capabilities()`. Provider factory in `commands.rs::create_provider()` maps name strings to concrete types.

**AgentTool Enum:**
- Purpose: Type-safe representation of available exploration tools
- Examples: `src-tauri/src/agent/tools.rs` (AgentTool enum, ToolInput, ToolOutput, ToolDefinition)
- Pattern: Enum with `from_name()` for LLM string-to-enum conversion. Tool definitions exported as JSON schemas in OpenAI and Gemini formats. Execution logic in `AgentRuntime::execute_tool()` matches on the enum.

**ExplorationContext:**
- Purpose: Track agent's exploration progress, visited nodes, budget, and relevance scoring
- Examples: `src-tauri/src/agent/context.rs`
- Pattern: Mutable state object recording explored nodes, visit counts, and summaries. Computes relevance scores post-exploration using visit frequency (60%) + content richness (40%) weighting.

**QueryIntent Classification:**
- Purpose: Classify user queries to adapt agent behavior (step budget, search strategy, nudge thresholds)
- Examples: `src-tauri/src/agent/query.rs`
- Pattern: Keyword-based classifier producing one of 6 intents (Summarize, Entity, Factual, Comparison, ListExtract, Specific). Each intent maps to minimum tool calls, exploration hints, and adaptive max_steps (6-15 range).

## Entry Points

**Rust Backend Entry:**
- Location: `src-tauri/src/main.rs` -> `src-tauri/src/lib.rs::run()`
- Triggers: Application launch
- Responsibilities: Initialize SQLite DB in platform app data directory, register Tauri plugins (opener, dialog), register managed state (Mutex<Database>, CancelFlag), register all IPC command handlers, start Tauri event loop

**Frontend Entry:**
- Location: `src/main.tsx` -> `src/App.tsx`
- Triggers: WebView load
- Responsibilities: Initialize theme from localStorage/system preference, render 3-panel layout (Sidebar, ChatPanel, PreviewPanel)

**IPC Commands (all in `src-tauri/src/commands.rs`):**
- Document: `list_documents`, `get_document`, `ingest_document`, `delete_document`
- Tree exploration: `get_tree_overview`, `expand_node`, `search_document`
- Agent chat: `chat_with_agent` (async, emits streaming events), `abort_query`
- Providers: `get_providers`, `save_provider`, `delete_provider`
- Settings: `get_setting`, `set_setting`
- Conversations: `list_conversations`, `save_conversation`, `get_conversation_messages`, `save_message`, `delete_conversation`
- Traces: `get_traces`, `get_steps`, `get_cost_summary`
- Bookmarks: `save_bookmark`, `get_bookmarks`, `delete_bookmark`
- File dialog: `open_file_dialog`

## Error Handling

**Strategy:** Result-based error propagation with domain-specific error enums

**Patterns:**
- All Tauri commands return `Result<T, String>`, converting internal errors via `.map_err(|e| e.to_string())`
- Backend modules define typed errors with `thiserror`: `DbError`, `ParseError`, `LLMError`, `RuntimeError`, `TreeError`
- Frontend: fire-and-forget pattern for persistence operations (`.catch(err => console.warn(...))`) with local state updated optimistically
- LLM errors during agent chat are emitted as `chat-error` events with the request correlation ID
- Agent loop gracefully handles unknown tool names by returning error message as tool result

## Cross-Cutting Concerns

**Logging:** `console.warn` on frontend for failed IPC calls. No structured logging framework on either side.

**Validation:** Minimal. Backend validates required tool parameters (`MissingParam` error). Frontend relies on UI constraints (button disable states, required fields). No input sanitization layer.

**Authentication:** Not applicable (local desktop app). LLM API keys stored in SQLite `providers.api_key_encrypted` column (note: column name says "encrypted" but keys are stored as plaintext).

**Cancellation:** `CancelFlag(Arc<AtomicBool>)` checked at the top of each agent loop iteration. Frontend calls `abort_query` to set the flag. Flag reset at start of each new `chat_with_agent` call.

**Cost Tracking:** Per-model pricing loaded from embedded `src-tauri/src/pricing.json` (compile-time). Cost estimated per LLM turn and distributed across tool calls. Stored in traces table.

**Streaming:** Agent responses are "simulated streaming" — full response received from LLM, then split into word-sized chunks (3 words at a time) and emitted via `chat-token` events with `tokio::task::yield_now()` between chunks.

---

*Architecture analysis: 2026-03-05*
