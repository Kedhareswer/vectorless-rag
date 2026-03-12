use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::document::tree::DocumentTree;
use crate::llm::provider::ProviderConfig;

const KEYCHAIN_SERVICE: &str = "vectorless-rag";
const KEYCHAIN_PLACEHOLDER: &str = "__keychain__";

fn keychain_set(provider_id: &str, api_key: &str) -> Result<(), keyring::Error> {
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, provider_id)?;
    entry.set_password(api_key)
}

fn keychain_get(provider_id: &str) -> Result<String, keyring::Error> {
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, provider_id)?;
    entry.get_password()
}

fn keychain_delete(provider_id: &str) -> Result<(), keyring::Error> {
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, provider_id)?;
    entry.delete_credential()
}

#[derive(Error, Debug)]
pub enum DbError {
    #[error("SQLite error: {0}")]
    SqliteError(#[from] rusqlite::Error),
    #[error("Serialization error: {0}")]
    SerdeError(#[from] serde_json::Error),
    #[error("Pool error: {0}")]
    PoolError(#[from] r2d2::Error),
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

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CrossDocRelation {
    pub id: String,
    pub source_doc_id: String,
    pub source_node_id: String,
    pub target_doc_id: String,
    pub target_node_id: String,
    pub relation_type: String,
    pub confidence: f64,
    pub description: Option<String>,
    pub created_at: String,
}

pub struct Database {
    pool: Pool<SqliteConnectionManager>,
}

impl Database {
    pub fn new(path: &str) -> Result<Self, DbError> {
        let manager = SqliteConnectionManager::file(path)
            .with_init(|conn| {
                conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
                Ok(())
            });
        let pool = Pool::builder()
            .max_size(8)
            .build(manager)?;
        Ok(Self { pool })
    }

    /// Get a connection from the pool.
    pub(crate) fn conn(&self) -> Result<r2d2::PooledConnection<SqliteConnectionManager>, DbError> {
        Ok(self.pool.get()?)
    }

    pub fn initialize(&self) -> Result<(), DbError> {
        let conn = self.conn()?;
        conn.execute_batch(
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

            CREATE TABLE IF NOT EXISTS cross_doc_relations (
                id TEXT PRIMARY KEY,
                source_doc_id TEXT NOT NULL,
                source_node_id TEXT NOT NULL,
                target_doc_id TEXT NOT NULL,
                target_node_id TEXT NOT NULL,
                relation_type TEXT NOT NULL,
                confidence REAL NOT NULL DEFAULT 1.0,
                description TEXT,
                created_at TEXT NOT NULL
            );
            ",
        )?;
        // Run versioned migrations
        self.run_migrations()?;
        Ok(())
    }

    /// Current schema version. Bump this when adding a new migration.
    const LATEST_VERSION: i64 = 3;

    fn run_migrations(&self) -> Result<(), DbError> {
        let conn = self.conn()?;

        // Create version tracking table if it doesn't exist
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_version (
                version INTEGER NOT NULL
            );"
        )?;

        let current: i64 = conn
            .query_row("SELECT COALESCE(MAX(version), 0) FROM schema_version", [], |r| r.get(0))
            .unwrap_or(0);

        // Dispatch table: index 0 = migrate_v0_to_v1, index 1 = migrate_v1_to_v2, etc.
        let migrations: Vec<fn(&Self) -> Result<(), DbError>> = vec![
            Self::migrate_v0_to_v1,
            Self::migrate_v1_to_v2,
            Self::migrate_v2_to_v3,
        ];
        assert!(migrations.len() as i64 == Self::LATEST_VERSION,
            "Migration count ({}) does not match LATEST_VERSION ({})", migrations.len(), Self::LATEST_VERSION);

        for (i, migrate) in migrations.iter().enumerate() {
            let version = (i + 1) as i64;
            if current < version {
                migrate(self)?;
            }
        }

        Ok(())
    }

    /// V0→V1: Add input_tokens/output_tokens columns to traces.
    /// (Previously an ad-hoc ALTER TABLE check.)
    fn migrate_v0_to_v1(&self) -> Result<(), DbError> {
        let conn = self.conn()?;
        let has_input_tokens = conn
            .prepare("SELECT input_tokens FROM traces LIMIT 0")
            .is_ok();
        if !has_input_tokens {
            conn.execute_batch(
                "ALTER TABLE traces ADD COLUMN input_tokens INTEGER NOT NULL DEFAULT 0;
                 ALTER TABLE traces ADD COLUMN output_tokens INTEGER NOT NULL DEFAULT 0;"
            )?;
        }
        conn.execute(
            "INSERT OR REPLACE INTO schema_version (version) VALUES (?1)",
            params![1i64],
        )?;
        Ok(())
    }

    /// V1→V2: Add indexes for common query patterns.
    fn migrate_v1_to_v2(&self) -> Result<(), DbError> {
        let conn = self.conn()?;
        conn.execute_batch(
            "CREATE INDEX IF NOT EXISTS idx_messages_conv_id ON messages(conv_id);
             CREATE INDEX IF NOT EXISTS idx_traces_conv_id ON traces(conv_id);
             CREATE INDEX IF NOT EXISTS idx_exploration_steps_msg_id ON exploration_steps(msg_id);
             CREATE INDEX IF NOT EXISTS idx_bookmarks_doc_id ON bookmarks(doc_id);
             CREATE INDEX IF NOT EXISTS idx_cross_doc_relations_source ON cross_doc_relations(source_doc_id);
             CREATE INDEX IF NOT EXISTS idx_cross_doc_relations_target ON cross_doc_relations(target_doc_id);"
        )?;
        conn.execute(
            "INSERT OR REPLACE INTO schema_version (version) VALUES (?1)",
            params![2i64],
        )?;
        Ok(())
    }

    /// V2→V3: Add conversation_documents join table (documents scoped per chat).
    /// Migrates existing conversations.doc_id into the new table.
    fn migrate_v2_to_v3(&self) -> Result<(), DbError> {
        let conn = self.conn()?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS conversation_documents (
                conv_id TEXT NOT NULL,
                doc_id TEXT NOT NULL,
                added_at TEXT NOT NULL,
                PRIMARY KEY (conv_id, doc_id)
            );
            CREATE INDEX IF NOT EXISTS idx_conv_docs_conv ON conversation_documents(conv_id);
            CREATE INDEX IF NOT EXISTS idx_conv_docs_doc ON conversation_documents(doc_id);"
        )?;

        // Migrate existing conversations.doc_id → conversation_documents
        let mut stmt = conn.prepare(
            "SELECT id, doc_id FROM conversations WHERE doc_id IS NOT NULL AND doc_id != ''"
        )?;
        let rows: Vec<(String, String)> = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?.filter_map(|r| r.ok()).collect();
        for (conv_id, doc_id) in rows {
            let now = chrono::Utc::now().to_rfc3339();
            conn.execute(
                "INSERT OR IGNORE INTO conversation_documents (conv_id, doc_id, added_at) VALUES (?1, ?2, ?3)",
                params![conv_id, doc_id, now],
            )?;
        }

        conn.execute(
            "INSERT OR REPLACE INTO schema_version (version) VALUES (?1)",
            params![3i64],
        )?;
        Ok(())
    }

    // --- Document CRUD ---

    pub fn save_document(&self, doc: &DocumentTree, file_path: Option<&str>) -> Result<(), DbError> {
        let tree_json = serde_json::to_string(doc)?;
        let doc_type = format!("{:?}", doc.doc_type);
        let conn = self.conn()?;
        conn.execute(
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
        let conn = self.conn()?;
        let mut stmt = conn
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
        let conn = self.conn()?;
        let mut stmt = conn
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
        let conn = self.conn()?;
        conn.execute("DELETE FROM documents WHERE id = ?1", params![id])?;
        Ok(())
    }

    // --- Conversation CRUD ---

    pub fn get_conversation_created_at(&self, conv_id: &str) -> Result<Option<String>, DbError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare("SELECT created_at FROM conversations WHERE id = ?1")?;
        let result = stmt.query_row(params![conv_id], |row| row.get(0));
        match result {
            Ok(val) => Ok(Some(val)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(DbError::SqliteError(e)),
        }
    }

    pub fn save_conversation(&self, conv: &ConversationRecord) -> Result<(), DbError> {
        let conn = self.conn()?;
        conn.execute(
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
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
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
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
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
        let conn = self.conn()?;
        conn.execute(
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
        let conn = self.conn()?;
        let tx = conn.unchecked_transaction()?;
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
        tx.execute("DELETE FROM conversation_documents WHERE conv_id = ?1", params![conv_id])?;
        tx.execute("DELETE FROM conversations WHERE id = ?1", params![conv_id])?;
        tx.commit()?;
        Ok(())
    }

    // --- Conversation-Document associations ---

    pub fn add_doc_to_conversation(&self, conv_id: &str, doc_id: &str) -> Result<(), DbError> {
        let now = chrono::Utc::now().to_rfc3339();
        let conn = self.conn()?;
        conn.execute(
            "INSERT OR IGNORE INTO conversation_documents (conv_id, doc_id, added_at) VALUES (?1, ?2, ?3)",
            params![conv_id, doc_id, now],
        )?;
        Ok(())
    }

    pub fn remove_doc_from_conversation(&self, conv_id: &str, doc_id: &str) -> Result<(), DbError> {
        let conn = self.conn()?;
        conn.execute(
            "DELETE FROM conversation_documents WHERE conv_id = ?1 AND doc_id = ?2",
            params![conv_id, doc_id],
        )?;
        Ok(())
    }

    pub fn get_conversation_doc_ids(&self, conv_id: &str) -> Result<Vec<String>, DbError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT doc_id FROM conversation_documents WHERE conv_id = ?1 ORDER BY added_at ASC"
        )?;
        let rows = stmt.query_map(params![conv_id], |row| row.get(0))?;
        let mut ids = Vec::new();
        for row in rows {
            ids.push(row?);
        }
        Ok(ids)
    }

    pub fn update_conversation_title(&self, conv_id: &str, title: &str) -> Result<(), DbError> {
        let now = chrono::Utc::now().to_rfc3339();
        let conn = self.conn()?;
        conn.execute(
            "UPDATE conversations SET title = ?1, updated_at = ?2 WHERE id = ?3",
            params![title, now, conv_id],
        )?;
        Ok(())
    }

    // --- Provider CRUD ---

    pub fn save_provider(&self, config: &ProviderConfig) -> Result<(), DbError> {
        // Store API key in OS keychain, keep a placeholder in SQLite
        let db_key_value = match &config.api_key {
            Some(key) if !key.is_empty() => {
                match keychain_set(&config.id, key) {
                    Ok(()) => Some(KEYCHAIN_PLACEHOLDER.to_string()),
                    Err(_) => Some(key.clone()), // fallback to plaintext if keychain unavailable
                }
            }
            other => other.clone(),
        };

        let conn = self.conn()?;
        conn.execute(
            "INSERT OR REPLACE INTO providers (id, name, api_key_encrypted, base_url, model, is_active)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                config.id,
                config.name,
                db_key_value,
                config.base_url,
                config.model,
                config.is_active as i32,
            ],
        )?;
        Ok(())
    }

    pub fn get_providers(&self) -> Result<Vec<ProviderConfig>, DbError> {
        let conn = self.conn()?;
        let mut stmt = conn
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
            let mut config = row?;
            match &config.api_key {
                Some(key) if key == KEYCHAIN_PLACEHOLDER => {
                    // Retrieve from OS keychain
                    config.api_key = Some(keychain_get(&config.id).unwrap_or_default());
                }
                Some(key) if !key.is_empty() => {
                    // Legacy plaintext key — migrate to keychain
                    if keychain_set(&config.id, key).is_ok() {
                        let _ = conn.execute(
                            "UPDATE providers SET api_key_encrypted = ?1 WHERE id = ?2",
                            params![KEYCHAIN_PLACEHOLDER, config.id],
                        );
                    }
                }
                _ => {}
            }
            providers.push(config);
        }
        Ok(providers)
    }

    pub fn delete_provider(&self, id: &str) -> Result<(), DbError> {
        // Remove API key from OS keychain
        let _ = keychain_delete(id);
        let conn = self.conn()?;
        conn.execute("DELETE FROM providers WHERE id = ?1", params![id])?;
        Ok(())
    }

    // --- Settings ---

    pub fn get_setting(&self, key: &str) -> Result<Option<String>, DbError> {
        let conn = self.conn()?;
        let mut stmt = conn
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
        let conn = self.conn()?;
        conn.execute(
            "INSERT OR REPLACE INTO settings (key, value_json) VALUES (?1, ?2)",
            params![key, value],
        )?;
        Ok(())
    }

    // --- Bookmarks ---

    pub fn save_bookmark(&self, bookmark: &BookmarkRecord) -> Result<(), DbError> {
        let conn = self.conn()?;
        conn.execute(
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
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
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
        let conn = self.conn()?;
        conn.execute("DELETE FROM bookmarks WHERE id = ?1", params![id])?;
        Ok(())
    }

    // --- Cross-document relations ---

    pub fn save_cross_doc_relation(&self, rel: &CrossDocRelation) -> Result<(), DbError> {
        let conn = self.conn()?;
        conn.execute(
            "INSERT OR REPLACE INTO cross_doc_relations (id, source_doc_id, source_node_id, target_doc_id, target_node_id, relation_type, confidence, description, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                rel.id,
                rel.source_doc_id,
                rel.source_node_id,
                rel.target_doc_id,
                rel.target_node_id,
                rel.relation_type,
                rel.confidence,
                rel.description,
                rel.created_at,
            ],
        )?;
        Ok(())
    }

    /// Get all cross-doc relations involving a given node (from either side).
    pub fn get_cross_doc_relations_for_node(&self, node_id: &str) -> Result<Vec<CrossDocRelation>, DbError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, source_doc_id, source_node_id, target_doc_id, target_node_id, relation_type, confidence, description, created_at
             FROM cross_doc_relations WHERE source_node_id = ?1 OR target_node_id = ?1"
        )?;
        let rows = stmt.query_map(params![node_id], |row| {
            Ok(CrossDocRelation {
                id: row.get(0)?,
                source_doc_id: row.get(1)?,
                source_node_id: row.get(2)?,
                target_doc_id: row.get(3)?,
                target_node_id: row.get(4)?,
                relation_type: row.get(5)?,
                confidence: row.get(6)?,
                description: row.get(7)?,
                created_at: row.get(8)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.into())
    }

    /// Get all cross-doc relations between two documents.
    pub fn get_cross_doc_relations_between(&self, doc_a: &str, doc_b: &str) -> Result<Vec<CrossDocRelation>, DbError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, source_doc_id, source_node_id, target_doc_id, target_node_id, relation_type, confidence, description, created_at
             FROM cross_doc_relations
             WHERE (source_doc_id = ?1 AND target_doc_id = ?2) OR (source_doc_id = ?2 AND target_doc_id = ?1)"
        )?;
        let rows = stmt.query_map(params![doc_a, doc_b], |row| {
            Ok(CrossDocRelation {
                id: row.get(0)?,
                source_doc_id: row.get(1)?,
                source_node_id: row.get(2)?,
                target_doc_id: row.get(3)?,
                target_node_id: row.get(4)?,
                relation_type: row.get(5)?,
                confidence: row.get(6)?,
                description: row.get(7)?,
                created_at: row.get(8)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.into())
    }

    pub fn delete_cross_doc_relation(&self, id: &str) -> Result<(), DbError> {
        let conn = self.conn()?;
        conn.execute("DELETE FROM cross_doc_relations WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Get all cross-doc relations involving any of the given document IDs.
    pub fn get_cross_doc_relations_for_docs(&self, doc_ids: &[String]) -> Result<Vec<CrossDocRelation>, DbError> {
        if doc_ids.is_empty() {
            return Ok(Vec::new());
        }
        // Build a dynamic IN clause
        let placeholders: Vec<String> = (1..=doc_ids.len()).map(|i| format!("?{}", i)).collect();
        let in_clause = placeholders.join(", ");
        let sql = format!(
            "SELECT id, source_doc_id, source_node_id, target_doc_id, target_node_id, relation_type, confidence, description, created_at
             FROM cross_doc_relations
             WHERE source_doc_id IN ({}) OR target_doc_id IN ({})",
            in_clause, in_clause
        );
        let conn = self.conn()?;
        let mut stmt = conn.prepare(&sql)?;
        // Bind each doc_id twice (once for source, once for target)
        let params: Vec<&dyn rusqlite::types::ToSql> = doc_ids.iter()
            .chain(doc_ids.iter())
            .map(|s| s as &dyn rusqlite::types::ToSql)
            .collect();
        let rows = stmt.query_map(params.as_slice(), |row| {
            Ok(CrossDocRelation {
                id: row.get(0)?,
                source_doc_id: row.get(1)?,
                source_node_id: row.get(2)?,
                target_doc_id: row.get(3)?,
                target_node_id: row.get(4)?,
                relation_type: row.get(5)?,
                confidence: row.get(6)?,
                description: row.get(7)?,
                created_at: row.get(8)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(DbError::from)
    }

    // --- Cost summary ---

    pub fn get_cost_summary(&self) -> Result<Vec<CostSummaryRecord>, DbError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::tree::{DocType, DocumentTree, NodeType, TreeNode};

    fn test_db() -> (Database, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let path = dir.path().join("test.db");
        let db = Database::new(path.to_str().unwrap()).expect("failed to create test DB");
        db.initialize().expect("failed to initialize DB");
        (db, dir) // dir must be kept alive so the temp directory isn't deleted
    }

    fn sample_tree() -> DocumentTree {
        let mut tree = DocumentTree::new("test-doc".to_string(), DocType::Markdown);
        let child = TreeNode::new(NodeType::Section, "Section 1 content".to_string());
        let root_id = tree.root_id.clone();
        tree.add_node(&root_id, child).unwrap();
        tree
    }

    #[test]
    fn test_initialize_no_error() {
        let (_db, _dir) = test_db();
    }

    #[test]
    fn test_save_and_get_document_roundtrip() {
        let (db, _dir) = test_db();
        let tree = sample_tree();
        let doc_id = tree.id.clone();

        db.save_document(&tree, Some("/tmp/test.md")).unwrap();
        let loaded = db.get_document(&doc_id).unwrap().expect("document should exist");

        assert_eq!(loaded.id, tree.id);
        assert_eq!(loaded.name, tree.name);
        assert_eq!(loaded.doc_type, tree.doc_type);
        assert_eq!(loaded.root_id, tree.root_id);
        assert_eq!(loaded.nodes.len(), tree.nodes.len());
        // Verify child node content survived the roundtrip
        for (id, node) in &tree.nodes {
            let loaded_node = loaded.nodes.get(id).expect("node should exist");
            assert_eq!(loaded_node.content, node.content);
            assert_eq!(loaded_node.node_type, node.node_type);
        }
    }

    #[test]
    fn test_get_document_not_found() {
        let (db, _dir) = test_db();
        let result = db.get_document("nonexistent-id").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_list_documents() {
        let (db, _dir) = test_db();
        let tree1 = sample_tree();
        let tree2 = DocumentTree::new("second-doc".to_string(), DocType::Pdf);

        db.save_document(&tree1, None).unwrap();
        db.save_document(&tree2, None).unwrap();

        let docs = db.list_documents().unwrap();
        assert_eq!(docs.len(), 2);
        let names: Vec<&str> = docs.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"test-doc"));
        assert!(names.contains(&"second-doc"));
    }

    #[test]
    fn test_delete_document() {
        let (db, _dir) = test_db();
        let tree = sample_tree();
        let doc_id = tree.id.clone();

        db.save_document(&tree, None).unwrap();
        assert!(db.get_document(&doc_id).unwrap().is_some());

        db.delete_document(&doc_id).unwrap();
        assert!(db.get_document(&doc_id).unwrap().is_none());
    }

    #[test]
    fn test_save_and_list_conversations() {
        let (db, _dir) = test_db();
        let conv = ConversationRecord {
            id: "conv-1".to_string(),
            title: "Test Conversation".to_string(),
            doc_id: Some("doc-1".to_string()),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        };

        db.save_conversation(&conv).unwrap();

        let convs = db.list_conversations().unwrap();
        assert_eq!(convs.len(), 1);
        assert_eq!(convs[0].id, "conv-1");
        assert_eq!(convs[0].title, "Test Conversation");
        assert_eq!(convs[0].doc_id, Some("doc-1".to_string()));
    }

    #[test]
    fn test_save_message_and_get_conversation_messages() {
        let (db, _dir) = test_db();
        let conv = ConversationRecord {
            id: "conv-msg".to_string(),
            title: "Msg Test".to_string(),
            doc_id: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        };
        db.save_conversation(&conv).unwrap();

        let msg1 = MessageRecord {
            id: "msg-1".to_string(),
            conv_id: "conv-msg".to_string(),
            role: "user".to_string(),
            content: "Hello".to_string(),
            created_at: "2026-01-01T00:00:01Z".to_string(),
        };
        let msg2 = MessageRecord {
            id: "msg-2".to_string(),
            conv_id: "conv-msg".to_string(),
            role: "assistant".to_string(),
            content: "Hi there".to_string(),
            created_at: "2026-01-01T00:00:02Z".to_string(),
        };

        db.save_message(&msg1).unwrap();
        db.save_message(&msg2).unwrap();

        let msgs = db.get_conversation_messages("conv-msg").unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].id, "msg-1");
        assert_eq!(msgs[0].role, "user");
        assert_eq!(msgs[0].content, "Hello");
        assert_eq!(msgs[1].id, "msg-2");
        assert_eq!(msgs[1].role, "assistant");
        assert_eq!(msgs[1].content, "Hi there");
    }

    #[test]
    fn test_get_conversation_messages_empty() {
        let (db, _dir) = test_db();
        let msgs = db.get_conversation_messages("nonexistent").unwrap();
        assert!(msgs.is_empty());
    }

    #[test]
    fn test_delete_conversation_also_deletes_messages() {
        let (db, _dir) = test_db();
        let conv = ConversationRecord {
            id: "conv-del".to_string(),
            title: "To Delete".to_string(),
            doc_id: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        };
        db.save_conversation(&conv).unwrap();

        let msg = MessageRecord {
            id: "msg-del".to_string(),
            conv_id: "conv-del".to_string(),
            role: "user".to_string(),
            content: "will be deleted".to_string(),
            created_at: "2026-01-01T00:00:01Z".to_string(),
        };
        db.save_message(&msg).unwrap();

        db.delete_conversation("conv-del").unwrap();

        let convs = db.list_conversations().unwrap();
        assert!(convs.is_empty());

        let msgs = db.get_conversation_messages("conv-del").unwrap();
        assert!(msgs.is_empty());
    }

    #[test]
    fn test_get_setting_missing_key_returns_none() {
        let (db, _dir) = test_db();
        let result = db.get_setting("nonexistent-key").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_set_and_get_setting_roundtrip() {
        let (db, _dir) = test_db();
        db.set_setting("theme", "dark").unwrap();

        let val = db.get_setting("theme").unwrap().expect("setting should exist");
        assert_eq!(val, "dark");
    }

    #[test]
    fn test_set_setting_overwrites() {
        let (db, _dir) = test_db();
        db.set_setting("theme", "light").unwrap();
        db.set_setting("theme", "dark").unwrap();

        let val = db.get_setting("theme").unwrap().expect("setting should exist");
        assert_eq!(val, "dark");
    }

    #[test]
    fn test_migrations_run_to_latest() {
        let (db, _dir) = test_db();
        let conn = db.conn().unwrap();
        let version: i64 = conn
            .query_row("SELECT MAX(version) FROM schema_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, Database::LATEST_VERSION);
    }

    #[test]
    fn test_migrations_idempotent() {
        let (db, _dir) = test_db();
        // Running initialize again should not fail
        db.initialize().unwrap();
        let conn = db.conn().unwrap();
        let version: i64 = conn
            .query_row("SELECT MAX(version) FROM schema_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, Database::LATEST_VERSION);
    }

    #[test]
    fn test_v2_indexes_exist() {
        let (db, _dir) = test_db();
        let conn = db.conn().unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name LIKE 'idx_%'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(count >= 6, "expected at least 6 indexes, got {}", count);
    }
}
