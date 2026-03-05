# External Integrations

**Analysis Date:** 2026-03-05

## APIs & External Services

All LLM provider integrations share a common `LLMProvider` trait defined in `src-tauri/src/llm/provider.rs`. Each provider implements `async fn chat()` returning `LLMResponse` with token tracking. Provider configs (including API keys) are stored in SQLite and managed via Tauri IPC commands. The provider factory is in `src-tauri/src/commands.rs` (`create_provider` function, line 398).

### Anthropic (Direct API)
- SDK/Client: Custom implementation in `src-tauri/src/llm/anthropic.rs`
- Base URL: `https://api.anthropic.com/v1`
- Auth: `x-api-key` header + `anthropic-version` header
- Default model: `claude-sonnet-4-6`
- API format: Anthropic Messages API (distinct from OpenAI format)

### Google AI Studio (Gemini)
- SDK/Client: Custom implementation in `src-tauri/src/llm/google.rs`
- Base URL: `https://generativelanguage.googleapis.com/v1beta`
- Auth: API key in URL query param
- Special handling: Uses Gemini-specific tool definition format (`get_gemini_tool_definitions()` in `src-tauri/src/agent/tools.rs`)
- Models: gemini-2.5-pro, gemini-2.5-flash, gemini-2.0-flash, gemini-1.5-pro, gemini-1.5-flash

### OpenRouter
- SDK/Client: Custom implementation in `src-tauri/src/llm/openrouter.rs`
- Base URL: `https://openrouter.ai/api/v1`
- Auth: Bearer token
- Provides access to Claude, GPT, Llama, Mistral, DeepSeek models via single API

### AgentRouter
- SDK/Client: Custom implementation in `src-tauri/src/llm/agentrouter.rs`
- Base URL: `https://agentrouter.org/v1`
- Auth: `x-api-key` header (Anthropic-compatible proxy)
- Default model: `claude-sonnet-4-5-20250514`
- API format: Anthropic Messages API format

### Groq
- SDK/Client: Custom implementation in `src-tauri/src/llm/groq.rs`
- Base URL: `https://api.groq.com/openai/v1`
- Auth: Bearer token
- Models: Llama, Mixtral, Gemma (fast inference)

### OpenAI-Compatible Providers (shared implementation)
All use `OpenAICompatProvider` in `src-tauri/src/llm/openai_compat.rs` with different base URLs:

**OpenAI:**
- Base URL: `https://api.openai.com/v1`
- Auth: Bearer token
- Models: GPT-4o, GPT-4.1, o1, o3-mini, o4-mini

**DeepSeek:**
- Base URL: `https://api.deepseek.com/v1`
- Auth: Bearer token
- Models: deepseek-chat, deepseek-reasoner

**xAI (Grok):**
- Base URL: `https://api.x.ai/v1`
- Auth: Bearer token
- Models: grok-3, grok-3-mini

**Qwen (Alibaba):**
- Base URL: `https://dashscope-intl.aliyuncs.com/compatible-mode/v1`
- Auth: Bearer token
- Models: qwen-max, qwen-plus, qwen-turbo

**Custom OpenAI-Compatible:**
- Base URL: User-provided
- Provider name: `openai-compat`
- For any service exposing a standard `/v1/chat/completions` endpoint

### Ollama (Local)
- SDK/Client: Custom implementation in `src-tauri/src/llm/ollama.rs`
- Base URL: `http://localhost:11434` (local)
- Auth: None (no API key required)
- Models: User-configured (Llama, Mistral, LLaVA for vision)
- Requires Ollama running locally

## Data Storage

