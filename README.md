<div align="left">

# TGG — Tree-Grounded Generation

**A desktop application for document question-answering without embeddings, vector databases, or chunking.**

Documents are parsed into structured trees. An LLM agent navigates them using explicit tools — expanding sections, searching content, following cross-references — rather than retrieving decontextualized fragments by cosine similarity.

[![Tauri v2](https://img.shields.io/badge/Tauri-v2-24C8D8?style=flat-square&logo=tauri&logoColor=white)](https://v2.tauri.app)
[![React 19](https://img.shields.io/badge/React-19-61DAFB?style=flat-square&logo=react&logoColor=black)](https://react.dev)
[![Rust](https://img.shields.io/badge/Rust-DEA584?style=flat-square&logo=rust&logoColor=black)](https://www.rust-lang.org)
[![SQLite](https://img.shields.io/badge/SQLite-003B57?style=flat-square&logo=sqlite&logoColor=white)](https://sqlite.org)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow?style=flat-square)](LICENSE)

</div>

---

## The Problem with Traditional RAG

Traditional RAG pipelines follow the same pattern: split documents into fixed-size chunks, embed each chunk into a vector, retrieve the top-K chunks by cosine similarity, and pass them to an LLM.

The fundamental problem is what gets destroyed in the process. Take a structured document:

```
§3  Authentication
 ├── §3.1  Token Types
 │    ├── Bearer   — expires in 24h  — read/write access
 │    ├── Refresh  — expires in 30d  — read-only
 │    └── API Key  — never expires   — configurable scope
 ├── §3.2  Authorization
 │    └── Role-based access control (references §3.1)
 └── §3.3  Refresh Flow
      └── Step-by-step token renewal (referenced from §3.1)
```

After chunking and embedding, the LLM receives something like this:

```
chunk_1:  "Bearer expires in 24h read/write access Refresh expires in 30d read-only..."
chunk_2:  "authentication tokens are issued upon login the refresh flow handles renewal..."
chunk_3:  "step 1 call the refresh endpoint with the current refresh token step 2..."
```

The heading hierarchy is gone. The table structure collapsed into a string. The cross-reference from §3.1 to §3.3 points nowhere. The LLM has no way to know these fragments are related, which section they belong to, or how to reason across them.

| What gets destroyed | Effect on the LLM |
| :--- | :--- |
| Heading hierarchy | Sections have no context — a paragraph floats with no parent |
| Table structure | Rows and columns become an undifferentiated string |
| Cross-references | "See §3.3" and "See Figure 5" are dead ends |
| Document boundaries | Chunks from unrelated documents appear in the same answer |
| Code structure | Functions are detached from their class and module |

---

## How TGG Works

TGG replaces the retrieval pipeline with an agent that navigates document structure directly. The core difference at each stage:

| Stage | Traditional RAG | TGG |
| :--- | :--- | :--- |
| **Ingest** | Split into fixed-size text chunks | Parse into a structured tree of typed nodes |
| **Index** | Embed every chunk, store in vector DB | Store the tree in local SQLite — no embedding |
| **On query** | Embed query, run cosine similarity search | Agent receives tools and a step budget |
| **Retrieval** | Return top-K chunks ranked by similarity | Agent calls `tree_overview`, `expand_node`, `search_content` as needed |
| **Context** | Decontextualized fragments injected into prompt | Agent reads exactly the nodes it chooses |
| **Answer** | LLM generates from shuffled fragments | LLM generates from real document structure |
| **Explainability** | No record of why a chunk was retrieved | Full trace of every tool call and node visited |

The agent decides its own exploration strategy. For a broad summarization query, it scans top-level sections. For a specific factual question, it searches first and then expands the relevant node. For a comparison query, it retrieves both sides explicitly. Every decision is recorded in a queryable trace.

### Exploration Tools

| Tool | Description |
| :--- | :--- |
| `tree_overview(doc_id)` | Returns the top-level structure of a document — equivalent to a table of contents |
| `expand_node(node_id)` | Reads the full content of a node and its immediate children |
| `search_content(query, scope?)` | Text search within the entire document or a specific subtree |
| `get_relations(node_id)` | Follows cross-reference edges to related nodes |
| `get_node_context(node_id)` | Returns the node's position in the document hierarchy |
| `compare_nodes(node_a, node_b)` | Retrieves two nodes side-by-side for direct comparison |
| `get_image(node_id)` | Retrieves image nodes; vision-capable models describe them inline |

### Adaptive Step Budget

Before execution, the query is classified by intent and scored for complexity. The step budget is allocated accordingly and capped between 6 and 15.

| Intent | Trigger words | Base budget |
| :--- | :--- | :---: |
| Summarize | "summarize", "overview", "explain" | 8 – 12 |
| Factual | "what is", "how does", "when did" | 4 – 6 |
| Comparison | "compare", "difference between", "vs" | 6 – 10 |
| List extraction | "list all", "what are the features" | 5 – 8 |
| Entity | "who", "which company", "author" | 3 – 5 |

---

## Universal Document Tree

Every supported format is parsed into the same tree schema:

```rust
Node {
    id:        String,
    node_type: NodeType,   // Section | Paragraph | Table | Row | CodeBlock | Image | ...
    content:   String,
    metadata:  Metadata,   // page, line range, word count, language, heading level, ...
    children:  Vec<Node>,
    relations: Vec<Relation>,
}
```

This uniform representation means the agent uses the same tools regardless of whether it is exploring a PDF research paper, a Word document, a Markdown file, or a source code repository.

| Format | Extensions | Parsed structure |
| :--- | :--- | :--- |
| PDF | `.pdf` | Pages → sections (heuristic heading detection) → paragraphs, images |
| Word | `.docx` | Heading styles → paragraphs → nested tables |
| Markdown | `.md` `.markdown` | Full GFM — headings, code blocks, lists, images, links |
| Spreadsheet | `.xlsx` `.xls` `.ods` | Sheets → rows → typed cells |
| CSV | `.csv` | Header row → data rows (up to 500 per file) |
| Source code | 23 languages | Modules → classes → functions → blocks |
| Plain text | `.txt` `.log` | Paragraphs split on blank lines |

Each node carries metadata the agent uses for smarter decisions: page numbers, word counts, line ranges, language identifiers, and heading levels.

---

## LLM Providers

TGG implements a unified `LLMProvider` trait in Rust. Streaming, tool calling, and cost tracking work identically across all providers.

| Provider | Models | Notes |
| :--- | :--- | :--- |
| **Anthropic** | Claude Sonnet 4.5 / 4.6, Opus 4.6, Haiku 4.5 | Native API |
| **OpenAI** | GPT-4o, GPT-4.1, o1, o3, o4-mini | |
| **Google AI Studio** | Gemini 2.5 Pro, 2.5 Flash, 2.0 Flash | Vision-capable |
| **Groq** | Llama 3.3-70B, Mixtral 8x7B, Gemma2-9B | Fast inference |
| **DeepSeek** | DeepSeek Chat, DeepSeek Reasoner | |
| **xAI** | Grok 3, Grok 3 Mini | |
| **Qwen** | Qwen Max, Qwen Plus, Qwen Turbo | |
| **OpenRouter** | 100+ models | Single API key for Claude, GPT, Llama, Mistral, and more |
| **AgentRouter** | GPT-5, DeepSeek, GLM | Automatic model routing |
| **Ollama** | Any locally-served model | No API key required |

Per-token pricing is embedded for 40+ models. Every query shows exact input and output token counts with cost.

---

## Observability

Every query produces a structured trace stored in local SQLite — no external service required.

```
Query     "What are the token expiry rules?"
Provider  claude-sonnet-4-6
Result    5 steps · 3,427 tokens · $0.0078 · 6.6s

  #   Tool                  Input tokens   Output tokens   Latency   Cost
  ─   ──────────────────    ────────────   ─────────────   ───────   ──────
  1   tree_overview                  320              92     0.8s   $0.0010
  2   search_content                 198              89     1.1s   $0.0008
  3   expand_node(§3.1)              601             290     1.4s   $0.0020
  4   expand_node(§3.3)              442             192     1.2s   $0.0010
  5   final answer                   543             660     2.1s   $0.0030
  ─   ──────────────────    ────────────   ─────────────   ───────   ──────
      Total                        2,104           1,323     6.6s   $0.0078
```

The trace view in the UI shows each step with expandable input and output. The document tree panel highlights visited nodes in real time as the agent explores.

---

## Features

**Agentic document exploration** — The agent receives tools and decides its own exploration path. Real-time thinking blocks in the chat panel show each tool call and result as it happens.

**Persistent conversations** — Chat history is stored in SQLite and can be resumed at any time. Up to 60 messages of prior context are loaded automatically.

**Multi-document queries** — Multiple documents can be selected simultaneously. The agent sees all document structures and decides which to explore.

**Interactive tree visualization** — The preview panel renders the document tree as an interactive SVG canvas with zoom, pan, and click-to-inspect. Nodes are color-coded by type and highlight as the agent visits them.

**Query cancellation** — An in-flight query can be cancelled at any step boundary via an `Arc<AtomicBool>` flag. No further tokens are spent after cancellation.

**Bookmarks** — Individual tree nodes can be bookmarked and retrieved across sessions.

**Local-first** — All data is stored in a SQLite database in the system app data directory. No telemetry, no cloud dependency. Documents do not leave the machine.

**Light and dark themes** — System preference is detected automatically with a manual override. Theming is implemented entirely through CSS custom properties.

---

## Architecture

```
┌──────────────────────────────────────────────────────┐
│  React 19 + TypeScript (Vite)                        │
│                                                      │
│  Sidebar          Chat Panel        Preview Panel    │
│  Documents        Messages          Tree / Graph     │
│  Conversations    Thinking blocks   Trace timeline   │
│  Settings         Streaming         Cost breakdown   │
│                   Drag and drop     Bookmarks        │
│                                                      │
│              Tauri IPC  (invoke + events)            │
└──────────────────────────┬───────────────────────────┘
                           │
┌──────────────────────────┴───────────────────────────┐
│  Rust + Tokio                                        │
│                                                      │
│  ┌───────────────────────────────────────────────┐   │
│  │  Agent Runtime                                │   │
│  │  Query preprocessing · Intent classification  │   │
│  │  Adaptive step budget · Tool execution loop   │   │
│  │  Cancellation (Arc<AtomicBool>)               │   │
│  └──────────────┬──────────────┬─────────────────┘   │
│                 │              │                      │
│  ┌──────────────┴──┐  ┌────────┴──┐  ┌────────────┐  │
│  │  Document       │  │  LLM      │  │  SQLite    │  │
│  │  Engine         │  │  Layer    │  │  (WAL)     │  │
│  │  7 parsers      │  │  10       │  │  documents │  │
│  │  Tree builder   │  │  providers│  │  messages  │  │
│  │  Metadata       │  │  40+      │  │  traces    │  │
│  │  Relations      │  │  models   │  │  bookmarks │  │
│  └─────────────────┘  └───────────┘  └────────────┘  │
└──────────────────────────────────────────────────────┘
```

### Stack

| Layer | Technology |
| :--- | :--- |
| Desktop shell | Tauri v2 |
| Frontend | React 19, TypeScript, Vite |
| Backend | Rust, Tokio async runtime |
| Database | SQLite via `rusqlite`, WAL mode |
| State management | Zustand v5 |
| Styling | CSS Modules, CSS custom properties |
| Markdown rendering | `react-markdown`, `remark-gfm`, `rehype-raw` |

---

## Getting Started

### Prerequisites

- [Node.js](https://nodejs.org/) 18 or later
- [Rust](https://rustup.rs/) stable toolchain
- Platform build tools — see [Tauri v2 prerequisites](https://v2.tauri.app/start/prerequisites/)

### Development

```bash
npm install
npm run tauri dev
```

### Production Build

```bash
npm run tauri build
```

Output is placed in `src-tauri/target/release/bundle/`. Produces `.msi` / `.exe` on Windows, `.dmg` on macOS, and `.deb` / `.AppImage` on Linux.

### First Steps

1. Open **Settings** and add an LLM provider — paste an API key and select a model.
2. Click **+** in the sidebar and upload a document.
3. Start a conversation. The agent will explore the document tree and return an answer grounded in its structure.

---

## Project Structure

```
src-tauri/src/
├── lib.rs                    # App setup, DB init, Tauri builder
├── commands.rs               # 26 Tauri IPC command handlers
├── document/
│   ├── tree.rs               # Universal Document Tree schema
│   └── parser.rs             # 7 format parsers
├── agent/
│   ├── runtime.rs            # Agent loop, tool execution, per-step tracking
│   ├── tools.rs              # 7 exploration tool definitions
│   ├── query.rs              # Query preprocessing, intent classification
│   └── context.rs            # Exploration context, visit tracking
├── llm/
│   ├── provider.rs           # Unified LLMProvider trait
│   ├── anthropic.rs          # Anthropic (Claude)
│   ├── openai_compat.rs      # OpenAI, DeepSeek, xAI, Qwen
│   ├── google.rs             # Google AI Studio (Gemini)
│   ├── groq.rs               # Groq
│   ├── openrouter.rs         # OpenRouter
│   ├── agentrouter.rs        # AgentRouter
│   ├── ollama.rs             # Ollama (local)
│   └── pricing.json          # Per-token rates for 40+ models
└── db/
    └── schema.rs             # SQLite schema, migrations, CRUD

src/
├── components/
│   ├── chat/                 # ChatPanel, ThinkingBlock
│   ├── preview/              # PreviewPanel, TreeView, CanvasView, TraceView
│   ├── sidebar/              # Documents, Chats, Settings tabs
│   └── settings/             # Provider configuration modal
├── stores/                   # Zustand: chat, documents, settings, theme
├── lib/                      # Tauri IPC wrappers, types, utilities
└── styles/                   # CSS custom properties, global styles
```

---

## License

MIT
