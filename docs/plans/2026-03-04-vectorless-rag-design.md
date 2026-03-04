# Vectorless RAG — Full Design Document
**Date**: 2026-03-04
**Status**: Approved

## 1. Problem Statement
Traditional RAG systems rely on embeddings and vector similarity search to retrieve document chunks. This approach loses document structure, ignores relationships between sections, and returns decontextualized fragments. We build a fundamentally different approach: **agentic document exploration** where an LLM navigates structured document trees using tools, preserving context and relationships.

## 2. Core Innovation
Instead of: `query → embed → vector search → retrieve chunks → stuff into prompt`
We do: `query → agent plans exploration → navigates tree with tools → reasons over structure → answers`

The LLM acts like a researcher browsing a document — scanning the table of contents, diving into relevant sections, cross-referencing, examining diagrams — rather than a search engine returning keyword matches.

## 3. Architecture

### 3.1 System Layers
```
┌─────────────────────────────────────────────┐
│           React Frontend (Vite)             │
│  Sidebar │ Chat + Agent Steps │ Preview     │
├─────────────────────────────────────────────┤
│           Tauri IPC Bridge                  │
├─────────────────────────────────────────────┤
│           Rust Backend Core                 │
│  Document Engine │ Agent Runtime │ LLM Layer│
├─────────────────────────────────────────────┤
│     SQLite DB        │    File Store        │
└─────────────────────────────────────────────┘
```

### 3.2 Document Engine
**Parsers** (each outputs Universal Document Tree nodes):
- PDF: `pdf-extract` or `pdfium` bindings → headings, paragraphs, tables, images
- Markdown: `pulldown-cmark` → headers, code blocks, lists, links
- Word (.docx): `docx-rs` → structured content extraction
- Code files: `tree-sitter` → AST-based structure (functions, classes, modules)
- Images: Extract metadata, defer description to vision LLM
- Plain text: Heuristic splitting by paragraphs/sections

**Universal Document Tree (UDT)**:
```rust
struct DocumentTree {
    id: Uuid,
    name: String,
    doc_type: DocType,
    root: NodeId,
    nodes: HashMap<NodeId, TreeNode>,
    created_at: DateTime,
}

struct TreeNode {
    id: NodeId,
    node_type: NodeType,     // Section, Paragraph, Table, Image, Code, etc.
    content: String,          // Text content or description
    metadata: NodeMetadata,   // Page number, position, format-specific data
    children: Vec<NodeId>,
    relations: Vec<Relation>, // Cross-references, links, dependencies
    summary: Option<String>,  // LLM-generated summary (lazy)
    raw_image: Option<PathBuf>, // For image nodes
}

struct Relation {
    target: NodeId,
    relation_type: RelationType, // References, DependsOn, SimilarTo, Contains
    label: Option<String>,
}
```

### 3.3 Agent Runtime
**Tool definitions** (exposed to LLM via function calling):

| Tool | Input | Output | Purpose |
|------|-------|--------|---------|
| `tree_overview` | `doc_id` | Top-level nodes with summaries | Get document structure |
| `expand_node` | `node_id, depth?` | Children and their content | Dive into a section |
| `search_content` | `query, scope?` | Matching nodes with context | Grep-like search |
| `get_relations` | `node_id` | Related nodes and edge types | Follow cross-references |
| `get_image` | `node_id` | Image description (via vision LLM) | Understand visual content |
| `compare_nodes` | `node_a, node_b` | Side-by-side content | Cross-reference sections |
| `get_node_context` | `node_id` | Parent chain + siblings | Understand position in doc |

**Agent loop**:
1. User asks a question
2. Agent receives question + document overview (compressed)
3. Agent decides which tool to call
4. Tool returns result, agent reasons about it
5. Agent decides: answer or explore more
6. Repeat 3-5 until confident
7. Return answer with source node references

**Context management**:
- Running summary of explored nodes (avoids re-reading)
- Exploration budget (max steps configurable, default 10)
- Progressive context: start with overview, add detail as agent drills down

### 3.4 LLM Provider Layer
```rust
#[async_trait]
trait LLMProvider: Send + Sync {
    async fn chat(&self, messages: Vec<Message>, tools: Option<Vec<Tool>>) -> Result<Response>;
    async fn chat_stream(&self, messages: Vec<Message>, tools: Option<Vec<Tool>>) -> Result<StreamHandle>;
    async fn describe_image(&self, image: &[u8], prompt: &str) -> Result<String>;
    fn capabilities(&self) -> ProviderCapabilities;
    fn name(&self) -> &str;
}

struct ProviderCapabilities {
    supports_vision: bool,
    supports_tool_calling: bool,
    max_context_tokens: usize,
    supports_streaming: bool,
}
```

**Providers**:
- **Groq**: Llama 3.3 70B, Mixtral 8x7B, Gemma 2 9B — fast inference, tool calling
- **Google AI Studio**: Gemini 2.5 Pro, Gemini 2.5 Flash — vision + long context
- **OpenRouter**: Claude 4, GPT-4.1, Llama 3.3, DeepSeek V3, Mistral Large — unified access
- **Ollama**: Llama 3.3, Mistral, Qwen 2.5, LLaVA (vision) — fully offline

**Fallback logic**: If active provider lacks vision → route image descriptions to first available vision-capable provider.

