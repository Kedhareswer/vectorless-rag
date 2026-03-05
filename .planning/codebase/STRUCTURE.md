# Codebase Structure

**Analysis Date:** 2026-03-05

## Directory Layout

```
vectorless-rag/
├── src/                        # React frontend source
│   ├── main.tsx                # React DOM entry point
│   ├── App.tsx                 # Root component (3-panel layout)
│   ├── App.module.css          # Root layout styles
│   ├── vite-env.d.ts           # Vite type declarations
│   ├── components/             # UI components by feature area
│   │   ├── chat/               # Chat panel components
│   │   ├── common/             # Shared UI primitives
│   │   ├── preview/            # Right panel (tree, canvas, trace views)
│   │   ├── settings/           # Settings modal
│   │   └── sidebar/            # Left sidebar
│   ├── stores/                 # Zustand state stores
│   ├── styles/                 # Global theme CSS
│   └── lib/                    # Utilities and Tauri IPC wrappers
├── src-tauri/                  # Rust backend (Tauri v2)
│   ├── Cargo.toml              # Rust dependencies
│   ├── tauri.conf.json         # Tauri configuration
│   ├── capabilities/           # Tauri v2 capability permissions
│   ├── icons/                  # App icons
│   ├── src/
│   │   ├── main.rs             # Windows entry point (calls lib::run)
│   │   ├── lib.rs              # App setup: DB init, state registration, command handler registration
│   │   ├── commands.rs         # All Tauri IPC command handlers + agent chat loop
│   │   ├── pricing.json        # Embedded per-model token pricing table
│   │   ├── agent/              # Agent runtime, tools, context, query preprocessing
│   │   ├── db/                 # SQLite schema, CRUD, traces
│   │   ├── document/           # Parsers, tree data structures, image extraction
│   │   └── llm/                # LLM provider trait + implementations
│   └── gen/                    # Tauri-generated schemas (auto-generated)
├── public/                     # Static assets served by Vite
├── dist/                       # Vite build output
├── docs/                       # Documentation and plans
├── index.html                  # HTML shell for Vite/WebView
├── package.json                # Node dependencies and scripts
├── vite.config.ts              # Vite configuration
├── tsconfig.json               # TypeScript config (frontend)
└── tsconfig.node.json          # TypeScript config (Vite/Node)
```

## Directory Purposes

**`src/components/chat/`:**
- Purpose: Chat interface — message input, message display, agent thinking visualization
- Contains: `ChatPanel.tsx` (main chat view), `ChatPanel.module.css`, `ThinkingBlock.tsx` (animated agent thinking indicator), `ThinkingBlock.module.css`
- Key files: `ChatPanel.tsx` is the primary user interaction surface

**`src/components/preview/`:**
- Purpose: Right panel with document visualization and exploration trace
- Contains: `PreviewPanel.tsx` (panel container with collapsible sections), `TreeView.tsx` (document tree browser), `CanvasView.tsx` (SVG-based zoom/pan graph), `TraceView.tsx` (exploration step timeline)
- Key files: `PreviewPanel.tsx` orchestrates stacked sections; `CanvasView.tsx` renders node graph with SVG

**`src/components/sidebar/`:**
- Purpose: Left sidebar for navigation — document list, conversation list, settings access
- Contains: `Sidebar.tsx`, `Sidebar.module.css`

**`src/components/settings/`:**
- Purpose: Modal for configuring LLM providers
- Contains: `SettingsModal.tsx`, `SettingsModal.module.css`

**`src/components/common/`:**
- Purpose: Reusable UI primitives
- Contains: `IconButton.tsx`, `IconButton.module.css`

**`src/stores/`:**
- Purpose: Zustand state management — one store per domain
- Contains:
  - `chat.ts` — conversations, messages, exploration steps, session totals, visited nodes
  - `documents.ts` — document list, active/selected documents, tree loading
  - `settings.ts` — LLM provider configs, active provider selection
  - `theme.ts` — light/dark/system theme with localStorage + media query listener

**`src/lib/`:**
- Purpose: Tauri IPC wrapper functions and shared TypeScript types
- Contains: `tauri.ts` — typed `invoke()` wrappers for every backend command, TypeScript interfaces matching Rust structs (snake_case)

**`src/styles/`:**
- Purpose: Global theme CSS custom properties
- Contains: `theme.css` — CSS custom properties for light/dark themes, global font and color tokens

