//! Question batches through a provider's Batch API (Anthropic Message Batches).
//!
//! Each question runs the same deterministic pipeline as an interactive chat —
//! heuristic preprocess, local-SLM enrichment when it applies, tree fetch, the
//! same system prompt with its guardrails and cache breakpoint — but instead of
//! one streaming call per question, all final calls go to the provider in one
//! asynchronous batch at half the price. No tools, no loop: one answer call per
//! question, exactly as in `run_agent_chat`.
//!
//! Batched questions are answered independently: no conversation history is
//! sent, so each answer stands on the documents alone.

use std::sync::atomic::AtomicBool;

use crate::db::{BatchItemRecord, BatchJobRecord, Database};
use crate::document::tree::DocumentTree;
use crate::llm::anthropic::{AnthropicProvider, BatchRequest};
use crate::llm::provider::{Message, ProviderConfig};

use super::chat_handler::{build_system_prompt, push_document_content, should_enrich};
use super::deterministic::{fetch_content, format_for_prompt};
use super::query::{extract_terms_from_text, generate_hyde, preprocess_query, rewrite_query, stepback_query};

/// Most questions accepted in one batch. Enrichment runs locally before the
/// batch is submitted (up to three short SLM generations per question), so a
/// cap keeps submission time bounded; the API itself allows far more.
pub const MAX_BATCH_QUESTIONS: usize = 50;

/// Providers whose Batch API is implemented.
pub fn supports_batches(provider_name: &str) -> bool {
    provider_name.eq_ignore_ascii_case("anthropic")
}

/// The answer-step messages for one question: what `run_agent_chat` would send,
/// minus conversation history.
fn answer_messages(question: &str, trees: &[DocumentTree], relations_context: &str) -> Vec<Message> {
    let mut processed = preprocess_query(question);

    // Same enrichment as the interactive pipeline (Phase 3). Failures are
    // non-fatal: the question is fetched with the terms it already has.
    if should_enrich(&processed) {
        let cancel = AtomicBool::new(false);
        for enrich in [rewrite_query, generate_hyde, stepback_query] {
            if let Ok(result) = enrich(question, &cancel) {
                for term in extract_terms_from_text(&result.text) {
                    if !processed.search_terms.contains(&term) {
                        processed.search_terms.push(term);
                    }
                }
            }
        }
    }

    let fetched = fetch_content(&processed, trees);
    let content_text = format_for_prompt(&fetched);
    let doc_label = if trees.len() > 1 {
        format!("{} documents", trees.len())
    } else {
        trees.first().map(|t| t.name.clone()).unwrap_or_else(|| "document".to_string())
    };

    let mut messages = vec![Message::text("system", &build_system_prompt(&doc_label, relations_context))];
    push_document_content(&mut messages, &content_text);
    messages.push(Message::text("user", question));
    messages
}

/// Known cross-document relations, formatted as in the interactive prompt.
fn relations_context(db: &Database, doc_ids: &[String]) -> String {
    if doc_ids.len() < 2 {
        return String::new();
    }
    let known = db.get_cross_doc_relations_for_docs(doc_ids).unwrap_or_default();
    if known.is_empty() {
        return String::new();
    }
    let lines: Vec<String> = known
        .iter()
        .take(15)
        .map(|r| format!("- {}: {}", r.relation_type, r.description.as_deref().unwrap_or("")))
        .collect();
    format!("\n\nKnown relations between these documents:\n{}", lines.join("\n"))
}

/// Clean, de-duplicated questions; errors if none are left or too many.
pub fn normalize_questions(questions: &[String]) -> Result<Vec<String>, String> {
    let mut out: Vec<String> = Vec::new();
    for q in questions {
        let q = q.trim();
        if !q.is_empty() && !out.iter().any(|seen| seen == q) {
            out.push(q.to_string());
        }
    }
    if out.is_empty() {
        return Err("Add at least one question.".to_string());
    }
    if out.len() > MAX_BATCH_QUESTIONS {
        return Err(format!(
            "A batch holds at most {} questions ({} given).",
            MAX_BATCH_QUESTIONS,
            out.len()
        ));
    }
    Ok(out)
}

