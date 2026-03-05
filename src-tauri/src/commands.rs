use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use tauri::{Emitter, State};
use serde::Deserialize;

/// Shared cancellation flag — set to true to abort the running query.
pub struct CancelFlag(pub Arc<AtomicBool>);

/// Truncate a string at a UTF-8 safe char boundary.
fn safe_truncate(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

use crate::agent::runtime::{build_system_prompt, AgentRuntime};
use crate::agent::tools::{AgentTool, ToolInput};
use crate::agent::query::{preprocess_query, QueryIntent};
use crate::db::{Database, DocumentSummary, ConversationRecord, MessageRecord, CostSummaryRecord, StepRecord, TraceRecord};
use crate::document::parser::get_parser_for_file;
use crate::document::tree::{DocumentTree, TreeNode, TreeNodeSummary};
use crate::llm::provider::{LLMProvider, Message, ProviderConfig, Tool};
use crate::llm::{AgentRouterProvider, AnthropicProvider, GoogleProvider, GroqProvider, OllamaProvider, OpenAICompatProvider, OpenRouterProvider};

// --- Document commands ---

#[tauri::command]
pub fn list_documents(db: State<Mutex<Database>>) -> Result<Vec<DocumentSummary>, String> {
    let db = db.lock().map_err(|e| format!("Lock error: {}", e))?;
    db.list_documents().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_document(db: State<Mutex<Database>>, id: String) -> Result<DocumentTree, String> {
    let db = db.lock().map_err(|e| format!("Lock error: {}", e))?;
    db.get_document(&id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Document not found: {}", id))
}

#[tauri::command]
pub fn ingest_document(
    db: State<Mutex<Database>>,
    file_path: String,
) -> Result<DocumentTree, String> {
    let parser = get_parser_for_file(&file_path);
    let tree = parser.parse(&file_path).map_err(|e| e.to_string())?;
    let db = db.lock().map_err(|e| format!("Lock error: {}", e))?;
    db.save_document(&tree, Some(&file_path))
        .map_err(|e| e.to_string())?;
    Ok(tree)
}

#[tauri::command]
pub fn delete_document(db: State<Mutex<Database>>, id: String) -> Result<(), String> {
    let db = db.lock().map_err(|e| format!("Lock error: {}", e))?;
    db.delete_document(&id).map_err(|e| e.to_string())
}

// --- Tree exploration commands ---

#[tauri::command]
pub fn get_tree_overview(
    db: State<Mutex<Database>>,
    doc_id: String,
) -> Result<Vec<TreeNodeSummary>, String> {
    let db = db.lock().map_err(|e| format!("Lock error: {}", e))?;
    let tree = db
        .get_document(&doc_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Document not found: {}", doc_id))?;
    Ok(tree.tree_overview())
}

#[tauri::command]
pub fn expand_node(
    db: State<Mutex<Database>>,
    doc_id: String,
    node_id: String,
) -> Result<TreeNode, String> {
    let db = db.lock().map_err(|e| format!("Lock error: {}", e))?;
    let tree = db
        .get_document(&doc_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Document not found: {}", doc_id))?;
    tree.get_node(&node_id)
        .cloned()
        .ok_or_else(|| format!("Node not found: {}", node_id))
}

#[tauri::command]
pub fn search_document(
    db: State<Mutex<Database>>,
    doc_id: String,
    query: String,
) -> Result<Vec<TreeNode>, String> {
    let db = db.lock().map_err(|e| format!("Lock error: {}", e))?;
    let tree = db
        .get_document(&doc_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Document not found: {}", doc_id))?;

    let query_lower = query.to_lowercase();
    let matches: Vec<TreeNode> = tree
        .nodes
        .values()
        .filter(|node| node.content.to_lowercase().contains(&query_lower))
        .cloned()
        .collect();

    Ok(matches)
}

// --- Provider commands ---

#[tauri::command]
pub fn get_providers(db: State<Mutex<Database>>) -> Result<Vec<ProviderConfig>, String> {
    let db = db.lock().map_err(|e| format!("Lock error: {}", e))?;
    db.get_providers().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_provider(
    db: State<Mutex<Database>>,
    config: ProviderConfig,
) -> Result<(), String> {
    let db = db.lock().map_err(|e| format!("Lock error: {}", e))?;
    db.save_provider(&config).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_provider(db: State<Mutex<Database>>, id: String) -> Result<(), String> {
    let db = db.lock().map_err(|e| format!("Lock error: {}", e))?;
    db.delete_provider(&id).map_err(|e| e.to_string())
}

// --- Settings commands ---

#[tauri::command]
pub fn get_setting(db: State<Mutex<Database>>, key: String) -> Result<Option<String>, String> {
    let db = db.lock().map_err(|e| format!("Lock error: {}", e))?;
    db.get_setting(&key).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_setting(
    db: State<Mutex<Database>>,
    key: String,
    value: String,
) -> Result<(), String> {
    let db = db.lock().map_err(|e| format!("Lock error: {}", e))?;
    db.set_setting(&key, &value).map_err(|e| e.to_string())
}

// --- Conversation commands (Feature 1: Chat Persistence) ---

#[tauri::command]
pub fn list_conversations(db: State<Mutex<Database>>) -> Result<Vec<ConversationRecord>, String> {
    let db = db.lock().map_err(|e| format!("Lock error: {}", e))?;
    db.list_conversations().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_conversation(
    db: State<Mutex<Database>>,
    id: String,
    title: String,
    doc_id: Option<String>,
) -> Result<(), String> {
    let db = db.lock().map_err(|e| format!("Lock error: {}", e))?;
    let now = chrono::Utc::now().to_rfc3339();
    // Preserve original created_at if conversation already exists
    let created_at = db.get_conversation_created_at(&id)
        .unwrap_or(None)
        .unwrap_or_else(|| now.clone());
    let conv = ConversationRecord {
        id,
        title,
        doc_id,
        created_at,
        updated_at: now,
    };
    db.save_conversation(&conv).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_conversation_messages(
    db: State<Mutex<Database>>,
    conv_id: String,
) -> Result<Vec<MessageRecord>, String> {
    let db = db.lock().map_err(|e| format!("Lock error: {}", e))?;
    db.get_conversation_messages(&conv_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_message(
    db: State<Mutex<Database>>,
    id: String,
    conv_id: String,
    role: String,
    content: String,
) -> Result<(), String> {
    let now = chrono::Utc::now().to_rfc3339();
    let msg = MessageRecord {
        id,
        conv_id,
        role,
        content,
        created_at: now,
    };
    let db = db.lock().map_err(|e| format!("Lock error: {}", e))?;
    db.save_message(&msg).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_conversation(db: State<Mutex<Database>>, conv_id: String) -> Result<(), String> {
    let db = db.lock().map_err(|e| format!("Lock error: {}", e))?;
    db.delete_conversation(&conv_id).map_err(|e| e.to_string())
}

// --- Trace commands ---

#[tauri::command]
pub fn get_traces(
    db: State<Mutex<Database>>,
    conv_id: String,
) -> Result<Vec<TraceRecord>, String> {
    let db = db.lock().map_err(|e| format!("Lock error: {}", e))?;
    db.get_traces(&conv_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_steps(db: State<Mutex<Database>>, msg_id: String) -> Result<Vec<StepRecord>, String> {
    let db = db.lock().map_err(|e| format!("Lock error: {}", e))?;
    db.get_steps(&msg_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_cost_summary(db: State<Mutex<Database>>) -> Result<Vec<CostSummaryRecord>, String> {
    let db = db.lock().map_err(|e| format!("Lock error: {}", e))?;
    db.get_cost_summary().map_err(|e| e.to_string())
}

// --- Bookmark commands ---

#[tauri::command]
pub fn save_bookmark(
    db: State<Mutex<Database>>,
    doc_id: String,
    node_id: String,
    label: String,
) -> Result<(), String> {
    let now = chrono::Utc::now().to_rfc3339();
    let bookmark = crate::db::BookmarkRecord {
        id: uuid::Uuid::new_v4().to_string(),
        doc_id,
        node_id,
        label,
        created_at: now,
    };
    let db = db.lock().map_err(|e| format!("Lock error: {}", e))?;
    db.save_bookmark(&bookmark).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_bookmarks(
    db: State<Mutex<Database>>,
    doc_id: String,
) -> Result<Vec<crate::db::BookmarkRecord>, String> {
    let db = db.lock().map_err(|e| format!("Lock error: {}", e))?;
    db.get_bookmarks(&doc_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_bookmark(db: State<Mutex<Database>>, id: String) -> Result<(), String> {
    let db = db.lock().map_err(|e| format!("Lock error: {}", e))?;
    db.delete_bookmark(&id).map_err(|e| e.to_string())
}

// --- Cancel command ---

#[tauri::command]
pub fn abort_query(cancel_flag: State<CancelFlag>) -> Result<(), String> {
    cancel_flag.0.store(true, Ordering::SeqCst);
    Ok(())
}

// --- File dialog command ---

#[tauri::command]
pub async fn open_file_dialog(app: tauri::AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;

    let file = app
        .dialog()
        .file()
        .add_filter("All Supported", &[
            "md", "markdown", "txt", "text", "log",
            "pdf", "docx",
            "csv", "xlsx", "xls", "ods",
            "rs", "py", "js", "ts", "jsx", "tsx", "go", "java",
            "c", "cpp", "h", "hpp", "cs", "rb", "php", "swift", "kt",
            "sql", "sh", "toml", "yaml", "yml", "json", "xml", "html", "css",
        ])
        .add_filter("Documents", &["pdf", "docx", "md", "markdown", "txt"])
        .add_filter("Spreadsheets", &["csv", "xlsx", "xls", "ods"])
        .add_filter("Code", &[
            "rs", "py", "js", "ts", "jsx", "tsx", "go", "java",
            "c", "cpp", "h", "cs", "rb", "php", "swift", "kt", "sql",
            "sh", "toml", "yaml", "yml", "json", "xml", "html", "css",
        ])
        .add_filter("All Files", &["*"])
        .blocking_pick_file();

    match file {
        Some(path) => Ok(Some(path.to_string())),
        None => Ok(None),
    }
}

// --- Agent chat command ---

#[derive(serde::Serialize, Clone)]
struct ExplorationStepStartEvent {
    /// Correlation ID to match events to a specific chat request
    #[serde(rename = "requestId")]
    request_id: String,
    #[serde(rename = "stepNumber")]
    step_number: u32,
    tool: String,
    #[serde(rename = "inputSummary")]
    input_summary: String,
}

#[derive(serde::Serialize, Clone)]
struct ExplorationStepCompleteEvent {
    #[serde(rename = "requestId")]
    request_id: String,
    #[serde(rename = "stepNumber")]
    step_number: u32,
    #[serde(rename = "outputSummary")]
    output_summary: String,
    #[serde(rename = "tokensUsed")]
    tokens_used: u32,
    #[serde(rename = "latencyMs")]
    latency_ms: u64,
    /// Estimated cost for this step ($ based on input/output token split)
    cost: f64,
    /// Node IDs visited/accessed by this tool call (Feature 4: Live Visualization)
    #[serde(rename = "nodeIds")]
    node_ids: Vec<String>,
}

#[derive(serde::Serialize, Clone)]
struct ChatResponseEvent {
    #[serde(rename = "requestId")]
    request_id: String,
    content: String,
}

#[derive(serde::Serialize, Clone)]
struct ChatTokenEvent {
    #[serde(rename = "requestId")]
    request_id: String,
    token: String,
    done: bool,
}

#[derive(serde::Serialize, Clone)]
struct ChatErrorEvent {
    #[serde(rename = "requestId")]
    request_id: String,
    error: String,
}

/// Emit a response token-by-token for streaming UX.
/// Splits on word boundaries and emits each chunk with a small yield.
async fn emit_streaming_response(app: &tauri::AppHandle, request_id: &str, content: &str) {
    // Split into word-sized chunks for smoother streaming
    let words: Vec<&str> = content.split_inclusive(|c: char| c.is_whitespace() || c == '\n')
        .collect();

    let chunk_size = 3; // Emit ~3 words at a time for natural flow
    for chunk in words.chunks(chunk_size) {
        let token: String = chunk.concat();
        let _ = app.emit("chat-token", ChatTokenEvent { request_id: request_id.to_string(), token, done: false });
        tokio::task::yield_now().await;
    }
    let _ = app.emit("chat-token", ChatTokenEvent { request_id: request_id.to_string(), token: String::new(), done: true });
}

fn create_provider(config: ProviderConfig) -> Result<Box<dyn LLMProvider>, String> {
    let provider_name = config.name.to_lowercase();
    match provider_name.as_str() {
        "ollama" => Ok(Box::new(OllamaProvider::new(config))),
        "groq" => Ok(Box::new(GroqProvider::new(config))),
        "google" => Ok(Box::new(GoogleProvider::new(config))),
        "openrouter" => Ok(Box::new(OpenRouterProvider::new(config))),
        "agentrouter" => Ok(Box::new(AgentRouterProvider::new(config))),
        "anthropic" => Ok(Box::new(AnthropicProvider::new(config))),
        "openai" => Ok(Box::new(OpenAICompatProvider::new(
            config,
            "OpenAI",
            "https://api.openai.com/v1",
        ))),
        "deepseek" => Ok(Box::new(OpenAICompatProvider::new(
            config,
            "DeepSeek",
            "https://api.deepseek.com/v1",
        ))),
        "xai" => Ok(Box::new(OpenAICompatProvider::new(
            config,
            "xAI",
            "https://api.x.ai/v1",
        ))),
        "qwen" => Ok(Box::new(OpenAICompatProvider::new(
            config,
            "Qwen",
            "https://dashscope-intl.aliyuncs.com/compatible-mode/v1",
        ))),
        "openai-compat" => Ok(Box::new(OpenAICompatProvider::new(
            config,
            "Custom",
            "",
        ))),
        _ => Err(format!("Unknown provider: {}", config.name)),
    }
}

fn build_llm_tools(provider_name: &str) -> Vec<Tool> {
    let defs = crate::agent::tools::get_tool_definitions();
    let is_google = provider_name == "google";

    defs.into_iter()
        .map(|td| {
            let parameters = if is_google {
                crate::agent::tools::get_gemini_tool_definitions()
                    .into_iter()
                    .find(|g| g["name"].as_str() == Some(&td.name))
                    .map(|g| g["parameters"].clone())
                    .unwrap_or(td.parameters_schema)
            } else {
                td.parameters_schema
            };
            Tool {
                name: td.name,
                description: td.description,
                parameters,
            }
        })
        .collect()
}

/// Extract node IDs from tool call arguments and results (Feature 4)
fn extract_node_ids(tool_name: &str, params: &HashMap<String, serde_json::Value>, result: &str) -> Vec<String> {
    let mut ids = Vec::new();

    // Extract from params
    if let Some(v) = params.get("node_id").and_then(|v| v.as_str()) {
        ids.push(v.to_string());
    }
    if let Some(v) = params.get("node_a").and_then(|v| v.as_str()) {
        ids.push(v.to_string());
    }
    if let Some(v) = params.get("node_b").and_then(|v| v.as_str()) {
        ids.push(v.to_string());
    }

    // For search results, try to extract matched node IDs from the JSON output
    if tool_name == "search_content" {
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(result) {
            if let Some(matches) = parsed["matches"].as_array() {
                for m in matches.iter().take(10) {
                    if let Some(id) = m["id"].as_str() {
                        ids.push(id.to_string());
                    }
                }
            }
        }
    }

    ids
}

/// Per-model pricing rates ($ per 1M tokens)
#[derive(Deserialize, Clone, Debug)]
struct ModelPricing {
    input: f64,
    output: f64,
}

/// Load the pricing table once from the embedded JSON.
fn pricing_table() -> &'static HashMap<String, ModelPricing> {
    static TABLE: OnceLock<HashMap<String, ModelPricing>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let json_str = include_str!("pricing.json");
        serde_json::from_str(json_str).unwrap_or_default()
    })
}

/// Estimate cost using per-model input/output rates from pricing.json.
/// Falls back to _default rates if model is not found.
fn estimate_cost(model_id: &str, input_tokens: u32, output_tokens: u32) -> f64 {
    let table = pricing_table();
    let rates = table
        .get(model_id)
        .or_else(|| table.get("_default"))
        .cloned()
        .unwrap_or(ModelPricing { input: 0.50, output: 1.50 });
    (input_tokens as f64 / 1_000_000.0) * rates.input
        + (output_tokens as f64 / 1_000_000.0) * rates.output
}

#[tauri::command]
pub async fn chat_with_agent(
    app: tauri::AppHandle,
    db: State<'_, Mutex<Database>>,
    cancel_flag: State<'_, CancelFlag>,
    message: String,
    doc_ids: Vec<String>,
    provider_id: String,
    conv_id: Option<String>,
) -> Result<(), String> {
    // Reset cancel flag at start of new query
    cancel_flag.0.store(false, Ordering::SeqCst);
    let cancel = cancel_flag.0.clone();
    // Generate a unique request correlation ID
    let request_id = uuid::Uuid::new_v4().to_string();

    // Support both single doc_id and multiple doc_ids
    if doc_ids.is_empty() {
        return Err("At least one document must be selected".to_string());
    }

    // Clone data out of the mutex before any async work
    let (trees, provider_config) = {
        let db = db.lock().map_err(|e| format!("Lock error: {}", e))?;

        let mut trees = Vec::new();
        for doc_id in &doc_ids {
            let tree = db
                .get_document(doc_id)
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("Document not found: {}", doc_id))?;
            trees.push(tree);
        }

        let providers = db.get_providers().map_err(|e| e.to_string())?;
        let provider_config = providers
            .into_iter()
            .find(|p| p.id == provider_id)
            .ok_or_else(|| format!("Provider not found: {}", provider_id))?;

        (trees, provider_config)
    };

    // Use the first tree as primary, but make all available for tools
    let primary_tree = &trees[0];

    let provider_name = provider_config.name.to_lowercase();
    let model_id = provider_config.model.clone();
    let provider = create_provider(provider_config)?;

    let processed = preprocess_query(&message);

    // Build combined tree overview for multi-doc queries
    let mut overview_parts = Vec::new();
    for tree in &trees {
        let overview = tree.tree_overview();
        let text = serde_json::to_string_pretty(&overview)
            .unwrap_or_else(|_| "Unable to generate tree overview".to_string());
        if trees.len() > 1 {
            overview_parts.push(format!("=== Document: {} ===\n{}", tree.name, text));
        } else {
            overview_parts.push(text);
        }
    }
    let overview_text = overview_parts.join("\n\n");

    let exploration_hint = if trees.len() > 1 {
        format!(
            "{}\n\nYou have {} documents loaded. You can compare nodes across documents using compare_nodes.",
            processed.exploration_hint,
            trees.len()
        )
    } else {
        processed.exploration_hint.clone()
    };

    let system_prompt = build_system_prompt(&overview_text, &exploration_hint);
    let tools = build_llm_tools(&provider_name);
    let adaptive_max_steps = processed.recommended_max_steps;

    // Use a separate throwaway runtime for pre-computation so it doesn't
    // eat into the agent's actual step budget.
    let mut pre_runtime = AgentRuntime::new(10);

    // Pre-search against the primary tree (only for targeted queries)
    let mut pre_search_results = Vec::new();
    if matches!(processed.intent, QueryIntent::Entity | QueryIntent::Specific | QueryIntent::Factual) {
        for term in processed.search_terms.iter().take(2) {
            let mut params = HashMap::new();
            params.insert("query".to_string(), serde_json::Value::String(term.clone()));
            let input = ToolInput {
                tool: AgentTool::SearchContent,
                params,
            };
            if let Ok(output) = pre_runtime.execute_tool(primary_tree, &input) {
                let result_str = serde_json::to_string(&output.result).unwrap_or_default();
                if result_str.len() > 20 {
                    pre_search_results.push((term.clone(), result_str));
                }
            }
        }
    }

    // Only pre-expand for very small documents (≤3 top-level nodes) to avoid
    // burning tokens on large docs. The agent can expand selectively.
    let tree_overview_summary = primary_tree.tree_overview();
    let mut pre_expand_results = Vec::new();
    if tree_overview_summary.len() <= 3 {
        for summary in &tree_overview_summary {
            let mut params = HashMap::new();
            params.insert("node_id".to_string(), serde_json::Value::String(summary.id.clone()));
            let input = ToolInput {
                tool: AgentTool::ExpandNode,
                params,
            };
            if let Ok(output) = pre_runtime.execute_tool(primary_tree, &input) {
                let result_str = serde_json::to_string(&output.result).unwrap_or_default();
                if result_str.len() > 2000 {
                    pre_expand_results.push(format!("{}... [truncated]", safe_truncate(&result_str, 2000)));
                } else {
                    pre_expand_results.push(result_str);
                }
            }
        }
    }

    // Now create the real runtime with full budget for the agent
    let mut runtime = AgentRuntime::new(adaptive_max_steps);

    // Load conversation history for multi-turn context
    let mut history_messages: Vec<Message> = Vec::new();
    if let Some(ref cid) = conv_id {
        if let Ok(db_guard) = db.lock() {
            if let Ok(records) = db_guard.get_conversation_messages(cid) {
                // Skip the last message if it matches the current user message (just added by frontend)
                let relevant: Vec<_> = records.iter()
                    .filter(|r| !(r.role == "user" && r.content == message))
                    .collect();
                // Include recent history (limit to last 10 messages to avoid token bloat)
                let start = if relevant.len() > 10 { relevant.len() - 10 } else { 0 };
                for r in &relevant[start..] {
                    history_messages.push(Message::text(&r.role, &r.content));
                }
            }
        }
    }

    let mut messages: Vec<Message> = vec![
        Message::text("system", &system_prompt),
    ];
    messages.extend(history_messages);
    messages.push(Message::text("user", &message));

    if !pre_search_results.is_empty() || !pre_expand_results.is_empty() {
        let mut context_parts = Vec::new();
        if !pre_search_results.is_empty() {
            context_parts.push("I've already searched the document for relevant content. Here are the search results:".to_string());
            for (term, result) in &pre_search_results {
                let truncated = if result.len() > 2000 { safe_truncate(result, 2000) } else { result };
                context_parts.push(format!("Search for \"{}\": {}", term, truncated));
            }
        }
        if !pre_expand_results.is_empty() {
            context_parts.push("I've also pre-expanded the document sections. Here is the actual content:".to_string());
            for result in &pre_expand_results {
                context_parts.push(result.clone());
            }
        }
        context_parts.push("Use this information along with additional tool calls to provide a thorough answer.".to_string());
        messages.push(Message::text("system", &context_parts.join("\n\n")));
    }

    let max_steps = adaptive_max_steps;
    let mut total_tokens = 0u32;
    let mut total_input_tokens = 0u32;
    let mut total_output_tokens = 0u32;
    let mut tool_call_counter = 0u32;
    let mut nudge_counter = 0u32;
    let max_nudges = 2u32;
    let min_tool_calls = processed.min_tool_calls;
    let overall_start = tokio::time::Instant::now();
    // Cap message context to prevent unbounded memory growth
    let max_context_messages = 60;

    for _llm_turn in 1..=max_steps {

        // Check if user cancelled
        if cancel.load(Ordering::SeqCst) {
            let _ = app.emit(
                "chat-error",
                ChatErrorEvent {
                    request_id: request_id.clone(),
                    error: "Query cancelled.".to_string(),
                },
            );
            return Ok(());
        }

        // Trim oldest mid-conversation messages if context grows too large,
        // keeping system prompt (first) and recent messages
        if messages.len() > max_context_messages {
            let keep_front = 2; // system + user query
            let keep_back = max_context_messages - keep_front;
            let drain_end = messages.len() - keep_back;
            messages.drain(keep_front..drain_end);
        }

        // Time the LLM call itself — this is the real latency
        let llm_turn_start = tokio::time::Instant::now();

        let llm_response = match provider
            .chat(messages.clone(), Some(tools.clone()))
            .await
        {
            Ok(resp) => resp,
            Err(e) => {
                let _ = app.emit(
                    "chat-error",
                    ChatErrorEvent {
                        request_id: request_id.clone(),
                        error: format!("LLM error: {}", e),
                    },
                );
                return Err(format!("LLM error: {}", e));
            }
        };

        total_tokens += llm_response.tokens_used;
        total_input_tokens += llm_response.input_tokens;
        total_output_tokens += llm_response.output_tokens;

        if llm_response.tool_calls.is_empty() {
            if tool_call_counter < min_tool_calls && _llm_turn < max_steps && nudge_counter < max_nudges {
                nudge_counter += 1;
                let nudge = format!(
                    "You haven't explored the document enough yet ({} tool calls, minimum {} required). \
                     Use expand_node to read the actual content of relevant sections before answering. \
                     The tree overview only shows titles, not content.",
                    tool_call_counter, min_tool_calls
                );
                if let Some(ref partial) = llm_response.content {
                    messages.push(Message::text("assistant", partial));
                }
                messages.push(Message::text("user", &nudge));
                continue;
            }

            let answer = llm_response
                .content
                .unwrap_or_else(|| "I explored the document but couldn't generate a response. Try rephrasing your question or asking about a different aspect of the document.".to_string());

            // Stream tokens for progressive UX
            emit_streaming_response(&app, &request_id, &answer).await;

            // Also emit full response for backwards compatibility
            let _ = app.emit(
                "chat-response",
                ChatResponseEvent {
                    request_id: request_id.clone(),
                    content: answer.clone(),
                },
            );

            runtime.context.compute_relevance_scores();
            let trace_conv_id = conv_id.as_deref().unwrap_or(&doc_ids[0]);
            save_trace_data(&db, trace_conv_id, &model_id, &runtime, total_tokens, total_input_tokens, total_output_tokens, overall_start.elapsed().as_millis() as u64)?;
            return Ok(());
        }

        let assistant_content = llm_response.content.as_deref();
        messages.push(Message::assistant_with_tool_calls(
            assistant_content,
            llm_response.raw_tool_calls.clone(),
        ));

        // LLM call time — distribute evenly across all tool calls in the batch
        let llm_turn_ms = llm_turn_start.elapsed().as_millis() as u64;
        let turn_tool_count = llm_response.tool_calls.len() as u32;
        let tokens_per_tool = if turn_tool_count > 0 { llm_response.tokens_used / turn_tool_count } else { 0 };
        let input_per_tool = if turn_tool_count > 0 { llm_response.input_tokens / turn_tool_count } else { 0 };
        let output_per_tool = if turn_tool_count > 0 { llm_response.output_tokens / turn_tool_count } else { 0 };
        let latency_per_tool = if turn_tool_count > 0 { llm_turn_ms / turn_tool_count as u64 } else { 0 };
        let cost_per_tool = estimate_cost(&model_id, input_per_tool, output_per_tool);

        for tool_call in &llm_response.tool_calls {
            let agent_tool = match AgentTool::from_name(&tool_call.name) {
                Some(t) => t,
                None => {
                    let error_msg = format!("Unknown tool: {}", tool_call.name);
                    messages.push(Message::tool_result(
                        &tool_call.id,
                        &tool_call.name,
                        &error_msg,
                    ));
                    continue;
                }
            };

            let input_summary = match serde_json::to_string(&tool_call.arguments) {
                Ok(s) => {
                    if s.len() > 100 {
                        format!("{}...", safe_truncate(&s, 100))
                    } else {
                        s
                    }
                }
                Err(_) => "{}".to_string(),
            };

            tool_call_counter += 1;
            let current_step = tool_call_counter;

            let _ = app.emit(
                "exploration-step-start",
                ExplorationStepStartEvent {
                    request_id: request_id.clone(),
                    step_number: current_step,
                    tool: tool_call.name.clone(),
                    input_summary: input_summary.clone(),
                },
            );

            let params: HashMap<String, serde_json::Value> =
                match tool_call.arguments.as_object() {
                    Some(obj) => obj.clone().into_iter().collect(),
                    None => HashMap::new(),
                };

            let tool_input = ToolInput {
                tool: agent_tool,
                params: params.clone(),
            };

            let tool_exec_start = tokio::time::Instant::now();

            // For multi-doc: try all trees if the node isn't found in the primary one
            let tool_result = {
                let mut result = None;
                for tree in &trees {
                    match runtime.execute_tool(tree, &tool_input) {
                        Ok(output) => {
                            let result_str = serde_json::to_string(&output.result)
                                .unwrap_or_else(|_| "{}".to_string());
                            if result_str.len() > 4000 {
                                result = Some(format!("{}... [truncated, {} chars total]", safe_truncate(&result_str, 4000), result_str.len()));
                            } else {
                                result = Some(result_str);
                            }
                            break;
                        }
                        Err(_) if trees.len() > 1 => continue,
                        Err(e) => {
                            result = Some(format!("Tool error: {}", e));
                            break;
                        }
                    }
                }
                result.unwrap_or_else(|| "Tool error: node not found in any loaded document".to_string())
            };

            let output_summary = if tool_result.len() > 150 {
                format!("{}...", safe_truncate(&tool_result, 150))
            } else {
                tool_result.clone()
            };

            // Extract node IDs for live visualization (Feature 4)
            let node_ids = extract_node_ids(&tool_call.name, &params, &tool_result);

            // Include both LLM time (distributed) and local tool execution time
            let tool_exec_ms = tool_exec_start.elapsed().as_millis() as u64;
            let _ = app.emit(
                "exploration-step-complete",
                ExplorationStepCompleteEvent {
                    request_id: request_id.clone(),
                    step_number: current_step,
                    output_summary: output_summary.clone(),
                    tokens_used: tokens_per_tool,
                    latency_ms: latency_per_tool + tool_exec_ms,
                    cost: cost_per_tool,
                    node_ids,
                },
            );

            messages.push(Message::tool_result(
                &tool_call.id,
                &tool_call.name,
                &tool_result,
            ));
        }
    }

    messages.push(Message::text(
        "user",
        "You have used all available exploration steps. Based on everything you explored, provide a clear and helpful answer to the user's original question. If the document does not contain the information the user asked about, say so explicitly and share whatever relevant information you did find. Never leave the user without a response.",
    ));

    let final_response = match provider.chat(messages, None).await {
        Ok(resp) => resp,
        Err(e) => {
            let _ = app.emit(
                "chat-error",
                ChatErrorEvent {
                    request_id: request_id.clone(),
                    error: format!("LLM error on final synthesis: {}", e),
                },
            );
            return Err(format!("LLM error on final synthesis: {}", e));
        }
    };

    total_tokens += final_response.tokens_used;
    total_input_tokens += final_response.input_tokens;
    total_output_tokens += final_response.output_tokens;

    let answer = final_response
        .content
        .unwrap_or_else(|| "I explored the document but couldn't find information relevant to your question. The document may not cover this topic — try rephrasing or asking about something within the document's scope.".to_string());

    // Stream tokens for progressive UX
    emit_streaming_response(&app, &request_id, &answer).await;

    let _ = app.emit(
        "chat-response",
        ChatResponseEvent { request_id: request_id.clone(), content: answer },
    );

    runtime.context.compute_relevance_scores();
    let trace_conv_id = conv_id.as_deref().unwrap_or(&doc_ids[0]);
    save_trace_data(&db, trace_conv_id, &model_id, &runtime, total_tokens, total_input_tokens, total_output_tokens, overall_start.elapsed().as_millis() as u64)?;

    Ok(())
}

fn save_trace_data(
    db: &State<'_, Mutex<Database>>,
    conv_id: &str,
    model_id: &str,
    runtime: &AgentRuntime,
    total_tokens: u32,
    input_tokens: u32,
    output_tokens: u32,
    total_latency_ms: u64,
) -> Result<(), String> {
    let db = db.lock().map_err(|e| format!("Lock error: {}", e))?;

    let trace_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    let cost = estimate_cost(model_id, input_tokens, output_tokens);

    let trace = TraceRecord {
        id: trace_id.clone(),
        conv_id: conv_id.to_string(),
        provider_name: model_id.to_string(),
        total_tokens: total_tokens as i64,
        total_cost: cost,
        total_latency_ms: total_latency_ms as i64,
        steps_count: runtime.steps.len() as i64,
        created_at: now,
        input_tokens: input_tokens as i64,
        output_tokens: output_tokens as i64,
    };
    db.save_trace(&trace).map_err(|e| e.to_string())?;

    // Link steps to trace_id so get_steps(trace_id) can retrieve them
    for step in &runtime.steps {
        let step_record = StepRecord {
            id: uuid::Uuid::new_v4().to_string(),
            msg_id: trace_id.clone(),
            tool_name: step.tool.clone(),
            input_json: step.input_summary.clone(),
            output_json: step.output_summary.clone(),
            tokens_used: step.tokens_used as i64,
            latency_ms: step.latency_ms as i64,
        };
        db.save_step(&step_record).map_err(|e| e.to_string())?;
    }

    Ok(())
}