### 3.5 Database Schema (SQLite)
```sql
-- Document trees
CREATE TABLE documents (id TEXT PK, name, doc_type, file_path, tree_json, created_at, updated_at);
CREATE TABLE nodes (id TEXT PK, doc_id FK, node_type, content, metadata_json, parent_id, position);
CREATE TABLE relations (id TEXT PK, source_node_id FK, target_node_id FK, relation_type, label);

-- Chat & exploration
CREATE TABLE conversations (id TEXT PK, title, doc_id FK, created_at, updated_at);
CREATE TABLE messages (id TEXT PK, conv_id FK, role, content, created_at);
CREATE TABLE exploration_steps (id TEXT PK, msg_id FK, tool_name, input_json, output_json, tokens_used, latency_ms, cost);

-- Tracing & evaluation
CREATE TABLE traces (id TEXT PK, conv_id FK, total_tokens, total_cost, total_latency_ms, steps_count, created_at);
CREATE TABLE evals (id TEXT PK, trace_id FK, metric, score, details_json);

-- Settings
CREATE TABLE settings (key TEXT PK, value_json);
CREATE TABLE providers (id TEXT PK, name, api_key_encrypted, base_url, model, is_active, capabilities_json);
```

## 4. UI Design

### 4.1 Layout
Three-panel adaptive layout:
- **Sidebar** (240px, collapsible to 48px): Document library, conversation history, settings
- **Chat Panel** (flexible): Messages, agent exploration steps with animated thinking blocks
- **Preview Panel** (420px, collapsible): Document tree view, canvas graph, trace/eval, image preview

### 4.2 Theme System
Two themes (light + dark) with system auto-detect.

**Light mode** (Claude Desktop-inspired):
- Background: warm cream (#F4F3EE), cards white (#FFFFFF)
- Text: warm black (#1C1917), secondary stone (#78716C)
- Accent: Claude peach (#DE7356), deep crail (#C15F3C)
- Borders: subtle warm gray (#E7E5E0), shadows over borders

**Dark mode**:
- Background: warm charcoal (#1C1917), cards (#282420)
- Text: light cream (#F4F3EE), secondary (#A8A29E)
- Accent: same peach (#DE7356), lighter for hover (#E8845E)
- Borders: warm dark (#3D3730)

**Typography**: Inter 400/500/600 for UI, JetBrains Mono 400 for code/traces.

### 4.3 Agent Step Animation
1. User sends message
2. Pulsing accent-colored bar appears (CSS animation, 2s pulse cycle)
3. As each tool completes, bar transforms into compact card (slide-down, 200ms ease)
4. Card shows: tool icon + name, one-line summary, duration badge
5. Card is clickable → expands to show full input/output
6. During exploration, right panel tree view highlights current node with glow effect
7. Final answer streams below the step cards

### 4.4 Preview Panel Tabs
- **Tree**: Interactive document tree, expandable nodes, click to view content
- **Canvas**: Force-directed graph of document nodes and relations (using d3-force or @antv/g6)
- **Trace**: Timeline of agent steps, token/cost counters, latency chart
- **Image**: Full-size preview when exploring image nodes

### 4.5 Settings Screen
- Provider management: add/remove providers, set API keys, select models
- Model picker per provider (fetched from provider APIs where possible)
- Default provider selection, fallback chain configuration
- Exploration settings: max steps, context budget, auto-summarize toggle
- Theme toggle + system auto-detect
- Data management: export/import documents, clear traces

## 5. Document Ingestion Flow
1. User drops file(s) or clicks "Add Document"
2. File type detected, appropriate parser selected
3. **Phase 1 (fast)**: Extract text structure → build tree → store in SQLite
4. **Phase 2 (background)**: Generate top-level summaries via LLM
5. **Phase 3 (lazy)**: Image descriptions generated on first access
6. Progress shown in sidebar with status indicator per document
7. Document immediately available for exploration after Phase 1

## 6. Implementation Phases

### Phase 1: Foundation
- Tauri v2 project setup with React
- Theme system (light + dark)
- 3-panel layout shell
- SQLite database setup
- Settings screen with provider config

### Phase 2: Document Engine
- Markdown parser → UDT
- Plain text parser → UDT
- PDF parser → UDT
- Basic tree view in preview panel
- Document library in sidebar

### Phase 3: LLM Integration
- Provider trait + Ollama implementation (local-first)
- Groq implementation
- Google AI Studio implementation
- OpenRouter implementation
- Streaming responses via Tauri events

### Phase 4: Agent Runtime
- Tool definitions and execution
- Agent loop with tool calling
- Context management and progressive disclosure
- Animated thinking blocks in chat UI

### Phase 5: Advanced Features
- Canvas/graph view with d3-force
- Image node handling with vision LLMs
- Full tracing and eval panel
- Word document parser
- Code file parser (tree-sitter)
- Export/import functionality

## 7. Key Risks & Mitigations
| Risk | Mitigation |
|------|-----------|
| Large documents overflow context | Progressive disclosure, summarization, exploration budget |
| Vision API costs | Lazy processing, local LLaVA via Ollama, caching descriptions |
| Provider API differences | Unified trait abstraction, capability detection |
| PDF parsing complexity | Start with well-structured PDFs, improve iteratively |
| Agent goes in circles | Step limit, visited-node tracking, backtracking detection |
