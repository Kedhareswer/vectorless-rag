# Coding Conventions

**Analysis Date:** 2026-03-05

## Naming Patterns

**Files (TypeScript):**
- Components: PascalCase with `.tsx` extension (e.g., `ChatPanel.tsx`, `IconButton.tsx`, `PreviewPanel.tsx`)
- CSS Modules: Same name as component with `.module.css` suffix (e.g., `ChatPanel.module.css`)
- Stores: lowercase singular noun with `.ts` extension (e.g., `chat.ts`, `documents.ts`, `settings.ts`, `theme.ts`)
- Lib/utility files: lowercase with `.ts` extension (e.g., `tauri.ts`)

**Files (Rust):**
- Module files: `snake_case.rs` (e.g., `commands.rs`, `schema.rs`, `runtime.rs`)
- Module directories: `snake_case/` with `mod.rs` barrel file (e.g., `llm/mod.rs`, `agent/mod.rs`)
- One file per LLM provider: `groq.rs`, `anthropic.rs`, `openai_compat.rs`

**Functions (TypeScript):**
- camelCase for all functions and handlers: `handleSend`, `loadDocuments`, `setActiveDocument`
- Event handlers prefixed with `handle`: `handleKeyDown`, `handleDragOver`, `handleDrop`
- Async operations prefixed with action verb: `loadConversations`, `saveProviderToBackend`, `deleteDocumentFromBackend`
- IPC wrappers use bare verb: `listDocuments()`, `getDocument()`, `ingestDocument()`

**Functions (Rust):**
- snake_case for all functions: `list_documents`, `get_tree_overview`, `save_trace_data`
- Constructor pattern: `fn new(config: ProviderConfig) -> Self`
- Builder pattern for prompts: `build_system_prompt()`, `build_llm_tools()`

**Variables (TypeScript):**
- camelCase for all variables: `activeDocumentId`, `isExploring`, `explorationSteps`
- Boolean variables prefixed with `is`/`has`/`no`: `isExploring`, `isIngesting`, `hasContent`, `noProvider`
- Refs suffixed with `Ref`: `messagesEndRef`, `textareaRef`, `prevDocId`

**Variables (Rust):**
- snake_case: `cancel_flag`, `provider_config`, `tool_call_counter`
- Constants/statics: UPPER_SNAKE_CASE with `OnceLock`: `static TABLE: OnceLock<...>`

**Types/Interfaces (TypeScript):**
- PascalCase for all types and interfaces: `ChatMessage`, `ExplorationStep`, `ProviderConfig`
- Interface for component props: `IconButtonProps`, `ExplorationStepPayload`
- State interface suffixed with `State`: `ChatState`, `DocumentsState`, `SettingsState`
- Tauri-returned types use snake_case fields matching Rust structs (e.g., `doc_type`, `created_at`)
- Frontend interfaces use camelCase fields (e.g., `docType`, `createdAt`)

**Types (Rust):**
- PascalCase for structs, enums, traits: `DocumentTree`, `AgentTool`, `LLMProvider`
- Error enums suffixed with `Error`: `LLMError`, `DbError`, `ParseError`, `TreeError`
- Record structs suffixed with `Record`: `TraceRecord`, `StepRecord`, `MessageRecord`
- Event structs suffixed with `Event`: `ChatResponseEvent`, `ChatTokenEvent`

## Code Style

**Formatting:**
- No ESLint or Prettier config detected -- relies on TypeScript strict mode settings
- TypeScript strict mode enabled in `tsconfig.json`: `strict: true`, `noUnusedLocals: true`, `noUnusedParameters: true`
- Single quotes for strings in TypeScript (consistent throughout codebase)
- Double quotes for Rust string literals (language default)
- 2-space indentation in TypeScript/CSS, 4-space in Rust

**Linting:**
- No ESLint configuration present
- Rust uses standard `cargo` checks with `thiserror` for error types
- TypeScript compiler (`tsc`) serves as the linting layer via strict options

