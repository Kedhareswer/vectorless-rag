# TGG — Tree-Grounded Generation

A desktop application that replaces traditional vector/embedding-based RAG with **agentic document exploration**. Documents are parsed into structured trees; an LLM agent navigates them using tools (grep, expand, search, traverse) instead of semantic similarity search.

> **TGG** stands for **Tree-Grounded Generation** — answers grounded in the actual tree structure of your documents, not fuzzy vector similarity.

## Features

- **No embeddings, no vector DB** — agentic tree exploration using LLM tool calling
- **Universal Document Tree** — PDF, Markdown, code, images all parsed into a uniform tree structure
- **Multi-provider LLM support** — Groq, Google AI Studio, OpenRouter, AgentRouter, Anthropic, OpenAI, DeepSeek, xAI/Grok, Qwen, Ollama (local)
- **7 exploration tools** — tree_overview, expand_node, search_content, get_relations, get_image, compare_nodes, get_node_context
- **Full tracing & evaluation** — Langfuse-style local tracing with token/cost/latency tracking
- **Cancel mid-query** — stop any in-flight agent query instantly
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
| Anthropic | Cloud | claude-sonnet-4-5 |
| OpenAI | Cloud | gpt-4o |
| DeepSeek | Cloud | deepseek-chat |
| xAI / Grok | Cloud | grok-3 |
| Qwen | Cloud | qwen-max |

Configure providers in **Settings** (gear icon in sidebar). Each provider needs an API key (except Ollama) and a model name.

## Architecture

```
React Frontend <-> Tauri IPC (invoke) <-> Rust Backend
                                          |-- Document Engine (parsers, tree builder)
                                          |-- Agent Runtime (tools, context, planner)
                                          |-- LLM Provider Layer (10 providers)
                                          +-- SQLite DB (trees, traces, evals)
```

### How TGG differs from RAG

| Traditional RAG | TGG |
|-----------------|-----|
| Embed query → cosine similarity | Agent plans which tree nodes to explore |
| Returns decontextualized chunks | Returns answers grounded in document structure |
| Loses heading hierarchy | Preserves parent→child relationships |
| Flat retrieval | Deep, targeted traversal |
| No cross-referencing | Agent can follow relations between nodes |

## License

MIT
