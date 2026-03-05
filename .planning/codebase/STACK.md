# Technology Stack

**Analysis Date:** 2026-03-05

## Languages

**Primary:**
- TypeScript ~5.8.3 - Frontend (React components, stores, Tauri IPC wrappers)
- Rust 2021 Edition - Backend (Tauri commands, document parsing, LLM providers, database)

**Secondary:**
- CSS - Styling via CSS modules and custom properties
- SQL - Embedded SQLite schema in `src-tauri/src/db/schema.rs`

## Runtime

**Environment:**
- Tauri v2 - Desktop shell (Rust backend + WebView frontend)
- Tokio (full features) - Async runtime for Rust backend
- Node.js - Development tooling (Vite dev server, build)

**Package Manager:**
- npm - Frontend (lockfile: `package-lock.json` present)
- pnpm - Also present (`pnpm-lock.yaml` exists)
- Cargo - Rust dependencies (`src-tauri/Cargo.toml`)

## Frameworks

**Core:**
- React ^19.1.0 - Frontend UI framework (`src/`)
- Tauri v2 - Desktop application framework (`src-tauri/`)
- Zustand ^5.0.0 - Frontend state management (`src/stores/`)

**Build/Dev:**
- Vite ^7.0.4 - Frontend bundler and dev server (`vite.config.ts`)
- @vitejs/plugin-react ^4.6.0 - React Fast Refresh support
- TypeScript ~5.8.3 - Type checking (`tsconfig.json`)
- tauri-build v2 - Rust build dependency

## Key Dependencies

### Frontend (`package.json`)

**Critical:**
- `@tauri-apps/api` ^2 - Tauri IPC bridge (`invoke` for command calls, event listeners)
- `@tauri-apps/plugin-opener` ^2 - File/URL opening
- `react` ^19.1.0 - UI rendering
- `react-dom` ^19.1.0 - DOM rendering
- `zustand` ^5.0.0 - State management (4 stores: chat, documents, settings, theme)

**UI/Rendering:**
- `react-markdown` ^10.1.0 - Markdown rendering in chat responses
- `rehype-raw` ^7.0.0 - HTML pass-through in markdown
- `remark-gfm` ^4.0.1 - GitHub Flavored Markdown tables/strikethrough
- `lucide-react` ^0.500.0 - Icon library
- `clsx` ^2.1.0 - Conditional CSS class composition

### Backend (`src-tauri/Cargo.toml`)

**Critical:**
- `tauri` v2 - Application framework, IPC, window management
- `tauri-plugin-opener` v2 - File/URL opener plugin
- `tauri-plugin-dialog` v2 - Native file dialog (used in `open_file_dialog` command)
- `rusqlite` 0.32 (bundled) - SQLite database driver, bundles SQLite C library
- `reqwest` 0.12 (json, stream) - HTTP client for all LLM API calls
- `tokio` v1 (full) - Async runtime

**Serialization:**
- `serde` v1 (derive) - Struct serialization/deserialization
- `serde_json` v1 - JSON handling for LLM APIs and document tree storage

**Document Parsing:**
- `pulldown-cmark` 0.12 - Markdown parsing into AST events
- `pdf-extract` 0.7 - PDF text extraction
- `calamine` 0.26 - Excel/ODS spreadsheet reading (xlsx, xls, ods)
- `csv` v1 - CSV file parsing
- `zip` v2 - ZIP archive reading (used for DOCX parsing)
- `quick-xml` 0.37 - XML parsing (used for DOCX `word/document.xml`)

**Utilities:**
- `uuid` v1 (v4, serde) - UUID generation for IDs
- `chrono` 0.4 (serde) - Date/time handling
- `thiserror` v2 - Ergonomic error type definitions
- `async-trait` 0.1 - Async trait support for `LLMProvider` trait

## Configuration

**TypeScript:**
- `tsconfig.json`: ES2020 target, strict mode, `react-jsx` transform, bundler module resolution
- `tsconfig.node.json`: Separate config for Vite/Node tooling
- No path aliases configured

**Vite:**
- `vite.config.ts`: React plugin, fixed dev port 1420, ignores `src-tauri/` in watch
- Dev URL: `http://localhost:1420`

**Tauri:**
- `src-tauri/tauri.conf.json`: App identifier `com.vectorless.rag`, product name "TGG"
- Window: 1280x800 default, 900x600 minimum
- Bundle targets: MSI, NSIS (Windows installers)
- CSP: null (no Content Security Policy restriction)

**Environment:**
- No `.env` files detected - API keys are stored in SQLite `providers` table
- No `.nvmrc` or `rust-toolchain` files present

**Build:**
- `npm run dev` - Vite dev server
- `npm run build` - TypeScript check + Vite production build
- `npm run tauri` - Tauri CLI entry point

## Platform Requirements

**Development:**
- Rust toolchain (2021 edition)
- Node.js with npm
- Tauri v2 CLI (`@tauri-apps/cli`)
- System dependencies for Tauri (platform-specific WebView, build tools)

**Production:**
- Windows: MSI or NSIS installer (configured in `tauri.conf.json`)
- macOS/Linux: Not configured in bundle targets but code handles platform-specific app data dirs
- SQLite database auto-created at platform app data directory:
  - Windows: `%APPDATA%/vectorless-rag/vectorless-rag.db`
  - macOS: `~/Library/Application Support/vectorless-rag/vectorless-rag.db`
  - Linux: `$XDG_DATA_HOME/vectorless-rag/vectorless-rag.db` (fallback: `~/.local/share/vectorless-rag/`)

---

*Stack analysis: 2026-03-05*