## Import Organization

**Order (TypeScript):**
1. React core imports: `import { useState, useEffect } from 'react'`
2. External libraries: `import clsx from 'clsx'`, `import { listen } from '@tauri-apps/api/event'`
3. Icon imports from lucide-react: `import { Send, Square, FileText } from 'lucide-react'`
4. Internal stores: `import { useChatStore } from '../../stores/chat'`
5. Internal components: `import { ThinkingBlock } from './ThinkingBlock'`
6. CSS modules (always last): `import styles from './ChatPanel.module.css'`

**Order (Rust):**
1. Standard library: `use std::collections::HashMap`
2. External crates: `use serde::Serialize`, `use async_trait::async_trait`
3. Internal modules via `super::` or `crate::`: `use crate::document::tree::DocumentTree`

**Path Aliases:**
- No path aliases configured. All imports use relative paths with `../../` navigation.
- Rust uses `crate::` for absolute module paths and `super::` for parent module references.

## Error Handling

**TypeScript Patterns:**
- Async store actions wrapped in try/catch with `console.warn` for non-critical failures:
  ```typescript
  try {
    const records = await listConversations();
    set({ conversations });
  } catch (err) {
    console.warn('Failed to load conversations:', err);
  }
  ```
- Fire-and-forget for non-critical persistence: `.catch((err) => console.warn('Failed to save:', err))`
- User-facing errors stored in store state as `error: string | null` with `clearError()` method
- Component-level errors via `useState<string | null>`: `sendError` in `ChatPanel.tsx`
- Error conversion via `String(err)` for display

**Rust Patterns:**
- All Tauri commands return `Result<T, String>` -- errors converted via `.map_err(|e| e.to_string())`
- Database lock acquisition: `db.lock().map_err(|e| format!("Lock error: {}", e))?`
- Domain-specific error enums using `thiserror`:
  ```rust
  #[derive(Error, Debug)]
  pub enum LLMError {
      #[error("HTTP request failed: {0}")]
      RequestError(#[from] reqwest::Error),
      #[error("API error: {0}")]
      ApiError(String),
  }
  ```
- Error types per module: `LLMError` in `src-tauri/src/llm/provider.rs`, `DbError` in `src-tauri/src/db/schema.rs`, `ParseError` in `src-tauri/src/document/parser.rs`, `TreeError` in `src-tauri/src/document/tree.rs`
- `thiserror` `#[from]` for automatic conversion from dependency errors

## Logging

**Framework:** Console (browser dev tools) on frontend, no structured logging on backend

**Patterns:**
- Use `console.warn` for recoverable errors in store actions (never `console.error`)
- No `console.log` statements detected in production code
- Rust backend has no logging framework -- errors propagate via `Result`

## Comments

**When to Comment:**
- Section dividers in Rust commands file: `// --- Document commands ---`, `// --- Provider commands ---`
- Doc comments (`///`) on public Rust functions and structs for important abstractions
- Inline comments for non-obvious logic: `// Guard against stale results`, `// Fire-and-forget`
- Feature tracking comments: `/** Feature 4: Live Visualization */`, `// (Feature 1: Chat Persistence)`
- JSDoc-style `/** */` comments on TypeScript interface fields for important context

**JSDoc/TSDoc:**
- Minimal. Used sparingly on interface fields:
  ```typescript
  /** Cost in $ for this step, computed by the backend using per-model input/output rates */
  cost: number;
  ```
- Rust doc comments on key public items only -- not exhaustive

## Component Design

**Pattern:** Named function exports (not default exports for components)
```typescript
export function ChatPanel() { ... }
export function IconButton({ icon, onClick, ... }: IconButtonProps) { ... }
```
- Exception: `App.tsx` uses `export default App`
- Components are function components only -- no class components
- Props destructured in function signature

