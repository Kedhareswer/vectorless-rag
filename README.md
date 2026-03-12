# TGG — Tree-Grounded Generation

**A desktop application for document question-answering without embeddings, vector databases, or chunking.**

Documents are parsed into structured trees. A deterministic fetcher navigates them based on enriched search terms — rather than retrieving decontextualized fragments by cosine similarity — then a single streaming LLM call generates the grounded answer.

[![Tauri v2](https://img.shields.io/badge/Tauri-v2-24C8D8?style=flat-square&logo=tauri&logoColor=white)](https://v2.tauri.app)
[![React 19](https://img.shields.io/badge/React-19-61DAFB?style=flat-square&logo=react&logoColor=black)](https://react.dev)
[![Rust](https://img.shields.io/badge/Rust-DEA584?style=flat-square&logo=rust&logoColor=black)](https://www.rust-lang.org)
[![SQLite](https://img.shields.io/badge/SQLite-003B57?style=flat-square&logo=sqlite&logoColor=white)](https://sqlite.org)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow?style=flat-square)](LICENSE)

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

The heading hierarchy is gone. The table structure collapsed into a string. The cross-reference from §3.1 to §3.3 points nowhere.

| What gets destroyed | Effect on the LLM |
| :--- | :--- |
| Heading hierarchy | Sections have no context — a paragraph floats with no parent |
| Table structure | Rows and columns become an undifferentiated string |
| Cross-references | "See §3.3" and "See Figure 5" are dead ends |
| Document boundaries | Chunks from unrelated documents appear in the same answer |
| Code structure | Functions are detached from their class and module |

---

## How TGG Works

TGG replaces the retrieval pipeline with a deterministic fetch step that navigates document structure directly, guided by LLM-powered query enrichment.

| Stage | Traditional RAG | TGG |
| :--- | :--- | :--- |
| **Ingest** | Split into fixed-size text chunks | Parse into a structured tree of typed nodes |
| **Index** | Embed every chunk, store in vector DB | Store the tree in local SQLite — no embedding |
| **On query** | Embed query, run cosine similarity search | Enrich query with 3 LLM calls (rewrite, HyDE, StepBack) |
| **Retrieval** | Return top-K chunks ranked by similarity | Deterministic fetch: code decides what to read based on intent + enriched terms |
| **Context** | Decontextualized fragments injected into prompt | Exactly the right nodes, structured by document hierarchy |
| **Answer** | LLM generates from shuffled fragments | One streaming LLM call — reads and answers |
| **Explainability** | No record of why a chunk was retrieved | Full trace of every fetch step with tokens and latency |

### Query Pipeline (9 phases)

```
Phase 1  Heuristic preprocess — classify intent, extract search terms
Phase 3  Query enrichment (3 LLM calls)
         • Query Rewrite  — search-optimized reformulation
         • HyDE           — hypothetical answer passage, extract terms
         • StepBack       — broader question, extract background terms
Phase 4  Deterministic fetch — code reads the right tree nodes
Phase 5  Cross-doc relation discovery (if multiple docs)
Phase 8  ONE streaming LLM call — no tools, reads and answers
Phase 9  Save full trace
```

Every step is shown in real time as a thinking block in the chat panel.

---

## Universal Document Tree

Every supported format is parsed into the same tree schema:

```rust
Node {
    id:        String,
    node_type: NodeType,   // Section | Paragraph | Table | Row | CodeBlock | Image | ...
    content:   String,
    metadata:  Metadata,   // page, word count, entities, topics, heading level, ...
    children:  Vec<Node>,
    relations: Vec<Relation>,
}
```

| Format | Extensions | Parsed structure |
| :--- | :--- | :--- |
| PDF | `.pdf` | Pages → sections (heuristic heading detection) → paragraphs |
| Word | `.docx` | Heading styles → paragraphs → nested tables |
| Markdown | `.md` `.markdown` | Full GFM — headings, code blocks, lists, images, links |
| Spreadsheet | `.xlsx` `.xls` `.ods` | Sheets → rows → typed cells |
| CSV | `.csv` | Header row → data rows (up to 500 per file) |
| Source code | 23 languages | Modules → classes → functions → blocks |
| Plain text | `.txt` `.log` | Paragraphs split on blank lines |

---

## LLM Providers

TGG implements a unified `LLMProvider` trait in Rust. Streaming, retry, and cost tracking work identically across all providers.

| Provider | Models | Notes |
| :--- | :--- | :--- |
| **Anthropic** | Claude 4.6 Sonnet/Opus, 4.5 Haiku | Native SSE streaming |
| **OpenAI** | GPT-4o, GPT-4.1, o1, o3, o4-mini | |
| **Google AI Studio** | Gemini 2.5 Pro, 2.5 Flash | Vision-capable |
| **Groq** | Llama 3.3-70B, Mixtral 8x7B | Fast inference |
| **DeepSeek** | DeepSeek Chat, DeepSeek Reasoner | |
| **xAI** | Grok 3, Grok 3 Mini | |
| **Qwen** | Qwen Max, Qwen Plus, Qwen Turbo | |
| **OpenRouter** | 100+ models | Single key for Claude, GPT, Llama, Mistral, and more |
| **AgentRouter** | GPT-5, DeepSeek, GLM | Automatic model routing |
| **Ollama** | Any locally-served model | Download works; inference is a stub |

Per-token pricing is embedded for 40+ models. Every query shows exact input/output token counts with cost.

---

## Observability

Every query produces a structured trace stored in local SQLite — no external service required.

```
Query     "What are the token expiry rules?"
Provider  claude-sonnet-4-6
Result    6 steps · 2,398 tokens · $0.0078 · 6.6s

  #   Step                  Input tokens   Output tokens   Latency   Cost
  ─   ──────────────────    ────────────   ─────────────   ───────   ──────
  1   query_rewrite                  220              45     0.7s   $0.0004
  2   hyde                           198              89     1.1s   $0.0008
  3   stepback                       180              62     0.9s   $0.0006
  4   search (3 terms)               —                —      0.0s   —
  5   expand (§3.1, §3.3)            —                —      0.0s   —
  6   llm_call                     1,800             660     3.9s   $0.0060
  ─   ──────────────────    ────────────   ─────────────   ───────   ──────
      Total                        2,398           1,029     6.6s   $0.0078
```

---

## Features

**Deterministic pipeline** — Query enrichment followed by code-driven content fetch. Real-time thinking blocks in the chat panel show each step as it happens.

**Persistent conversations** — Chat history stored in SQLite, resumed automatically. Up to 8,000 tokens of prior context loaded per query.

**Multi-document queries** — Multiple documents per conversation. Cross-document relations (shared entities, topic overlap) discovered automatically and injected into context.

**Interactive tree visualization** — The preview panel renders the document tree as an interactive SVG canvas with zoom, pan, and click-to-inspect. Nodes are color-coded by type.

**Query cancellation** — An in-flight query can be cancelled via an `Arc<AtomicBool>` flag checked at three points in the pipeline.

**Local-first** — All data in SQLite in the system app data directory. No telemetry, no cloud dependency. Documents do not leave the machine.

**Light and dark themes** — System preference detected automatically with a manual override via CSS custom properties.

---

## Architecture

```
React 19 + TypeScript (Vite)

Sidebar          Chat Panel        Preview Panel
Documents        Messages          Tree / Graph
Conversations    Thinking blocks   Trace timeline
Settings         Streaming         Cost breakdown

         Tauri IPC  (invoke + channels)
                    |
Rust + Tokio

  Query Pipeline (chat_handler.rs)
  Query preprocessing · 3 enrichment calls
  Deterministic fetch · Single streaming call
  Cancellation (Arc<AtomicBool>)
        |                    |
  Document Engine      LLM Layer          SQLite (WAL)
  7 parsers            10 providers       documents
  Tree builder         40+ models         messages
  Metadata                                traces
  Relations
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
| Markdown rendering | `react-markdown`, `remark-gfm`, `rehype-sanitize` |

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
2. Click **+** in the sidebar to start a new conversation and upload a document.
3. Ask a question. The pipeline will enrich the query, fetch the relevant tree nodes, and return a grounded answer.

---

## License

MIT
