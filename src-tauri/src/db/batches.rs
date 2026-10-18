//! Question batches sent through a provider's Batch API.
//!
//! A batch job is one submission (many questions over a chat's documents); its
//! items are the individual questions and, once the provider has finished, their
//! answers. Tables are created by the V3→V4 migration in `schema.rs`.

use rusqlite::{OptionalExtension, params};
use serde::Serialize;

use super::schema::{Database, DbError};

#[derive(Serialize, Clone, Debug)]
pub struct BatchJobRecord {
    pub id: String,
    pub conv_id: Option<String>,
    pub provider_id: String,
    /// The provider's own id for the batch (e.g. Anthropic's `msgbatch_...`).
    pub provider_batch_id: String,
    /// `in_progress`, `canceling` or `ended`, as last reported by the provider.
    pub status: String,
    pub created_at: String,
    pub ended_at: Option<String>,
    /// Set once results have been downloaded and stored, so they are fetched once.
    pub results_fetched: bool,
}

#[derive(Serialize, Clone, Debug)]
pub struct BatchItemRecord {
    pub job_id: String,
    pub custom_id: String,
    pub question: String,
    pub answer: Option<String>,
    pub error: Option<String>,
    pub input_tokens: i64,
    pub output_tokens: i64,
}

fn job_from_row(row: &rusqlite::Row) -> rusqlite::Result<BatchJobRecord> {
    Ok(BatchJobRecord {
        id: row.get(0)?,
        conv_id: row.get(1)?,
        provider_id: row.get(2)?,
        provider_batch_id: row.get(3)?,
        status: row.get(4)?,
        created_at: row.get(5)?,
        ended_at: row.get(6)?,
        results_fetched: row.get::<_, i64>(7)? != 0,
    })
}

const JOB_COLUMNS: &str =
    "id, conv_id, provider_id, provider_batch_id, status, created_at, ended_at, results_fetched";

