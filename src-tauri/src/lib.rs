pub mod document;
pub mod agent;
pub mod llm;
pub mod db;
pub mod commands;
pub mod util;
pub mod validation;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use db::Database;
use commands::CancelFlags;
use document::cache::TreeCache;

/// Resolve the database path inside the platform app data directory.
/// Falls back to current directory if unavailable.
fn resolve_db_path() -> String {
    if let Some(data_dir) = app_data_dir() {
        if std::fs::create_dir_all(&data_dir).is_ok() {
            let path = data_dir.join("vectorless-rag.db");
            return path.to_string_lossy().to_string();
        }
    }
    "vectorless-rag.db".to_string()
}

/// Get the platform-specific app data directory.
fn app_data_dir() -> Option<std::path::PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var("APPDATA")
            .ok()
            .map(|p| std::path::PathBuf::from(p).join("vectorless-rag"))
    }
    #[cfg(target_os = "macos")]
    {
        std::env::var("HOME")
            .ok()
            .map(|p| std::path::PathBuf::from(p).join("Library/Application Support/vectorless-rag"))
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        std::env::var("XDG_DATA_HOME")
            .ok()
            .or_else(|| std::env::var("HOME").ok().map(|h| format!("{}/.local/share", h)))
            .map(|p| std::path::PathBuf::from(p).join("vectorless-rag"))
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialize the database in the platform app data directory
    let db_path = resolve_db_path();
    let db = Database::new(&db_path).expect("Failed to open database");
    db.initialize().expect("Failed to initialize database schema");

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(db)
        .manage(Mutex::new(TreeCache::new(32)))
        .manage(CancelFlags(Arc::new(Mutex::new(HashMap::new()))))
        .invoke_handler(tauri::generate_handler![
            commands::list_documents,
            commands::get_document,
            commands::ingest_document,
            commands::delete_document,
            commands::get_tree_overview,
            commands::get_rich_overview,
            commands::expand_node,
            commands::search_document,
            commands::get_providers,
            commands::save_provider,
            commands::delete_provider,
            commands::get_setting,
            commands::set_setting,
            commands::get_traces,
            commands::get_steps,
            commands::get_cost_summary,
            commands::list_conversations,
            commands::save_conversation,
            commands::get_conversation_messages,
            commands::save_message,
            commands::delete_conversation,
            commands::add_doc_to_conversation,
            commands::remove_doc_from_conversation,
            commands::get_conversation_doc_ids,
            commands::chat_with_agent,
            commands::abort_query,
            commands::open_file_dialog,
            commands::save_bookmark,
            commands::get_bookmarks,
            commands::delete_bookmark,
            commands::get_cross_doc_relations,
            commands::get_model_options,
            commands::check_local_model,
            commands::download_local_model,
            commands::delete_local_model,
            commands::describe_image,
            commands::reenrich_document,
            commands::clear_app_data,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