**`src-tauri/src/agent/`:**
- Purpose: Document exploration agent logic
- Contains:
  - `mod.rs` — module re-exports
  - `runtime.rs` — `AgentRuntime` struct, `execute_tool()` dispatcher, `build_system_prompt()`, helper functions (`is_descendant_of`, `build_parent_chain`)
  - `tools.rs` — `AgentTool` enum, `ToolInput`/`ToolOutput`/`ToolDefinition` structs, JSON schema definitions in OpenAI and Gemini formats
  - `context.rs` — `ExplorationContext` tracking explored nodes, visit counts, budget, relevance scoring
  - `query.rs` — `preprocess_query()` with intent classification, search term extraction, exploration hint generation, adaptive step budgets

**`src-tauri/src/db/`:**
- Purpose: SQLite database layer
- Contains:
  - `mod.rs` — module re-exports
  - `schema.rs` — `Database` struct, `initialize()` with CREATE TABLE statements, migrations, all CRUD methods for documents, conversations, messages, providers, settings, bookmarks, cost summaries
  - `traces.rs` — `TraceRecord`, `StepRecord`, `EvalRecord` structs and their CRUD methods

**`src-tauri/src/document/`:**
- Purpose: File parsing and Universal Document Tree (UDT) data structures
- Contains:
  - `mod.rs` — module re-exports
  - `tree.rs` — `DocumentTree`, `TreeNode`, `TreeNodeSummary`, `NodeType` enum (Root, Section, Paragraph, Heading, Table, TableRow, TableCell, Image, CodeBlock, ListItem, Link, Unknown), `DocType` enum, `Relation`/`RelationType`
  - `parser.rs` — `DocumentParser` trait, implementations: `MarkdownParser`, `PlainTextParser`, `CodeParser`, `PdfParser`, `DocxParser`, `CsvParser`, `XlsxParser`, dispatcher `get_parser_for_file()`
  - `image.rs` — `ImageNode` struct, `extract_images_from_path()` helper

**`src-tauri/src/llm/`:**
- Purpose: LLM provider abstraction and implementations
- Contains:
  - `mod.rs` — module re-exports
  - `provider.rs` — `LLMProvider` async trait, `Message`, `Tool`, `ToolCall`, `LLMResponse`, `ProviderConfig`, `ProviderCapabilities`, `LLMError`
  - `anthropic.rs` — Anthropic Claude provider (native Messages API, converts to/from OpenAI tool format internally)
  - `google.rs` — Google AI Studio / Gemini provider
  - `groq.rs` — Groq provider (OpenAI-compatible)
  - `openrouter.rs` — OpenRouter provider
  - `agentrouter.rs` — AgentRouter provider
  - `ollama.rs` — Ollama local provider
  - `openai_compat.rs` — Generic OpenAI-compatible provider (used for OpenAI, DeepSeek, xAI, Qwen via different base URLs)

## Key File Locations

**Entry Points:**
- `src/main.tsx`: React DOM render entry
- `src/App.tsx`: Root component composing the 3-panel layout
- `src-tauri/src/main.rs`: Windows entry, calls `run()`
- `src-tauri/src/lib.rs`: App initialization (DB, state, commands)
- `index.html`: HTML shell for Vite dev server and WebView

**Configuration:**
- `src-tauri/Cargo.toml`: Rust dependencies
- `src-tauri/tauri.conf.json`: Tauri app config (window size, title, permissions)
- `package.json`: Node dependencies and scripts (`dev`, `build`, `tauri`)
- `vite.config.ts`: Vite build config
- `tsconfig.json`: TypeScript compiler options
- `src-tauri/src/pricing.json`: Embedded per-model token pricing (compile-time `include_str!`)

**Core Logic:**
- `src-tauri/src/commands.rs`: All IPC handlers including the 450-line `chat_with_agent` agent loop
- `src-tauri/src/agent/runtime.rs`: Tool execution engine (287 lines)
- `src-tauri/src/agent/query.rs`: Query intent classification and preprocessing (213 lines)
- `src-tauri/src/document/parser.rs`: All document parsers (679 lines)
- `src-tauri/src/document/tree.rs`: Core UDT data structures (186 lines)
- `src-tauri/src/llm/provider.rs`: LLM trait and shared types (125 lines)
- `src/lib/tauri.ts`: Frontend IPC wrappers and TypeScript interfaces (158 lines)

**State Management:**
- `src/stores/chat.ts`: Chat state (317 lines, most complex store)
- `src/stores/documents.ts`: Document state (172 lines)
- `src/stores/settings.ts`: Provider/settings state (167 lines)
- `src/stores/theme.ts`: Theme state (58 lines)