impl Database {
    /// Store a newly submitted job and its questions in one transaction.
    pub fn create_batch_job(
        &self,
        job: &BatchJobRecord,
        items: &[BatchItemRecord],
    ) -> Result<(), DbError> {
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO batch_jobs (id, conv_id, provider_id, provider_batch_id, status, created_at, ended_at, results_fetched)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                job.id,
                job.conv_id,
                job.provider_id,
                job.provider_batch_id,
                job.status,
                job.created_at,
                job.ended_at,
                job.results_fetched as i64,
            ],
        )?;
        for item in items {
            tx.execute(
                "INSERT INTO batch_items (job_id, custom_id, question, answer, error, input_tokens, output_tokens)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    item.job_id,
                    item.custom_id,
                    item.question,
                    item.answer,
                    item.error,
                    item.input_tokens,
                    item.output_tokens,
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn get_batch_job(&self, id: &str) -> Result<Option<BatchJobRecord>, DbError> {
        let conn = self.conn()?;
        Ok(conn
            .query_row(
                &format!("SELECT {JOB_COLUMNS} FROM batch_jobs WHERE id = ?1"),
                params![id],
                job_from_row,
            )
            .optional()?)
    }

    /// Jobs for one conversation, newest first.
    pub fn list_batch_jobs(&self, conv_id: &str) -> Result<Vec<BatchJobRecord>, DbError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(&format!(
            "SELECT {JOB_COLUMNS} FROM batch_jobs WHERE conv_id = ?1 ORDER BY created_at DESC"
        ))?;
        let rows = stmt.query_map(params![conv_id], job_from_row)?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn list_batch_items(&self, job_id: &str) -> Result<Vec<BatchItemRecord>, DbError> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT job_id, custom_id, question, answer, error, input_tokens, output_tokens
             FROM batch_items WHERE job_id = ?1 ORDER BY rowid",
        )?;
        let rows = stmt.query_map(params![job_id], |row| {
            Ok(BatchItemRecord {
                job_id: row.get(0)?,
                custom_id: row.get(1)?,
                question: row.get(2)?,
                answer: row.get(3)?,
                error: row.get(4)?,
                input_tokens: row.get(5)?,
                output_tokens: row.get(6)?,
            })
        })?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn update_batch_job_status(
        &self,
        id: &str,
        status: &str,
        ended_at: Option<&str>,
    ) -> Result<(), DbError> {
        let conn = self.conn()?;
        conn.execute(
            "UPDATE batch_jobs SET status = ?2, ended_at = COALESCE(?3, ended_at) WHERE id = ?1",
            params![id, status, ended_at],
        )?;
        Ok(())
    }

    /// Store every item's result and mark the job's results as fetched.
    pub fn save_batch_results(&self, job_id: &str, items: &[BatchItemRecord]) -> Result<(), DbError> {
        let mut conn = self.conn()?;
        let tx = conn.transaction()?;
        for item in items {
            tx.execute(
                "UPDATE batch_items SET answer = ?3, error = ?4, input_tokens = ?5, output_tokens = ?6
                 WHERE job_id = ?1 AND custom_id = ?2",
                params![
                    job_id,
                    item.custom_id,
                    item.answer,
                    item.error,
                    item.input_tokens,
                    item.output_tokens,
                ],
            )?;
        }
        tx.execute(
            "UPDATE batch_jobs SET results_fetched = 1 WHERE id = ?1",
            params![job_id],
        )?;
        tx.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> (tempfile::TempDir, Database) {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(dir.path().join("t.db").to_str().unwrap()).unwrap();
        db.initialize().unwrap();
        (dir, db)
    }

    fn job(id: &str, conv: &str, created_at: &str) -> BatchJobRecord {
        BatchJobRecord {
            id: id.to_string(),
            conv_id: Some(conv.to_string()),
            provider_id: "p1".to_string(),
            provider_batch_id: format!("msgbatch_{id}"),
            status: "in_progress".to_string(),
            created_at: created_at.to_string(),
            ended_at: None,
            results_fetched: false,
        }
    }

    fn item(job_id: &str, custom_id: &str, question: &str) -> BatchItemRecord {
        BatchItemRecord {
            job_id: job_id.to_string(),
            custom_id: custom_id.to_string(),
            question: question.to_string(),
            answer: None,
            error: None,
            input_tokens: 0,
            output_tokens: 0,
        }
    }

    #[test]
    fn batch_job_round_trip_and_results() {
        let (_dir, db) = db();
        db.create_batch_job(
            &job("j1", "c1", "2026-10-18T10:00:00Z"),
            &[item("j1", "q-0", "What is the revenue?"), item("j1", "q-1", "Who signed?")],
        )
        .unwrap();

        let items = db.list_batch_items("j1").unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].question, "What is the revenue?");
        assert!(items[0].answer.is_none());

        db.update_batch_job_status("j1", "ended", Some("2026-10-18T10:05:00Z")).unwrap();
        let mut done = item("j1", "q-0", "");
        done.answer = Some("$1.2M".to_string());
        done.input_tokens = 900;
        let mut failed = item("j1", "q-1", "");
        failed.error = Some("prompt is too long".to_string());
        db.save_batch_results("j1", &[done, failed]).unwrap();

        let job = db.get_batch_job("j1").unwrap().unwrap();
        assert_eq!(job.status, "ended");
        assert!(job.results_fetched);
        let items = db.list_batch_items("j1").unwrap();
        assert_eq!(items[0].answer.as_deref(), Some("$1.2M"));
        assert_eq!(items[0].question, "What is the revenue?"); // question kept
        assert_eq!(items[1].error.as_deref(), Some("prompt is too long"));
    }

    #[test]
    fn jobs_are_listed_per_conversation_newest_first() {
        let (_dir, db) = db();
        db.create_batch_job(&job("old", "c1", "2026-10-01T00:00:00Z"), &[]).unwrap();
        db.create_batch_job(&job("new", "c1", "2026-10-18T00:00:00Z"), &[]).unwrap();
        db.create_batch_job(&job("other", "c2", "2026-10-18T00:00:00Z"), &[]).unwrap();
        let ids: Vec<String> = db.list_batch_jobs("c1").unwrap().into_iter().map(|j| j.id).collect();
        assert_eq!(ids, vec!["new", "old"]);
        assert!(db.get_batch_job("missing").unwrap().is_none());
    }
}
