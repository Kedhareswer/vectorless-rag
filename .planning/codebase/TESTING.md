# Testing Patterns

**Analysis Date:** 2026-03-05

## Test Framework

**Runner:**
- Not configured. No test framework is installed or set up for either frontend or backend.

**Frontend:**
- No test runner (no Jest, Vitest, or similar) in `package.json` dependencies or devDependencies
- No test configuration files (`jest.config.*`, `vitest.config.*`) present
- No `*.test.*` or `*.spec.*` files in the `src/` directory
- No test scripts in `package.json`

**Backend (Rust):**
- No `#[cfg(test)]` modules or `#[test]` functions found anywhere in `src-tauri/src/`
- No integration test directory (`src-tauri/tests/`)
- Standard `cargo test` infrastructure available but unused

## Test File Organization

**Location:**
- No test files exist in the project

**Expected pattern (based on codebase structure):**
- Frontend tests should be co-located with source: `src/components/chat/ChatPanel.test.tsx`
- Store tests co-located: `src/stores/chat.test.ts`
- Rust unit tests as `#[cfg(test)]` modules at bottom of source files
- Rust integration tests in `src-tauri/tests/`

## Test Coverage

**Requirements:** None enforced. No coverage tool configured.

**Current state:** 0% coverage -- no tests exist.

## What Should Be Tested

**High-priority targets (core business logic):**

1. **Document parsing** (`src-tauri/src/document/parser.rs`):
   - Markdown parser: heading hierarchy, code blocks, lists, images, tables
   - Plain text parser, CSV parser, code parser
   - PDF/Word parsing (may need fixtures)
   - Edge cases: empty files, very large files, malformed input

2. **Document tree operations** (`src-tauri/src/document/tree.rs`):
   - `DocumentTree::new()`, `add_node()`, `get_node()`, `tree_overview()`
   - Tree traversal and search operations
   - Relation handling

3. **Agent tools** (`src-tauri/src/agent/tools.rs`, `src-tauri/src/agent/runtime.rs`):
   - Each tool execution: `TreeOverview`, `ExpandNode`, `SearchContent`, `GetRelations`, `CompareNodes`, `GetNodeContext`
   - `AgentRuntime` step budget enforcement
   - `ExplorationContext` tracking and relevance scoring

4. **Query preprocessing** (`src-tauri/src/agent/query.rs`):
   - Intent classification (Entity, Specific, Factual, Broad, etc.)
   - Search term extraction
   - Recommended max steps calculation

5. **Database operations** (`src-tauri/src/db/schema.rs`, `src-tauri/src/db/traces.rs`):
   - CRUD for documents, conversations, messages, providers, bookmarks
   - Trace and step persistence
   - Schema initialization/migration

6. **LLM provider response parsing** (`src-tauri/src/llm/*.rs`):
   - Tool call extraction from various provider response formats
   - Token counting
   - Error handling for API failures

7. **Zustand stores** (`src/stores/*.ts`):
   - State transitions: `createConversation`, `setActiveConversation`, `addMessage`
   - Cross-store interactions: chat store accessing documents store
   - Async actions: `loadConversations`, `loadSessionTotals`
   - Data conversion: `fromTauri()`, `toTauri()` mapping functions

8. **Tauri IPC wrappers** (`src/lib/tauri.ts`):
   - Parameter serialization correctness
   - Type mapping between snake_case and camelCase

## Recommended Test Setup

**Frontend (Vitest recommended -- aligns with Vite build):**

Add to `package.json`:
```json
{
  "devDependencies": {
    "vitest": "^3.0.0",
    "@testing-library/react": "^16.0.0",
    "@testing-library/jest-dom": "^6.0.0"
  },
  "scripts": {
    "test": "vitest run",
    "test:watch": "vitest",
    "test:coverage": "vitest run --coverage"
  }
}
```

Create `vitest.config.ts`:
```typescript
import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  test: {
    environment: 'jsdom',
    globals: true,
    setupFiles: ['./src/test/setup.ts'],
  },
});
```

**Mocking Tauri IPC:**
```typescript
// src/test/setup.ts
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}));
```

**Testing Zustand stores:**
```typescript
import { useChatStore } from '../stores/chat';

beforeEach(() => {
  useChatStore.setState({
    conversations: [],
    activeConversationId: null,
    messages: [],
    explorationSteps: [],
    isExploring: false,
    visitedNodeIds: [],
    activeNodeId: null,
    sessionTotals: { tokens: 0, cost: 0, latency: 0, steps: 0 },
    sessionSteps: [],
    isLoadingSession: false,
  });
});

test('createConversation adds to list and sets active', () => {
  const id = useChatStore.getState().createConversation('Test', 'doc-1');
  const state = useChatStore.getState();
  expect(state.conversations).toHaveLength(1);
  expect(state.activeConversationId).toBe(id);
});
```

**Backend (Rust standard test framework):**

Example unit test structure for `src-tauri/src/document/tree.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_tree_has_root() {
        let tree = DocumentTree::new("test.md".to_string(), DocType::Markdown);
        assert!(tree.get_node(&tree.root_id).is_some());
    }

    #[test]
    fn test_add_node_to_root() {
        let mut tree = DocumentTree::new("test.md".to_string(), DocType::Markdown);
        let root_id = tree.root_id.clone();
        let node = TreeNode::new(NodeType::Section, "Test Section".to_string());
        let node_id = node.id.clone();
        tree.add_node(&root_id, node).unwrap();
        assert!(tree.get_node(&node_id).is_some());
    }
}
```

Example for database with in-memory SQLite:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> Database {
        let db = Database::new(":memory:").unwrap();
        db.initialize().unwrap();
        db
    }

    #[test]
    fn test_save_and_list_documents() {
        let db = test_db();
        // ... test CRUD
    }
}
```

## Test Types

**Unit Tests:**
- Not present. Should cover: parsers, tree operations, agent tools, query preprocessing, store logic, data converters.

**Integration Tests:**
- Not present. Should cover: full agent query flow (document ingestion -> query -> tool calls -> response), database schema migrations, LLM provider API response parsing with fixture data.

**E2E Tests:**
- Not present. Tauri has `tauri-driver` for WebDriver-based E2E testing but it is not set up.

## Fixtures and Factories

**Test Data:**
- No fixtures directory exists
- Recommended locations:
  - `src-tauri/tests/fixtures/` for sample documents (`.md`, `.txt`, `.csv` files)
  - `src-tauri/tests/fixtures/responses/` for LLM provider response JSON fixtures
  - `src/test/fixtures/` for frontend mock data

**Pricing data:**
- `src-tauri/src/pricing.json` is embedded at compile time via `include_str!` -- can serve as test fixture for cost estimation tests

## CI/CD Testing

**Current state:** No CI pipeline detected for running tests.

**Recommended:**
- Add test scripts to `package.json` and run in CI
- Run `cargo test` for Rust backend in CI
- Block merges on test failures

---

*Testing analysis: 2026-03-05*
