use rusqlite::{params, Connection};
use serde::Serialize;
use thiserror::Error;

use crate::document::tree::DocumentTree;
use crate::llm::provider::ProviderConfig;

#[derive(Error, Debug)]
pub enum DbError {
    #[error("SQLite error: {0}")]
    SqliteError(#[from] rusqlite::Error),
    #[error("Serialization error: {0}")]
    SerdeError(#[from] serde_json::Error),
    #[error("Not found: {0}")]
    NotFound(String),
}

#[derive(Serialize, Clone, Debug)]
pub struct DocumentSummary {
    pub id: String,
    pub name: String,
    pub doc_type: String,
    pub created_at: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct ConversationRecord {
    pub id: String,
    pub title: String,
    pub doc_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct MessageRecord {
    pub id: String,
    pub conv_id: String,
    pub role: String,
    pub content: String,
    pub created_at: String,
}

pub struct Database {
    pub conn: Connection,
}

impl Database {
    pub fn new(path: &str) -> Result<Self, DbError> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        Ok(Self { conn })
    }

    pub fn initialize(&self) -> Result<(), DbError> {
        self.conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS documents (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                doc_type TEXT NOT NULL,
                file_path TEXT,
                tree_json TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS conversations (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                doc_id TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS messages (
                id TEXT PRIMARY KEY,
                conv_id TEXT NOT NULL,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS exploration_steps (
                id TEXT PRIMARY KEY,
                msg_id TEXT NOT NULL,
                tool_name TEXT NOT NULL,
                input_json TEXT NOT NULL,
                output_json TEXT NOT NULL,
                tokens_used INTEGER NOT NULL DEFAULT 0,
                latency_ms INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS traces (
                id TEXT PRIMARY KEY,
                conv_id TEXT NOT NULL,
                provider_name TEXT NOT NULL DEFAULT '',
                total_tokens INTEGER NOT NULL DEFAULT 0,
                total_cost REAL NOT NULL DEFAULT 0.0,
                total_latency_ms INTEGER NOT NULL DEFAULT 0,
                steps_count INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL,
                input_tokens INTEGER NOT NULL DEFAULT 0,
                output_tokens INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS evals (
                id TEXT PRIMARY KEY,
                trace_id TEXT NOT NULL,
                metric TEXT NOT NULL,
                score REAL NOT NULL,
                details_json TEXT
            );

            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value_json TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS providers (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                api_key_encrypted TEXT,
                base_url TEXT NOT NULL,
                model TEXT NOT NULL,
                is_active INTEGER NOT NULL DEFAULT 0,
                capabilities_json TEXT
            );

            CREATE TABLE IF NOT EXISTS bookmarks (
                id TEXT PRIMARY KEY,
                doc_id TEXT NOT NULL,
                node_id TEXT NOT NULL,
                label TEXT NOT NULL,
                created_at TEXT NOT NULL
            );
            ",
        )?;
        // Migrations for existing databases
        self.run_migrations()?;
        Ok(())
    }

    fn run_migrations(&self) -> Result<(), DbError> {
        // Add input_tokens/output_tokens columns to traces if missing
        let has_input_tokens = self.conn
            .prepare("SELECT input_tokens FROM traces LIMIT 0")
            .is_ok();
        if !has_input_tokens {
            self.conn.execute_batch(
                "ALTER TABLE traces ADD COLUMN input_tokens INTEGER NOT NULL DEFAULT 0;
                 ALTER TABLE traces ADD COLUMN output_tokens INTEGER NOT NULL DEFAULT 0;"
            )?;
        }
        Ok(())
    }

    // --- Document CRUD ---

    pub fn save_document(&self, doc: &DocumentTree, file_path: Option<&str>) -> Result<(), DbError> {
        let tree_json = serde_json::to_string(doc)?;
        let doc_type = format!("{:?}", doc.doc_type);
        self.conn.execute(
            "INSERT OR REPLACE INTO documents (id, name, doc_type, file_path, tree_json, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                doc.id,
                doc.name,
                doc_type,
                file_path,
                tree_json,
                doc.created_at,
                doc.updated_at,
            ],
        )?;
        Ok(())
    }

    pub fn get_document(&self, id: &str) -> Result<Option<DocumentTree>, DbError> {
        let mut stmt = self
            .conn
            .prepare("SELECT tree_json FROM documents WHERE id = ?1")?;
        let result = stmt.query_row(params![id], |row| {
            let json_str: String = row.get(0)?;
            Ok(json_str)
        });

        match result {
            Ok(json_str) => {
                let tree: DocumentTree = serde_json::from_str(&json_str)?;
                Ok(Some(tree))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(DbError::SqliteError(e)),
        }
    }

    pub fn list_documents(&self) -> Result<Vec<DocumentSummary>, DbError> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name, doc_type, created_at FROM documents ORDER BY created_at DESC")?;
        let rows = stmt.query_map([], |row| {
            Ok(DocumentSummary {
                id: row.get(0)?,
                name: row.get(1)?,
                doc_type: row.get(2)?,
                created_at: row.get(3)?,
            })
        })?;

        let mut docs = Vec::new();
        for row in rows {
            docs.push(row?);
        }
        Ok(docs)
    }

    pub fn delete_document(&self, id: &str) -> Result<(), DbError> {
        self.conn
            .execute("DELETE FROM documents WHERE id = ?1", params![id])?;
        Ok(())
    }

    // --- Conversation CRUD ---

    pub fn get_conversation_created_at(&self, conv_id: &str) -> Result<Option<String>, DbError> {
        let mut stmt = self.conn.prepare("SELECT created_at FROM conversations WHERE id = ?1")?;
        let result = stmt.query_row(params![conv_id], |row| row.get(0));
        match result {
            Ok(val) => Ok(Some(val)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(DbError::SqliteError(e)),
        }
    }

    pub fn save_conversation(&self, conv: &ConversationRecord) -> Result<(), DbError> {
        self.conn.execute(
            "INSERT OR REPLACE INTO conversations (id, title, doc_id, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                conv.id,
                conv.title,
                conv.doc_id,
                conv.created_at,
                conv.updated_at,
            ],
        )?;
        Ok(())
    }

    pub fn list_conversations(&self) -> Result<Vec<ConversationRecord>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, title, doc_id, created_at, updated_at FROM conversations ORDER BY updated_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(ConversationRecord {
                id: row.get(0)?,
                title: row.get(1)?,
                doc_id: row.get(2)?,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
            })
        })?;

        let mut convs = Vec::new();
        for row in rows {
            convs.push(row?);
        }
        Ok(convs)
    }

    pub fn get_conversation_messages(&self, conv_id: &str) -> Result<Vec<MessageRecord>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, conv_id, role, content, created_at FROM messages WHERE conv_id = ?1 ORDER BY created_at ASC",
        )?;
        let rows = stmt.query_map(params![conv_id], |row| {
            Ok(MessageRecord {
                id: row.get(0)?,
                conv_id: row.get(1)?,
                role: row.get(2)?,
                content: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?;

        let mut msgs = Vec::new();
        for row in rows {
            msgs.push(row?);
        }
        Ok(msgs)
    }

    pub fn save_message(&self, msg: &MessageRecord) -> Result<(), DbError> {
        self.conn.execute(
            "INSERT OR REPLACE INTO messages (id, conv_id, role, content, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                msg.id,
                msg.conv_id,
                msg.role,
                msg.content,
                msg.created_at,
            ],
        )?;
        Ok(())
    }

    pub fn delete_conversation(&self, conv_id: &str) -> Result<(), DbError> {
        let tx = self.conn.unchecked_transaction()?;
        // Delete exploration steps linked to traces for this conversation
        tx.execute(
            "DELETE FROM exploration_steps WHERE msg_id IN (SELECT id FROM traces WHERE conv_id = ?1)",
            params![conv_id],
        )?;
        // Delete evals linked to traces for this conversation
        tx.execute(
            "DELETE FROM evals WHERE trace_id IN (SELECT id FROM traces WHERE conv_id = ?1)",
            params![conv_id],
        )?;
        tx.execute("DELETE FROM traces WHERE conv_id = ?1", params![conv_id])?;
        tx.execute("DELETE FROM messages WHERE conv_id = ?1", params![conv_id])?;
        tx.execute("DELETE FROM conversations WHERE id = ?1", params![conv_id])?;
        tx.commit()?;
        Ok(())
    }

    pub fn update_conversation_title(&self, conv_id: &str, title: &str) -> Result<(), DbError> {
        let now = chrono::Utc::now().to_rfc3339();
        self.conn.execute(
            "UPDATE conversations SET title = ?1, updated_at = ?2 WHERE id = ?3",
            params![title, now, conv_id],
        )?;
        Ok(())
    }

    // --- Provider CRUD ---

    pub fn save_provider(&self, config: &ProviderConfig) -> Result<(), DbError> {
        self.conn.execute(
            "INSERT OR REPLACE INTO providers (id, name, api_key_encrypted, base_url, model, is_active)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                config.id,
                config.name,
                config.api_key,
                config.base_url,
                config.model,
                config.is_active as i32,
            ],
        )?;
        Ok(())
    }

    pub fn get_providers(&self) -> Result<Vec<ProviderConfig>, DbError> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name, api_key_encrypted, base_url, model, is_active FROM providers")?;
        let rows = stmt.query_map([], |row| {
            let is_active_int: i32 = row.get(5)?;
            Ok(ProviderConfig {
                id: row.get(0)?,
                name: row.get(1)?,
                api_key: row.get(2)?,
                base_url: row.get(3)?,
                model: row.get(4)?,
                is_active: is_active_int != 0,
            })
        })?;

        let mut providers = Vec::new();
        for row in rows {
            providers.push(row?);
        }
        Ok(providers)
    }

    pub fn delete_provider(&self, id: &str) -> Result<(), DbError> {
        self.conn
            .execute("DELETE FROM providers WHERE id = ?1", params![id])?;
        Ok(())
    }

    // --- Settings ---

    pub fn get_setting(&self, key: &str) -> Result<Option<String>, DbError> {
        let mut stmt = self
            .conn
            .prepare("SELECT value_json FROM settings WHERE key = ?1")?;
        let result = stmt.query_row(params![key], |row| {
            let value: String = row.get(0)?;
            Ok(value)
        });

        match result {
            Ok(value) => Ok(Some(value)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(DbError::SqliteError(e)),
        }
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<(), DbError> {
        self.conn.execute(
            "INSERT OR REPLACE INTO settings (key, value_json) VALUES (?1, ?2)",
            params![key, value],
        )?;
        Ok(())
    }

    // --- Bookmarks ---

    pub fn save_bookmark(&self, bookmark: &BookmarkRecord) -> Result<(), DbError> {
        self.conn.execute(
            "INSERT OR REPLACE INTO bookmarks (id, doc_id, node_id, label, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                bookmark.id,
                bookmark.doc_id,
                bookmark.node_id,
                bookmark.label,
                bookmark.created_at,
            ],
        )?;
        Ok(())
    }

    pub fn get_bookmarks(&self, doc_id: &str) -> Result<Vec<BookmarkRecord>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, doc_id, node_id, label, created_at FROM bookmarks WHERE doc_id = ?1 ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map(params![doc_id], |row| {
            Ok(BookmarkRecord {
                id: row.get(0)?,
                doc_id: row.get(1)?,
                node_id: row.get(2)?,
                label: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?;
        let mut bookmarks = Vec::new();
        for row in rows {
            bookmarks.push(row?);
        }
        Ok(bookmarks)
    }

    pub fn delete_bookmark(&self, id: &str) -> Result<(), DbError> {
        self.conn.execute("DELETE FROM bookmarks WHERE id = ?1", params![id])?;
        Ok(())
    }

    // --- Cost summary ---

    pub fn get_cost_summary(&self) -> Result<Vec<CostSummaryRecord>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT provider_name, SUM(total_tokens) as tokens, SUM(total_cost) as cost, COUNT(*) as query_count
             FROM traces WHERE provider_name != '' GROUP BY provider_name ORDER BY tokens DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(CostSummaryRecord {
                provider_name: row.get(0)?,
                total_tokens: row.get(1)?,
                total_cost: row.get(2)?,
                query_count: row.get(3)?,
            })
        })?;

        let mut summaries = Vec::new();
        for row in rows {
            summaries.push(row?);
        }
        Ok(summaries)
    }
}

#[derive(Serialize, Clone, Debug)]
pub struct CostSummaryRecord {
    pub provider_name: String,
    pub total_tokens: i64,
    pub total_cost: f64,
    pub query_count: i64,
}

#[derive(Serialize, Clone, Debug)]
pub struct BookmarkRecord {
    pub id: String,
    pub doc_id: String,
    pub node_id: String,
    pub label: String,
    pub created_at: String,
}
