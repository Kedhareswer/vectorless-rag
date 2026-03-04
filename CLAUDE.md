# TGG — Tree-Grounded Generation

## Project Overview
A desktop application (Tauri v2 + React) that replaces traditional vector/embedding-based RAG with **agentic document exploration**. Documents are parsed into structured trees; an LLM agent navigates them using tools (grep, expand, search, traverse) instead of semantic similarity search.

**TGG = Tree-Grounded Generation** — answers are grounded in the actual tree structure of documents, not cosine similarity over embeddings.

## Tech Stack
- **Desktop shell**: Tauri v2 (Rust backend + WebView frontend)
- **Frontend**: React 19 + TypeScript + Vite
- **Backend**: Rust (Tokio async runtime)
- **Database**: SQLite via `rusqlite` (document trees, traces, evals, settings)
- **Styling**: CSS custom properties + CSS modules, Inter font, JetBrains Mono for code
- **State management**: Zustand

## Architecture
```
React Frontend ←→ Tauri IPC (invoke) ←→ Rust Backend
                                          ├── Document Engine (parsers, tree builder, OCR)
                                          ├── Agent Runtime (tools, context, planner)
                                          ├── LLM Provider Layer (10 providers)
                                          └── SQLite DB (trees, traces, evals)
```

## LLM Providers (all with latest models)
- **Groq** — Llama, Mixtral, Gemma (fast inference)
- **Google AI Studio** — Gemini 2.5 Pro/Flash (vision capable)
- **OpenRouter** — Access to Claude, GPT, Llama, Mistral, DeepSeek, etc.
- **AgentRouter** — Smart LLM routing (GPT-5, DeepSeek, GLM), OpenAI-compatible
- **Anthropic** — Claude 3.5/3.7/4 Sonnet, Opus, Haiku (direct API)
- **OpenAI** — GPT-4o, GPT-4.1, o1, o3 mini
- **DeepSeek** — DeepSeek Chat, DeepSeek Reasoner
- **xAI / Grok** — Grok 3, Grok 3 Mini
- **Qwen** — Qwen Max, Qwen Plus, Qwen Turbo
- **Ollama** — Local models (Llama, Mistral, LLaVA for vision)
- Unified `LLMProvider` trait in Rust, each provider implements it

## Core Concepts

### Universal Document Tree (UDT)
Every document (PDF, Word, Markdown, code, images) is parsed into a uniform tree:
```rust
Node { id, node_type, content, metadata, children, relations }
```
- Type-specific parsers output into this same schema
- Images get placeholder nodes, described lazily by vision LLMs
- Relations are cross-references, links, dependencies between nodes

### Agent Exploration Tools
The LLM agent gets these tools to navigate document trees:
- `tree_overview(doc_id)` — see top-level structure
- `expand_node(node_id)` — dive deeper into a branch
- `search_content(query, scope)` — grep-like text search within a subtree
- `get_relations(node_id)` — follow edges to related nodes
- `get_image(node_id)` — retrieve and describe an image node
- `compare_nodes(node_a, node_b)` — cross-reference two parts

### Query Cancellation
Queries can be cancelled mid-flight via `abort_query` Tauri command. An `AtomicBool` cancel flag is managed as Tauri state, checked at the top of each agent loop turn.

### Tracing & Evaluation
Full Langfuse-style local tracing:
- Token usage per step, latency (LLM turn time distributed across tool calls), cost tracking
- Exploration path visualization in the Preview Panel
- Answer quality scoring
- All stored in local SQLite

## UI Design

### Layout: 3-panel adaptive
```
┌──────────┬─────────────────────┬──────────────────┐
│ Sidebar  │    Main Chat        │  Preview Panel   │
│  200px   │    flexible         │   360px          │
│          │  + animated agent   │  Stacked sections│
│  Docs    │    thinking blocks  │  • Doc Structure │
│  Chats   │                     │    (tree/graph)  │
│  Config  │                     │  • Trace/Eval    │
└──────────┴─────────────────────┴──────────────────┘
```
- Right panel has VS Code-style collapsible stacked sections
- Right panel collapses when not needed
- Sidebar collapses to 48px icon-only on small screens

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
- Frontend state in Zustand stores, one store per domain (chat, documents, settings, traces)

## File Structure
```
src-tauri/src/
  main.rs, lib.rs
  document/   — parsers, tree builder, image extraction
  agent/      — runtime, tools, context management
  llm/        — provider trait + implementations (one file per provider)
  db/         — SQLite schema, queries, migrations
  commands.rs — Tauri IPC command handlers (incl. CancelFlag state)

src/
  components/ — sidebar/, chat/, preview/, tree/, canvas/, trace/, common/
  hooks/      — custom React hooks
  stores/     — Zustand stores
  styles/     — theme.ts, global.css
  lib/        — utilities, types, Tauri invoke wrappers
```

## Key Decisions
- No vector database, no embeddings — agentic tree exploration only
- Local-first: all data in SQLite + filesystem
- Background document processing, lazy vision analysis
- Streaming LLM responses via Tauri events
- React 19 for frontend (ecosystem, familiarity)
- Both light + dark themes from day one
- Cancel flag via `Arc<AtomicBool>` Tauri state (no native invoke cancellation in Tauri)
- Latency measured per LLM turn (not per local tool exec) and distributed across tool calls

# currentDate
Today's date is 2026-03-05.

      IMPORTANT: this context may or may not be relevant to your tasks. You should not respond to this context unless it is highly relevant to your task.
