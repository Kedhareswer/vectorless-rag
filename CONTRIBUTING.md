# Contributing to TGG

Thank you for your interest in contributing. This document covers everything you need to know before opening a pull request — the project structure, development setup, coding conventions, and what kinds of contributions are most useful.

---

## Before You Start

Read these first. They are short and will save you time:

- [README.md](README.md) — understand what TGG is and how it works
- [Tauri v2 Architecture](https://v2.tauri.app/concept/) — how Rust backend and WebView frontend communicate via IPC
- [Tauri v2 Commands](https://v2.tauri.app/develop/calling-rust/) — how `#[tauri::command]` works and how the frontend calls Rust
- [Tokio async runtime](https://tokio.rs/tokio/tutorial) — TGG's backend is fully async; understand `async/await` in Rust before touching agent or LLM code
- [Zustand](https://zustand.docs.pmnd.rs/) — frontend state is managed with Zustand v5; understand stores and subscriptions

If you are new to Tauri, working through the [Tauri v2 quickstart](https://v2.tauri.app/start/) before contributing is strongly recommended.

---

## Development Setup

### Prerequisites

| Requirement | Version | Link |
| :--- | :--- | :--- |
| Node.js | 18 or later | https://nodejs.org |
| Rust | stable toolchain | https://rustup.rs |
| Platform build tools | — | https://v2.tauri.app/start/prerequisites/ |

### Running locally

```bash
git clone <repo-url>
cd vectorless-rag

npm install
npm run tauri dev
```

This starts the Vite dev server and opens a Tauri window. The Rust backend recompiles on changes. The frontend has hot module reloading.

### Building for production

```bash
npm run tauri build
```

Output is in `src-tauri/target/release/bundle/`.

---

## Project Structure

```
src-tauri/src/
├── lib.rs              # App setup, DB init, Tauri state, command registration
├── commands.rs         # All 26 Tauri IPC command handlers
├── document/
│   ├── tree.rs         # DocumentTree, TreeNode — the Universal Document Tree schema
│   └── parser.rs       # 7 format parsers (PDF, DOCX, MD, XLSX, CSV, Code, Text)
├── agent/
│   ├── runtime.rs      # Agent loop — reads tools, tracks steps, manages cancellation
│   ├── tools.rs        # 7 exploration tool definitions and execution
│   ├── query.rs        # Query preprocessing, intent classification, complexity scoring
│   └── context.rs      # Exploration context, visited nodes, relevance scoring
├── llm/
│   ├── provider.rs     # Unified LLMProvider trait — all providers implement this
│   ├── anthropic.rs    # Anthropic (Claude) — native API format
│   ├── openai_compat.rs # OpenAI, DeepSeek, xAI, Qwen — shared OpenAI-compatible layer
│   ├── google.rs       # Google AI Studio (Gemini)
│   ├── groq.rs         # Groq
│   ├── openrouter.rs   # OpenRouter
│   ├── agentrouter.rs  # AgentRouter
│   ├── ollama.rs       # Ollama (local models)
│   └── pricing.json    # Per-token pricing rates for 40+ models
└── db/
    └── schema.rs       # SQLite schema, migrations, all CRUD operations

src/
├── components/
│   ├── chat/           # ChatPanel, ThinkingBlock
│   ├── preview/        # PreviewPanel, TreeView, CanvasView, TraceView
│   ├── sidebar/        # Sidebar — Documents, Chats, Settings tabs
│   └── settings/       # Provider configuration modal
├── stores/             # Zustand stores: chat, documents, settings, theme
├── lib/                # Tauri IPC wrappers, shared TypeScript types, utilities
└── styles/             # CSS custom properties, global styles, theme tokens
```

---

## Coding Conventions

### Rust

- Snake_case for all identifiers. PascalCase for types and enums.
- One module per file. Keep `mod.rs` files minimal — just re-exports.
- Use `thiserror` for error types. Do not use `.unwrap()` or `.expect()` in production paths.
- All Tauri commands must be `async` and return `Result<T, String>`.
- Use `serde` for all serialization. Derive `Serialize`, `Deserialize` on any type crossing the IPC boundary.
- New LLM providers must implement the `LLMProvider` trait in `llm/provider.rs` and add their pricing to `pricing.json`.

```rust
// Correct Tauri command signature
#[tauri::command]
pub async fn my_command(
    arg: String,
    db: State<'_, DbState>,
) -> Result<MyReturnType, String> {
    // ...
}
```

### TypeScript / React

- camelCase for variables and functions. PascalCase for components and types.
- One component per file. Keep component files focused — extract sub-components when a file exceeds ~200 lines.
- All Tauri IPC calls go through wrapper functions in `src/lib/`. Do not call `invoke()` directly from components.
- State belongs in Zustand stores (`src/stores/`). Do not use React context for app-wide state.
- CSS is scoped with CSS Modules. Theme values come from CSS custom properties defined in `src/styles/`. Do not hardcode colors or spacing.

### Git

- Commit messages follow [Conventional Commits](https://www.conventionalcommits.org/): `feat:`, `fix:`, `docs:`, `refactor:`, `chore:`.
- One logical change per commit. Do not bundle unrelated changes.
- Branch names: `feat/<short-description>`, `fix/<short-description>`, `docs/<short-description>`.

---

## How to Contribute

### 1. Find or open an issue

Check the issue tracker before starting work. For significant changes, open an issue first to discuss the approach. This avoids wasted effort if the direction does not fit the project.

### 2. Fork and branch

```bash
git checkout -b feat/your-feature-name
```

### 3. Make your changes

Follow the conventions above. Keep changes focused on one thing.

### 4. Test your changes

```bash
# Type-check the frontend
npm run type-check

# Build to catch Rust compilation errors
npm run tauri build -- --debug
```

Manually verify that the feature you changed works end-to-end in the dev build before opening a PR.

### 5. Open a pull request

- Write a clear PR description explaining **what** changed and **why**.
- Reference the issue number if one exists (`Closes #123`).
- Keep PRs small and focused. Large PRs are harder to review and slower to merge.

---

## Good First Contributions

These areas are well-scoped and do not require deep knowledge of the full codebase:

| Area | Examples |
| :--- | :--- |
| **New document parser** | Add support for `.epub`, `.rtf`, `.html`, or `.pptx` by implementing a new parser in `document/parser.rs` following the existing pattern |
| **LLM pricing** | Update `pricing.json` with new model rates or add missing models |
| **New LLM provider** | Implement the `LLMProvider` trait for a new provider (see `llm/groq.rs` as the simplest example) |
| **UI polish** | Fix layout issues, improve accessibility, add keyboard shortcuts |
| **Documentation** | Improve inline code comments, fix typos, clarify confusing sections |

---

## What to Avoid

- Do not add dependencies without discussing first. The Rust and Node.js dependency trees are intentionally lean.
- Do not change the SQLite schema without providing a migration in `db/schema.rs` and considering backwards compatibility.
- Do not add cloud telemetry, analytics, or any network call that is not an explicit LLM API request initiated by the user.
- Do not break the `LLMProvider` trait interface without updating all 10 provider implementations.

---

## Questions

Open a GitHub Discussion or an issue tagged `question`. Keep questions specific — include the relevant code and what you tried.