**Database:**
- `src-tauri/src/db/schema.rs`: Schema definition + all CRUD (491 lines)
- `src-tauri/src/db/traces.rs`: Trace/step/eval storage (166 lines)

## Naming Conventions

**Files:**
- Rust: `snake_case.rs` (e.g., `openai_compat.rs`, `tree.rs`)
- TypeScript components: `PascalCase.tsx` (e.g., `ChatPanel.tsx`, `PreviewPanel.tsx`)
- TypeScript stores/utils: `camelCase.ts` (e.g., `chat.ts`, `tauri.ts`)
- CSS modules: `ComponentName.module.css` co-located with their component
- Rust modules: `mod.rs` in each directory for re-exports

**Directories:**
- Rust: `snake_case/` (e.g., `agent/`, `document/`, `llm/`, `db/`)
- TypeScript: `camelCase/` (e.g., `components/chat/`, `stores/`)

## Where to Add New Code

**New Document Parser:**
- Create parser struct implementing `DocumentParser` trait in `src-tauri/src/document/parser.rs`
- Add file extension mapping in `get_parser_for_file()` at the bottom of the same file
- Add `DocType` variant if needed in `src-tauri/src/document/tree.rs`
- Add file extension to the dialog filter in `commands.rs::open_file_dialog`
- Re-export from `src-tauri/src/document/mod.rs` if the type is public

**New LLM Provider:**
- Create `src-tauri/src/llm/newprovider.rs` implementing `LLMProvider` trait
- Add `pub mod newprovider;` and `pub use` in `src-tauri/src/llm/mod.rs`
- Add match arm in `commands.rs::create_provider()` factory function
- Add provider name to the settings UI dropdown in `src/components/settings/SettingsModal.tsx`

**New Agent Tool:**
- Add variant to `AgentTool` enum in `src-tauri/src/agent/tools.rs`
- Add `from_name()` match arm in the same file
- Add `ToolDefinition` entry in `get_tool_definitions()`
- Add execution logic as a new match arm in `AgentRuntime::execute_tool()` in `src-tauri/src/agent/runtime.rs`
- Update the system prompt tool list in `build_system_prompt()` in the same file

**New Tauri IPC Command:**
- Add `#[tauri::command]` function in `src-tauri/src/commands.rs`
- Register it in the `invoke_handler` array in `src-tauri/src/lib.rs`
- Add typed invoke wrapper in `src/lib/tauri.ts`

**New React Component:**
- Create `src/components/{feature}/ComponentName.tsx` with co-located `ComponentName.module.css`
- Use CSS custom properties from `src/styles/theme.css` for theming
- Import icons from `lucide-react`

**New Zustand Store:**
- Create `src/stores/newstore.ts` following the pattern: `create<StateType>((set, get) => ({ ... }))`
- Export named hook: `export const useNewStore = create<...>(...)`

**New Database Table:**
- Add `CREATE TABLE IF NOT EXISTS` in `Database::initialize()` in `src-tauri/src/db/schema.rs`
- Add migration in `run_migrations()` for existing databases
- Add CRUD methods on the `Database` impl
- Add record struct with `#[derive(Serialize, Clone, Debug)]`

**New Frontend Route/View:**
- This app does not use a router. All views are panel-based. Add new panels or modal components and toggle visibility via Zustand state.

## Special Directories

**`src-tauri/gen/`:**
- Purpose: Auto-generated Tauri v2 schemas and capability definitions
- Generated: Yes (by `tauri build` / `tauri dev`)
- Committed: Yes

**`dist/`:**
- Purpose: Vite production build output
- Generated: Yes (by `vite build`)
- Committed: Yes (currently in repo)

**`src-tauri/target/`:**
- Purpose: Rust build artifacts
- Generated: Yes (by `cargo build`)
- Committed: No (in .gitignore)

**`node_modules/`:**
- Purpose: Node.js dependencies
- Generated: Yes (by `npm install` / `pnpm install`)
- Committed: No (in .gitignore)

**`src-tauri/capabilities/`:**
- Purpose: Tauri v2 permission capability definitions (JSON)
- Generated: Partially (base generated, may be manually edited)
- Committed: Yes

**`src-tauri/icons/`:**
- Purpose: Application icons in various sizes for different platforms
- Generated: Yes (by `tauri icon`)
- Committed: Yes

---

*Structure analysis: 2026-03-05*
