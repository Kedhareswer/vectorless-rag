pub mod document;
pub mod agent;
pub mod llm;
pub mod db;
pub mod commands;

use std::sync::Mutex;
use db::Database;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialize the database
    let db = Database::new("vectorless-rag.db").expect("Failed to open database");
    db.initialize().expect("Failed to initialize database schema");

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(Mutex::new(db))
        .invoke_handler(tauri::generate_handler![
            commands::list_documents,
            commands::get_document,
            commands::ingest_document,
            commands::delete_document,
            commands::get_tree_overview,
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
            commands::chat_with_agent,
            commands::open_file_dialog,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