**Database:**
- SQLite via `rusqlite` 0.32 (bundled)
  - Connection: Resolved by `resolve_db_path()` in `src-tauri/src/lib.rs`
  - Client: Direct `rusqlite::Connection` wrapped in `Mutex<Database>` as Tauri state
  - Schema: `src-tauri/src/db/schema.rs` (inline SQL `CREATE TABLE` statements)
  - Trace queries: `src-tauri/src/db/traces.rs`
  - PRAGMA: WAL journal mode, foreign keys ON
  - Tables: `documents`, `conversations`, `messages`, `exploration_steps`, `traces`, `evals`, `settings`, `providers`, `bookmarks`
  - Migrations: Inline in `run_migrations()` method (adds columns if missing)

**File Storage:**
- Local filesystem only
- Documents read from user-selected file paths (via native file dialog)
- Document trees serialized as JSON blobs in SQLite `documents.tree_json` column

**Caching:**
- None (no Redis, no in-memory cache layer)

## Authentication & Identity

**Auth Provider:**
- None - Desktop application with no user accounts
- LLM API keys stored in SQLite `providers.api_key_encrypted` column
  - Note: Column named `api_key_encrypted` but currently stores plain text (no encryption observed in `save_provider` / `get_providers` in `src-tauri/src/db/schema.rs`)

## Monitoring & Observability

**Error Tracking:**
- None (no Sentry, no external error reporting)

**Logs:**
- No structured logging framework
- Errors propagated via `Result<T, String>` through Tauri IPC
- LLM errors emitted to frontend via `chat-error` Tauri event

**Tracing (Local):**
- Custom Langfuse-style local tracing in SQLite
- Token usage tracked per LLM turn (input/output tokens split)
- Cost estimation via `src-tauri/src/pricing.json` (per-model $/1M token rates)
- Latency measured per LLM turn, distributed across tool calls
- Exploration steps recorded in `exploration_steps` table
- Trace summaries in `traces` table with cost, latency, step count

## CI/CD & Deployment

**Hosting:**
- Desktop application (no server hosting)
- Distributed as Windows installer (MSI/NSIS)

**CI Pipeline:**
- Not detected in repository root (no `.github/workflows/`, no `Jenkinsfile`, etc.)

## Environment Configuration

**Required env vars:**
- None at build time
- All configuration stored in SQLite `settings` and `providers` tables

**Secrets location:**
- LLM API keys stored in SQLite database at app data directory
- No `.env` files used
- No external secrets manager

**Per-provider API key requirements:**
- Anthropic: API key required
- Google AI Studio: API key required
- OpenRouter: API key required
- AgentRouter: API key required
- Groq: API key required
- OpenAI: API key required
- DeepSeek: API key required
- xAI: API key required
- Qwen: API key required
- Ollama: No API key required (local)

## Webhooks & Callbacks

**Incoming:**
- None

**Outgoing:**
- None

## IPC Event System (Tauri)

The frontend communicates with the Rust backend via two mechanisms:

**Tauri Commands (Request/Response):**
- All defined in `src-tauri/src/commands.rs`
- Frontend wrappers in `src/lib/tauri.ts`
- Pattern: `invoke<ReturnType>('command_name', { args })`

**Tauri Events (Backend-to-Frontend Push):**
- `chat-token` - Streaming response tokens (`ChatTokenEvent`: requestId, token, done)
- `chat-response` - Complete response (`ChatResponseEvent`: requestId, content)
- `chat-error` - Error during agent chat (`ChatErrorEvent`: requestId, error)
- `exploration-step-start` - Tool call started (`ExplorationStepStartEvent`: requestId, stepNumber, tool, inputSummary)
- `exploration-step-complete` - Tool call finished (`ExplorationStepCompleteEvent`: requestId, stepNumber, outputSummary, tokensUsed, latencyMs, cost, nodeIds)

## Cost Tracking

**Pricing Model:**
- Per-model pricing table embedded in `src-tauri/src/pricing.json`
- Rates in $/1M tokens with separate input and output rates
- Default fallback: $0.50/1M input, $1.50/1M output
- Cost calculated per LLM turn and distributed across tool calls
- Aggregate cost summary queryable via `get_cost_summary` command

---

*Integration audit: 2026-03-05*