**State Management:**
- Zustand stores in `src/stores/`, one per domain
- Store creation: `create<StateInterface>((set, get) => ({ ... }))`
- Store access in components via selector: `useChatStore((s) => s.isExploring)`
- Complex destructuring for multiple fields: `const { messages, explorationSteps, ... } = useChatStore()`
- Cross-store access via `useDocumentsStore.getState()` (not hooks) when called from another store

**Hooks:**
- Standard React hooks only (`useState`, `useEffect`, `useRef`, `useCallback`)
- No custom hooks in `src/hooks/` directory (exists but empty)
- `useEffect` with dependency arrays always specified
- Cleanup functions returned from `useEffect` for event listeners

## CSS / Styling

**Approach:** CSS Modules with design tokens via CSS custom properties

**Pattern:**
- Every component has a co-located `.module.css` file
- Global design tokens in `src/styles/theme.css` using `:root` and `[data-theme="dark"]`
- Apply styles via `styles.className` from module import
- Conditional classes via `clsx`: `clsx(styles.bubble, msg.role === 'user' ? styles.bubbleUser : styles.bubbleAssistant)`

**Token naming:**
- Prefixed by category: `--bg-primary`, `--text-secondary`, `--accent`, `--border`, `--radius-sm`, `--shadow-md`, `--transition-fast`
- Use tokens for all colors, radii, shadows, transitions -- never hardcode values in component CSS

**CSS class naming:**
- camelCase within CSS modules: `.inputBar`, `.sendButton`, `.emptyTitle`
- Variant classes as standalone: `.ghost`, `.default`, `.sm`, `.md`
- State modifiers: `.sendButtonActive`, `.stopButton`, `.exploringIndicator`

## Tauri IPC Conventions

**Frontend wrapper pattern** in `src/lib/tauri.ts`:
```typescript
export const listDocuments = () => invoke<DocumentSummary[]>('list_documents');
export const getDocument = (id: string) => invoke<DocumentTree>('get_document', { id });
```
- All IPC wrappers are exported `const` arrow functions
- Parameter names match Rust command parameter names (camelCase in TS, mapped to snake_case by Tauri)
- Return types explicitly generic on `invoke<T>`
- Types matching Rust structs use snake_case field names (the Tauri serialization boundary)

**Rust command pattern:**
```rust
#[tauri::command]
pub fn command_name(db: State<Mutex<Database>>, param: Type) -> Result<T, String> {
    let db = db.lock().map_err(|e| format!("Lock error: {}", e))?;
    db.method(&param).map_err(|e| e.to_string())
}
```
- All commands are `pub fn` with `#[tauri::command]`
- Database accessed via `State<Mutex<Database>>`
- Async commands use `pub async fn` (e.g., `chat_with_agent`, `open_file_dialog`)

## Data Conversion Pattern

**Snake_case (Rust/Tauri) to camelCase (Frontend):**
- Explicit conversion functions in stores: `fromTauri()`, `fromTauriSummary()`, `toTauri()`
- Tauri types in `src/lib/tauri.ts` use snake_case fields (matching Rust)
- Store interfaces use camelCase fields (matching JS convention)
- Serde `#[serde(rename = "camelCase")]` used on event structs sent to frontend

## Module Design

**Exports (TypeScript):**
- Named exports for components: `export function ChatPanel()`
- Named exports for stores: `export const useChatStore = create<...>()`
- Named exports for IPC wrappers: `export const listDocuments = () => ...`
- Re-export types from stores: `export interface Conversation { ... }`

**Exports (Rust):**
- Barrel `mod.rs` files re-export public items: `pub use schema::{Database, DbError, ...}`
- One `pub mod` per submodule in `mod.rs`
- Public items marked `pub`, internal items unmarked

**Barrel Files:**
- Rust: `mod.rs` in each module directory with `pub mod` and `pub use` re-exports
- TypeScript: No barrel `index.ts` files -- imports go directly to source files

---

*Convention analysis: 2026-03-05*