/// Build every question's prompt, submit the batch and record the job.
///
/// Prompt building (local enrichment and tree fetch) is CPU-bound, so callers
/// run this off the async runtime's worker threads where possible; the network
/// call is awaited normally.
pub async fn submit_batch(
    db: &Database,
    provider_config: ProviderConfig,
    conv_id: Option<String>,
    doc_ids: &[String],
    trees: Vec<DocumentTree>,
    questions: Vec<String>,
) -> Result<(BatchJobRecord, Vec<BatchItemRecord>), String> {
    if !supports_batches(&provider_config.name) {
        return Err(format!(
            "Batch questions need an Anthropic provider; \"{}\" has no Batch API support here.",
            provider_config.name
        ));
    }
    let questions = normalize_questions(&questions)?;
    let relations = relations_context(db, doc_ids);

    let prompt_questions = questions.clone();
    let requests: Vec<BatchRequest> = tokio::task::spawn_blocking(move || {
        prompt_questions
            .iter()
            .enumerate()
            .map(|(i, q)| BatchRequest {
                custom_id: format!("q-{i}"),
                messages: answer_messages(q, &trees, &relations),
            })
            .collect()
    })
    .await
    .map_err(|e| format!("Preparing batch failed: {}", e))?;

    let provider_id = provider_config.id.clone();
    let provider = AnthropicProvider::new(provider_config);
    let status = provider
        .create_batch(requests)
        .await
        .map_err(|e| e.to_string())?;

    let job = BatchJobRecord {
        id: uuid::Uuid::new_v4().to_string(),
        conv_id,
        provider_id,
        provider_batch_id: status.id,
        status: status.processing_status,
        created_at: chrono::Utc::now().to_rfc3339(),
        ended_at: None,
        results_fetched: false,
    };
    let items: Vec<BatchItemRecord> = questions
        .into_iter()
        .enumerate()
        .map(|(i, question)| BatchItemRecord {
            job_id: job.id.clone(),
            custom_id: format!("q-{i}"),
            question,
            answer: None,
            error: None,
            input_tokens: 0,
            output_tokens: 0,
        })
        .collect();
    db.create_batch_job(&job, &items).map_err(|e| e.to_string())?;
    Ok((job, items))
}

/// Ask the provider how a job is doing; once it has ended, download and store
/// the answers (only the first time). Returns the job and its items.
pub async fn refresh_batch(
    db: &Database,
    provider_config: ProviderConfig,
    job_id: &str,
) -> Result<(BatchJobRecord, Vec<BatchItemRecord>), String> {
    let job = db
        .get_batch_job(job_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Batch not found: {}", job_id))?;

    if !job.results_fetched {
        let provider = AnthropicProvider::new(provider_config);
        let status = provider
            .get_batch(&job.provider_batch_id)
            .await
            .map_err(|e| e.to_string())?;
        let ended = status.processing_status == "ended";
        let ended_at = ended.then(|| chrono::Utc::now().to_rfc3339());
        db.update_batch_job_status(job_id, &status.processing_status, ended_at.as_deref())
            .map_err(|e| e.to_string())?;

        if let (true, Some(url)) = (ended, status.results_url.as_deref()) {
            let results = provider.batch_results(url).await.map_err(|e| e.to_string())?;
            let items: Vec<BatchItemRecord> = results
                .into_iter()
                .map(|r| BatchItemRecord {
                    job_id: job_id.to_string(),
                    custom_id: r.custom_id,
                    question: String::new(), // not updated; the stored question is kept
                    answer: r.text,
                    error: r.error,
                    input_tokens: r.input_tokens as i64,
                    output_tokens: r.output_tokens as i64,
                })
                .collect();
            db.save_batch_results(job_id, &items).map_err(|e| e.to_string())?;
        }
    }

    let job = db
        .get_batch_job(job_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Batch not found: {}", job_id))?;
    let items = db.list_batch_items(job_id).map_err(|e| e.to_string())?;
    Ok((job, items))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn questions_are_trimmed_and_deduplicated() {
        let q = normalize_questions(&[
            "  What is the revenue? ".to_string(),
            "".to_string(),
            "What is the revenue?".to_string(),
            "Who signed?".to_string(),
        ])
        .unwrap();
        assert_eq!(q, vec!["What is the revenue?", "Who signed?"]);
    }

    #[test]
    fn empty_and_oversized_batches_are_rejected() {
        assert!(normalize_questions(&["  ".to_string()]).is_err());
        let many: Vec<String> = (0..=MAX_BATCH_QUESTIONS).map(|i| format!("Question {i}?")).collect();
        assert!(normalize_questions(&many).is_err());
    }

    #[test]
    fn only_anthropic_has_batch_support() {
        assert!(supports_batches("anthropic"));
        assert!(supports_batches("Anthropic"));
        assert!(!supports_batches("openai"));
        assert!(!supports_batches("local"));
    }

    #[test]
    fn batched_prompt_matches_the_interactive_shape() {
        let mut tree = DocumentTree::new("report.pdf".to_string(), crate::document::tree::DocType::Pdf);
        let root = tree.root_id.clone();
        tree.add_node(
            &root,
            crate::document::tree::TreeNode::new(
                crate::document::tree::NodeType::Section,
                "Revenue was $1.2M in Q3 2025.".to_string(),
            ),
        )
        .unwrap();

        let messages = answer_messages("What was the revenue?", &[tree], "");
        assert_eq!(messages.first().unwrap().role, "system");
        assert!(messages[0].content.contains(crate::util::DOC_FENCE_START)); // guardrail rule
        let last = messages.last().unwrap();
        assert_eq!((last.role.as_str(), last.content.as_str()), ("user", "What was the revenue?"));
        // When content was fetched it is fenced and carries the cache breakpoint.
        if let Some(doc) = messages.iter().find(|m| m.content.starts_with("[Document content]")) {
            assert!(doc.cache);
            assert!(doc.content.contains(crate::util::DOC_FENCE_END));
        }
    }
}
