use rusqlite::params;
use serde::Serialize;

use super::schema::{Database, DbError};

#[derive(Serialize, Clone, Debug)]
pub struct TraceRecord {
    pub id: String,
    pub conv_id: String,
    pub provider_name: String,
    pub total_tokens: i64,
    pub total_cost: f64,
    pub total_latency_ms: i64,
    pub steps_count: i64,
    pub created_at: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct StepRecord {
    pub id: String,
    pub msg_id: String,
    pub tool_name: String,
    pub input_json: String,
    pub output_json: String,
    pub tokens_used: i64,
    pub latency_ms: i64,
}

#[derive(Serialize, Clone, Debug)]
pub struct EvalRecord {
    pub id: String,
    pub trace_id: String,
    pub metric: String,
    pub score: f64,
    pub details_json: Option<String>,
}

impl Database {
    pub fn save_trace(&self, trace: &TraceRecord) -> Result<(), DbError> {
        self.conn.execute(
            "INSERT OR REPLACE INTO traces (id, conv_id, provider_name, total_tokens, total_cost, total_latency_ms, steps_count, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                trace.id,
                trace.conv_id,
                trace.provider_name,
                trace.total_tokens,
                trace.total_cost,
                trace.total_latency_ms,
                trace.steps_count,
                trace.created_at,
            ],
        )?;
        Ok(())
    }

    pub fn get_traces(&self, conv_id: &str) -> Result<Vec<TraceRecord>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, conv_id, COALESCE(provider_name, '') as provider_name, total_tokens, total_cost, total_latency_ms, steps_count, created_at
             FROM traces WHERE conv_id = ?1 ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map(params![conv_id], |row| {
            Ok(TraceRecord {
                id: row.get(0)?,
                conv_id: row.get(1)?,
                provider_name: row.get(2)?,
                total_tokens: row.get(3)?,
                total_cost: row.get(4)?,
                total_latency_ms: row.get(5)?,
                steps_count: row.get(6)?,
                created_at: row.get(7)?,
            })
        })?;

        let mut traces = Vec::new();
        for row in rows {
            traces.push(row?);
        }
        Ok(traces)
    }

    pub fn save_step(&self, step: &StepRecord) -> Result<(), DbError> {
        self.conn.execute(
            "INSERT OR REPLACE INTO exploration_steps (id, msg_id, tool_name, input_json, output_json, tokens_used, latency_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                step.id,
                step.msg_id,
                step.tool_name,
                step.input_json,
                step.output_json,
                step.tokens_used,
                step.latency_ms,
            ],
        )?;
        Ok(())
    }

    pub fn get_steps(&self, msg_id: &str) -> Result<Vec<StepRecord>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, msg_id, tool_name, input_json, output_json, tokens_used, latency_ms
             FROM exploration_steps WHERE msg_id = ?1",
        )?;
        let rows = stmt.query_map(params![msg_id], |row| {
            Ok(StepRecord {
                id: row.get(0)?,
                msg_id: row.get(1)?,
                tool_name: row.get(2)?,
                input_json: row.get(3)?,
                output_json: row.get(4)?,
                tokens_used: row.get(5)?,
                latency_ms: row.get(6)?,
            })
        })?;

        let mut steps = Vec::new();
        for row in rows {
            steps.push(row?);
        }
        Ok(steps)
    }

    pub fn save_eval(&self, eval: &EvalRecord) -> Result<(), DbError> {
        self.conn.execute(
            "INSERT OR REPLACE INTO evals (id, trace_id, metric, score, details_json)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                eval.id,
                eval.trace_id,
                eval.metric,
                eval.score,
                eval.details_json,
            ],
        )?;
        Ok(())
    }

    pub fn get_evals(&self, trace_id: &str) -> Result<Vec<EvalRecord>, DbError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, trace_id, metric, score, details_json
             FROM evals WHERE trace_id = ?1",
        )?;
        let rows = stmt.query_map(params![trace_id], |row| {
            Ok(EvalRecord {
                id: row.get(0)?,
                trace_id: row.get(1)?,
                metric: row.get(2)?,
                score: row.get(3)?,
                details_json: row.get(4)?,
            })
        })?;

        let mut evals = Vec::new();
        for row in rows {
            evals.push(row?);
        }
        Ok(evals)
    }
}
