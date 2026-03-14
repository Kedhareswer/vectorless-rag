use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::ipc::Channel;
use tauri::{Manager, State};

use crate::agent::events::ChatEvent;
use crate::agent::run_agent_chat;
use keyring;
use crate::db::{CrossDocRelation, Database, DocumentSummary, ConversationRecord, MessageRecord, CostSummaryRecord, StepRecord, TraceRecord};
use crate::document::cache::TreeCache;
use crate::document::metadata::enrich_tree_metadata;
use crate::document::parser::get_parser_for_file;
use crate::document::tree::{DocumentTree, RichNodeSummary, TreeNode, TreeNodeSummary};
use crate::document::image::extract_images_from_path;
use crate::llm::local::{self, DownloadProgress, LocalModelStatus, ModelOption};
use crate::llm::provider::ProviderConfig;
use crate::validation;

/// Per-request cancellation flags keyed by request ID.
/// Each active query registers its own flag; `abort_query` sets matching flags.
pub struct CancelFlags(pub Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>);

/// Load a document tree from cache, falling back to DB on miss.
fn get_tree_cached(
    db: &State<Database>,
    cache: &State<Mutex<TreeCache>>,
    doc_id: &str,
) -> Result<DocumentTree, String> {
    {
        let cache_guard = cache.lock().map_err(|e| format!("Lock error: {}", e))?;
        if let Some(tree) = cache_guard.get(doc_id) {
            return Ok(tree.clone());
        }
    }
    let tree = db
        .get_document(doc_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Document not found: {}", doc_id))?;
    let mut cache_guard = cache.lock().map_err(|e| format!("Lock error: {}", e))?;
    cache_guard.insert(doc_id.to_string(), tree.clone());
    Ok(tree)
}

// --- Document commands ---

#[tauri::command]
pub fn list_documents(db: State<Database>) -> Result<Vec<DocumentSummary>, String> {
    db.list_documents().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_document(
    db: State<Database>,
    cache: State<Mutex<TreeCache>>,
    id: String,
) -> Result<DocumentTree, String> {
    get_tree_cached(&db, &cache, &id)
}

#[tauri::command]
pub fn ingest_document(
    app: tauri::AppHandle,
    db: State<Database>,
    cache: State<Mutex<TreeCache>>,
    file_path: String,
) -> Result<DocumentTree, String> {
    validation::validate_file_path(&file_path)?;
    let parser = get_parser_for_file(&file_path);
    let mut tree = parser.parse(&file_path).map_err(|e| e.to_string())?;

    // Try to start the llama-server sidecar for LLM-powered enrichment.
    // Non-fatal: if the sidecar binary isn't present or model isn't downloaded,
    // enrichment silently falls back to heuristic extraction.
    if let Ok(app_data) = app.path().app_data_dir() {
        let status = local::check_local_model(&app_data, &db);
        if let Some(model_path) = status.model_path {
            let _ = local::start_sidecar(&app_data, &model_path);
        }
    }

    // Enrich nodes with metadata (LLM-generated if model loaded, else heuristic)
    enrich_tree_metadata(&mut tree);

    // Extract embedded images (PDF and DOCX)
    let images = extract_images_from_path(&file_path, &tree.id);
    // Attach image nodes to the tree root so they appear in the structure
    if !images.is_empty() {
        use crate::document::tree::{NodeType, TreeNode};
        let root_id = tree.root_id.clone();
        for img in &images {
            let mut node = TreeNode::new(NodeType::Image, String::new());
            node.id = img.id.clone();
            node.metadata.insert("path".to_string(), serde_json::json!(img.path));
            node.metadata.insert("mime_type".to_string(), serde_json::json!(img.mime_type));
            if let Some((w, h)) = img.dimensions {
                node.metadata.insert("width".to_string(), serde_json::json!(w));
                node.metadata.insert("height".to_string(), serde_json::json!(h));
            }
            if let Some(desc) = &img.description {
                node.summary = Some(desc.clone());
            }
            let _ = tree.add_node(&root_id, node);
        }
    }

    db.save_document(&tree, Some(&file_path))
        .map_err(|e| e.to_string())?;
    // Populate cache with the enriched tree
    let mut cache_guard = cache.lock().map_err(|e| format!("Lock error: {}", e))?;
    cache_guard.insert(tree.id.clone(), tree.clone());
    Ok(tree)
}

#[tauri::command]
pub fn delete_document(
    db: State<Database>,
    cache: State<Mutex<TreeCache>>,
    id: String,
) -> Result<(), String> {
    db.delete_document(&id).map_err(|e| e.to_string())?;
    // Invalidate cache entry
    let mut cache_guard = cache.lock().map_err(|e| format!("Lock error: {}", e))?;
    cache_guard.invalidate(&id);
    Ok(())
}

// --- Tree exploration commands ---

#[tauri::command]
pub fn get_tree_overview(
    db: State<Database>,
    cache: State<Mutex<TreeCache>>,
    doc_id: String,
) -> Result<Vec<TreeNodeSummary>, String> {
    let tree = get_tree_cached(&db, &cache, &doc_id)?;
    Ok(tree.tree_overview())
}

#[tauri::command]
pub fn get_rich_overview(
    db: State<Database>,
    cache: State<Mutex<TreeCache>>,
    doc_id: String,
) -> Result<Vec<RichNodeSummary>, String> {
    let tree = get_tree_cached(&db, &cache, &doc_id)?;
    Ok(tree.rich_overview())
}

#[tauri::command]
pub fn expand_node(
    db: State<Database>,
    cache: State<Mutex<TreeCache>>,
    doc_id: String,
    node_id: String,
) -> Result<TreeNode, String> {
    let tree = get_tree_cached(&db, &cache, &doc_id)?;
    tree.get_node(&node_id)
        .cloned()
        .ok_or_else(|| format!("Node not found: {}", node_id))
}

#[tauri::command]
pub fn search_document(
    db: State<Database>,
    cache: State<Mutex<TreeCache>>,
    doc_id: String,
    query: String,
) -> Result<Vec<TreeNode>, String> {
    let tree = get_tree_cached(&db, &cache, &doc_id)?;

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
pub fn get_providers(db: State<Database>) -> Result<Vec<ProviderConfig>, String> {
    db.get_providers().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_provider(
    db: State<Database>,
    config: ProviderConfig,
) -> Result<(), String> {
    validation::validate_provider(&config)?;
    db.save_provider(&config).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_provider(db: State<Database>, id: String) -> Result<(), String> {
    db.delete_provider(&id).map_err(|e| e.to_string())
}

// --- Settings commands ---

#[tauri::command]
pub fn get_setting(db: State<Database>, key: String) -> Result<Option<String>, String> {
    db.get_setting(&key).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_setting(
    db: State<Database>,
    key: String,
    value: String,
) -> Result<(), String> {
    db.set_setting(&key, &value).map_err(|e| e.to_string())
}

// --- Conversation commands ---

#[tauri::command]
pub fn list_conversations(db: State<Database>) -> Result<Vec<ConversationRecord>, String> {
    db.list_conversations().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_conversation(
    db: State<Database>,
    id: String,
    title: String,
    doc_id: Option<String>,
) -> Result<(), String> {
    let now = chrono::Utc::now().to_rfc3339();
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
    db: State<Database>,
    conv_id: String,
) -> Result<Vec<MessageRecord>, String> {
    db.get_conversation_messages(&conv_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_message(
    db: State<Database>,
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
    db.save_message(&msg).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_conversation(db: State<Database>, conv_id: String) -> Result<(), String> {
    db.delete_conversation(&conv_id).map_err(|e| e.to_string())
}

// --- Conversation-Document association commands ---

#[tauri::command]
pub fn add_doc_to_conversation(
    db: State<Database>,
    conv_id: String,
    doc_id: String,
) -> Result<(), String> {
    db.add_doc_to_conversation(&conv_id, &doc_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn remove_doc_from_conversation(
    db: State<Database>,
    conv_id: String,
    doc_id: String,
) -> Result<(), String> {
    db.remove_doc_from_conversation(&conv_id, &doc_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_conversation_doc_ids(
    db: State<Database>,
    conv_id: String,
) -> Result<Vec<String>, String> {
    db.get_conversation_doc_ids(&conv_id).map_err(|e| e.to_string())
}

// --- Trace commands ---

#[tauri::command]
pub fn get_traces(
    db: State<Database>,
    conv_id: String,
) -> Result<Vec<TraceRecord>, String> {
    db.get_traces(&conv_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_steps(db: State<Database>, msg_id: String) -> Result<Vec<StepRecord>, String> {
    db.get_steps(&msg_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_cost_summary(db: State<Database>) -> Result<Vec<CostSummaryRecord>, String> {
    db.get_cost_summary().map_err(|e| e.to_string())
}

// --- Bookmark commands ---

#[tauri::command]
pub fn save_bookmark(
    db: State<Database>,
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
    db.save_bookmark(&bookmark).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_bookmarks(
    db: State<Database>,
    doc_id: String,
) -> Result<Vec<crate::db::BookmarkRecord>, String> {
    db.get_bookmarks(&doc_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_bookmark(db: State<Database>, id: String) -> Result<(), String> {
    db.delete_bookmark(&id).map_err(|e| e.to_string())
}

// --- Cross-doc relations command ---

#[tauri::command]
pub fn get_cross_doc_relations(
    db: State<Database>,
    doc_ids: Vec<String>,
) -> Result<Vec<CrossDocRelation>, String> {
    db.get_cross_doc_relations_for_docs(&doc_ids).map_err(|e| e.to_string())
}

// --- Local model commands ---

#[tauri::command]
pub fn get_model_options() -> Vec<ModelOption> {
    local::get_model_options()
}

#[tauri::command]
pub fn check_local_model(
    app: tauri::AppHandle,
    db: State<Database>,
) -> Result<LocalModelStatus, String> {
    let app_data = app.path().app_data_dir()
        .map_err(|e| format!("Failed to get app data dir: {}", e))?;
    Ok(local::check_local_model(&app_data, &db))
}

#[tauri::command]
pub async fn download_local_model(
    app: tauri::AppHandle,
    db: State<'_, Database>,
    on_progress: Channel<DownloadProgress>,
    model_id: String,
) -> Result<(), String> {
    let app_data = app.path().app_data_dir()
        .map_err(|e| format!("Failed to get app data dir: {}", e))?;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<DownloadProgress>();

    // Forward progress to Tauri channel
    let channel_clone = on_progress.clone();
    let forwarder = tokio::spawn(async move {
        while let Some(progress) = rx.recv().await {
            let _ = channel_clone.send(progress);
        }
    });

    let dest = local::download_model(&app_data, &model_id, tx).await?;

    let _ = forwarder.await;

    // Save model info to settings
    db.set_setting("local_model_id", &model_id)
        .map_err(|e| e.to_string())?;
    db.set_setting("local_model_path", &dest.to_string_lossy())
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub fn delete_local_model(
    app: tauri::AppHandle,
    db: State<Database>,
) -> Result<(), String> {
    let app_data = app.path().app_data_dir()
        .map_err(|e| format!("Failed to get app data dir: {}", e))?;
    local::delete_local_model(&app_data, &db)
}

// --- Re-enrich command ---

/// Re-run metadata enrichment on an already-ingested document.
/// Uses the local sidecar model if running, otherwise falls back to heuristic extraction.
/// Clears existing summaries so they are regenerated, then saves the updated tree.
#[tauri::command]
pub fn reenrich_document(
    app: tauri::AppHandle,
    db: State<Database>,
    cache: State<Mutex<TreeCache>>,
    doc_id: String,
) -> Result<(), String> {
    let mut tree = db
        .get_document(&doc_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Document not found: {}", doc_id))?;

    // Try to start the sidecar if a model is available
    if let Ok(app_data) = app.path().app_data_dir() {
        let status = local::check_local_model(&app_data, &db);
        if let Some(model_path) = status.model_path {
            let _ = local::start_sidecar(&app_data, &model_path);
        }
    }

    // Clear existing summaries on top-level nodes so they are regenerated
    let root_id = tree.root_id.clone();
    let child_ids: Vec<String> = tree
        .get_node(&root_id)
        .map(|r| r.children.clone())
        .unwrap_or_default();
    for child_id in &child_ids {
        if let Some(node) = tree.nodes.get_mut(child_id) {
            node.summary = None;
            node.metadata.remove("entities");
            node.metadata.remove("topics");
        }
    }

    enrich_tree_metadata(&mut tree);

    db.save_document(&tree, None).map_err(|e| e.to_string())?;

    // Invalidate cache so next load gets fresh tree
    let mut cache_guard = cache.lock().map_err(|e| format!("Lock error: {}", e))?;
    cache_guard.invalidate(&doc_id);
    cache_guard.insert(doc_id, tree);

    Ok(())
}

// --- Image description command ---

/// Describe an image node using a vision-capable LLM provider.
/// `image_path` is the local filesystem path to the extracted image file.
/// `provider_config` is the active provider config serialised as JSON.
#[tauri::command]
pub async fn describe_image(
    db: State<'_, Database>,
    doc_id: String,
    node_id: String,
    image_path: String,
) -> Result<String, String> {
    use crate::llm::provider::Message;
    use crate::agent::chat_handler::create_provider;

    validation::validate_file_path(&image_path)?;

    // Read image bytes and base64-encode
    let bytes = std::fs::read(&image_path)
        .map_err(|e| format!("Failed to read image: {}", e))?;
    let b64 = base64_encode(&bytes);

    let ext = std::path::Path::new(&image_path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("jpeg")
        .to_lowercase();
    let media_type = if ext == "png" { "image/png" } else { "image/jpeg" };

    // Get the active provider
    let configs = db.get_providers().map_err(|e| e.to_string())?;
    let active_id = db.get_setting("active_provider")
        .map_err(|e| e.to_string())?
        .unwrap_or_default();
    let config = configs
        .into_iter()
        .find(|c| c.id == active_id)
        .ok_or("No active provider configured")?;

    let provider = create_provider(config)?;

    // Build a vision message. OpenAI-compatible format: content as array with
    // image_url object. Providers that support vision (GPT-4o, Gemini) handle this.
    let vision_content = serde_json::json!([
        {
            "type": "image_url",
            "image_url": {
                "url": format!("data:{};base64,{}", media_type, b64)
            }
        },
        {
            "type": "text",
            "text": "Describe what is shown in this image in 1-3 sentences. Focus on the content relevant to a document (charts, diagrams, tables, figures). Be specific about data or labels visible."
        }
    ]);

    let messages = vec![
        Message::text("system", "You are a document analysis assistant. Describe images concisely and accurately."),
        Message {
            role: "user".to_string(),
            content: vision_content.to_string(),
            tool_calls_raw: None,
            tool_call_id: None,
            tool_name: None,
        },
    ];

    let response = provider.chat(messages, None).await
        .map_err(|e| format!("Vision LLM error: {}", e))?;

    let description = response.content
        .unwrap_or_default()
        .trim()
        .to_string();

    // Persist the description into the document tree
    if let Ok(Some(mut tree)) = db.get_document(&doc_id) {
        if let Some(node) = tree.nodes.get_mut(&node_id) {
            node.summary = Some(description.clone());
            node.metadata.insert("described".to_string(), serde_json::json!(true));
        }
        let _ = db.save_document(&tree, None);
    }

    Ok(description)
}

/// Simple base64 encoder — avoids pulling in a separate crate.
fn base64_encode(input: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(CHARS[((n >> 18) & 63) as usize] as char);
        out.push(CHARS[((n >> 12) & 63) as usize] as char);
        if chunk.len() > 1 { out.push(CHARS[((n >> 6) & 63) as usize] as char); } else { out.push('='); }
        if chunk.len() > 2 { out.push(CHARS[(n & 63) as usize] as char); } else { out.push('='); }
    }
    out
}

// --- Cancel command ---

#[tauri::command]
pub fn abort_query(
    cancel_flags: State<CancelFlags>,
    request_id: Option<String>,
) -> Result<(), String> {
    let flags = cancel_flags.0.lock().map_err(|e| format!("Lock error: {}", e))?;
    match request_id {
        Some(id) => {
            // Cancel a specific request
            if let Some(flag) = flags.get(&id) {
                flag.store(true, Ordering::SeqCst);
            }
        }
        None => {
            // Cancel ALL active requests
            for flag in flags.values() {
                flag.store(true, Ordering::SeqCst);
            }
        }
    }
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

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn chat_with_agent(
    db: State<'_, Database>,
    cache: State<'_, Mutex<TreeCache>>,
    cancel_flags: State<'_, CancelFlags>,
    on_event: Channel<ChatEvent>,
    message: String,
    doc_ids: Vec<String>,
    provider_id: String,
    conv_id: Option<String>,
) -> Result<(), String> {
    validation::validate_chat_input(&message, &doc_ids, &provider_id)?;

    // Create a per-request cancel flag
    let request_id = uuid::Uuid::new_v4().to_string();
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut flags = cancel_flags.0.lock().map_err(|e| format!("Lock error: {}", e))?;
        flags.insert(request_id.clone(), cancel.clone());
    }

    // Extract data from Tauri state, using cache where possible
    let (trees, provider_config) = {
        let mut trees = Vec::new();
        {
            let mut cache_guard = cache.lock().map_err(|e| format!("Lock error: {}", e))?;
            for doc_id in &doc_ids {
                if let Some(tree) = cache_guard.get(doc_id) {
                    trees.push(tree.clone());
                } else {
                    let tree = db
                        .get_document(doc_id)
                        .map_err(|e| e.to_string())?
                        .ok_or_else(|| format!("Document not found: {}", doc_id))?;
                    cache_guard.insert(doc_id.clone(), tree.clone());
                    trees.push(tree);
                }
            }
        }

        let providers = db.get_providers().map_err(|e| e.to_string())?;
        let provider_config = providers
            .into_iter()
            .find(|p| p.id == provider_id)
            .ok_or_else(|| format!("Provider not found: {}", provider_id))?;

        (trees, provider_config)
    };

    // Delegate to the agent chat handler
    let result = run_agent_chat(
        &on_event,
        &db,
        cancel,
        message,
        trees,
        provider_config,
        conv_id,
        doc_ids,
    )
    .await;

    // Clean up the cancel flag for this request
    if let Ok(mut flags) = cancel_flags.0.lock() {
        flags.remove(&request_id);
    }

    result
}

// --- Clear app data command ---

/// Wipe ALL app data: database, local model file, and keychain entries.
/// This is equivalent to uninstall + reinstall without needing to find hidden folders.
/// After this call the app is in a clean first-run state (the DB will be re-created on
/// next launch). The frontend should reload / clear its store state after calling this.
#[tauri::command]
pub fn clear_app_data(
    app: tauri::AppHandle,
    db: State<Database>,
) -> Result<(), String> {
    // Stop sidecar before touching its model file
    local::stop_sidecar();

    // Delete the local model file and clear its settings
    if let Ok(app_data) = app.path().app_data_dir() {
        let _ = local::delete_local_model(&app_data, &db);

        // Also wipe the llama-server binary on full reset (unlike "Remove model" which keeps it)
        let bdir = local::bin_dir(&app_data);
        if bdir.exists() {
            if let Ok(entries) = std::fs::read_dir(&bdir) {
                for entry in entries.flatten() {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }

        // Delete the SQLite database file(s) — the .db, .db-wal, .db-shm
        for suffix in &["vectorless-rag.db", "vectorless-rag.db-wal", "vectorless-rag.db-shm"] {
            let p = app_data.join(suffix);
            if p.exists() {
                let _ = std::fs::remove_file(&p);
            }
        }
    }

    // Clear OS keychain entries for all known providers
    let provider_ids: Vec<String> = db
        .get_providers()
        .unwrap_or_default()
        .into_iter()
        .map(|p| p.id)
        .collect();
    for id in &provider_ids {
        let _ = keyring::Entry::new("vectorless-rag", id)
            .and_then(|e| e.delete_credential());
    }

    Ok(())
}
