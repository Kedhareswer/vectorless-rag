# Vectorless RAG

A desktop application that replaces traditional vector/embedding-based RAG with **agentic document exploration**. Documents are parsed into structured trees; an LLM agent navigates them using tools (grep, expand, search, traverse) instead of semantic similarity search.

## Features

- **No embeddings, no vector DB** — agentic exploration using LLM tool calling
- **Universal Document Tree** — PDF, Markdown, code, images all parsed into a uniform tree structure
- **Multi-provider LLM support** — Groq, Google AI Studio, OpenRouter, AgentRouter, Ollama (local)
- **7 exploration tools** — tree_overview, expand_node, search_content, get_relations, get_image, compare_nodes, get_node_context
- **Full tracing & evaluation** — Langfuse-style local tracing with token/cost/latency tracking
- **Light & dark themes** — Claude Desktop-inspired warm cream/peach design
- **Local-first** — all data stored in SQLite, no cloud dependency

## Tech Stack

| Layer | Technology |
|-------|-----------|
| Desktop shell | Tauri v2 (Rust + WebView) |
| Frontend | React 19 + TypeScript + Vite |
| Backend | Rust (Tokio async runtime) |
| Database | SQLite via rusqlite |
| Styling | CSS Modules + custom properties |
| State | Zustand |

## Getting Started

### Prerequisites

- [Node.js](https://nodejs.org/) (v18+)
- [Rust](https://rustup.rs/) (stable)
- Platform build tools (see [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/))

### Development

```bash
# Install dependencies
npm install

# Run in development mode (launches Tauri + Vite)
npm run tauri dev
```

### Build for Windows

```bash
# Build the production installer (.msi and .exe)
npm run tauri build
```

The installer will be output to `src-tauri/target/release/bundle/`.

## LLM Providers

| Provider | Type | Default Model |
|----------|------|---------------|
| Ollama | Local | llama3.2 |
| Groq | Cloud | llama-3.3-70b-versatile |
| Google AI Studio | Cloud | gemini-2.0-flash |
| OpenRouter | Cloud | anthropic/claude-sonnet-4 |
| AgentRouter | Cloud | gpt-5 |

Configure providers in **Settings** (gear icon in sidebar). Each provider needs an API key (except Ollama) and a model name.

## Architecture

```
React Frontend <-> Tauri IPC (invoke) <-> Rust Backend
                                          |-- Document Engine (parsers, tree builder)
                                          |-- Agent Runtime (tools, context, planner)
                                          |-- LLM Provider Layer (5 providers)
                                          +-- SQLite DB (trees, traces, evals)
```

## License

MIT
