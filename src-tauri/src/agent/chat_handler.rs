use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use serde::Deserialize;
use tauri::ipc::Channel;

use crate::db::{CrossDocRelation, Database, StepRecord, TraceRecord};
use crate::document::metadata::discover_cross_doc_relations;
use crate::document::tree::DocumentTree;
use crate::llm::provider::{LLMProvider, Message, ProviderConfig};
use crate::llm::{
    AgentRouterProvider, AnthropicProvider, GoogleProvider, GroqProvider,
    OllamaProvider, OpenAICompatProvider, OpenRouterProvider, RetryProvider,
};

use super::events::ChatEvent;
use super::deterministic::{fetch_content, format_for_prompt};
use super::query::{preprocess_query, rewrite_query, generate_hyde, stepback_query, extract_terms_from_text, EnrichmentResult, QueryIntent};
use crate::llm::local;

/// Rough token estimate: ~4 chars per token plus overhead for role/formatting.
fn estimate_tokens(msg: &Message) -> usize {
    (msg.content.len() + msg.role.len() + 10) / 4
}

/// Context budget for conversation history (tokens).
const HISTORY_TOKEN_BUDGET: usize = 8000;

pub fn create_provider(config: ProviderConfig) -> Result<Box<dyn LLMProvider>, String> {
    let provider_name = config.name.to_lowercase();
    let inner: Box<dyn LLMProvider> = match provider_name.as_str() {
        "ollama" => Box::new(OllamaProvider::new(config)),
        "groq" => Box::new(GroqProvider::new(config)),
        "google" => Box::new(GoogleProvider::new(config)),
        "openrouter" => Box::new(OpenRouterProvider::new(config)),
        "agentrouter" => Box::new(AgentRouterProvider::new(config)),
        "anthropic" => Box::new(AnthropicProvider::new(config)),
        "openai" => Box::new(OpenAICompatProvider::new(
            config, "OpenAI", "https://api.openai.com/v1",
        )),
        "deepseek" => Box::new(OpenAICompatProvider::new(
            config, "DeepSeek", "https://api.deepseek.com/v1",
        )),
        "xai" => Box::new(OpenAICompatProvider::new(
            config, "xAI", "https://api.x.ai/v1",
        )),
        "qwen" => Box::new(OpenAICompatProvider::new(
            config, "Qwen", "https://dashscope-intl.aliyuncs.com/compatible-mode/v1",
        )),
        "openai-compat" => Box::new(OpenAICompatProvider::new(
            config, "Custom", "",
        )),
        _ => return Err(format!("Unknown provider: {}", config.name)),
    };
    // Wrap all providers with automatic retry on transient errors
    Ok(Box::new(RetryProvider::new(inner)))
}


#[derive(Deserialize, Clone, Debug)]
struct ModelPricing {
    input: f64,
    output: f64,
}

fn pricing_table() -> &'static HashMap<String, ModelPricing> {
    static TABLE: OnceLock<HashMap<String, ModelPricing>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let json_str = include_str!("../pricing.json");
        serde_json::from_str(json_str).unwrap_or_default()
    })
}

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

/// A pipeline step record for trace persistence (covers preprocessing, fetch, and LLM steps).
struct PipelineStep {
    tool: String,
    input: String,
    output: String,
    tokens: i64,
    latency_ms: i64,
}

fn save_trace_data(
    db: &Database,
    conv_id: &str,
    model_id: &str,
    pipeline_steps: &[PipelineStep],
    total_input_tokens: u32,
    total_output_tokens: u32,
    total_latency_ms: u64,
) -> Result<(), String> {
    let trace_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let total_tokens = total_input_tokens + total_output_tokens;
    let cost = estimate_cost(model_id, total_input_tokens, total_output_tokens);

    let trace = TraceRecord {
        id: trace_id.clone(),
        conv_id: conv_id.to_string(),
        provider_name: model_id.to_string(),
        total_tokens: total_tokens as i64,
        total_cost: cost,
        total_latency_ms: total_latency_ms as i64,
        steps_count: pipeline_steps.len() as i64,
        created_at: now,
        input_tokens: total_input_tokens as i64,
        output_tokens: total_output_tokens as i64,
    };
    db.save_trace(&trace).map_err(|e| e.to_string())?;

    for step in pipeline_steps {
        let step_record = StepRecord {
            id: uuid::Uuid::new_v4().to_string(),
            msg_id: trace_id.clone(),
            tool_name: step.tool.clone(),
            input_json: step.input.clone(),
            output_json: step.output.clone(),
            tokens_used: step.tokens,
            latency_ms: step.latency_ms,
        };
        db.save_step(&step_record).map_err(|e| e.to_string())?;
    }

    Ok(())
}

/// Build a human-readable output summary for a deterministic fetch step.
fn fetch_step_output_summary(
    step: &super::deterministic::FetchStep,
    fetched: &super::deterministic::FetchedContent,
) -> String {
    match step.operation.as_str() {
        "tree_overview" => format!("{} top-level sections found", step.node_ids.len()),
        "search" => format!("{} matching nodes", step.node_ids.len()),
        "expand" => {
            let content: String = fetched.sections.iter()
                .filter(|s| s.node_ids.iter().any(|nid| step.node_ids.contains(nid)))
                .map(|s| s.content.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            if content.is_empty() {
                format!("{} nodes expanded", step.node_ids.len())
            } else {
                let preview = content.trim().replace('\n', " ");
                let preview = preview.split_whitespace().collect::<Vec<_>>().join(" ");
                if preview.len() > 200 { format!("{}…", crate::util::safe_truncate(&preview, 200)) } else { preview }
            }
        }
        "scan_lists" => format!("{} list/table sections found", step.node_ids.len()),
        _ => format!("{} nodes", step.node_ids.len()),
    }
}

/// Decide whether this query needs enrichment via local model.
///
/// Enrichment is skipped when:
/// - No local SLM engine is loaded (no local model downloaded).
/// - The query is already long and specific (≥8 words + ≥4 search terms).
/// - The query has a clear intent with many terms (Comparison/ListExtract with ≥3 terms,
///   or Factual/Entity with ≥4 terms).
///
/// Enrichment runs when:
/// - Query is short (≤3 words) — single-word or terse queries need expansion.
/// - Summarize intent with few terms — summary queries benefit from broader terminology.
/// - Very few search terms (<3) — enrichment generates better vocabulary matches.
/// - Specific/Factual intent with few terms — unclear queries need disambiguation.
fn should_enrich(processed: &super::query::ProcessedQuery) -> bool {
    // Hard gate: local SLM engine must be loaded (model downloaded by user)
    if !local::is_engine_loaded() {
        return false;
    }

    let word_count = processed.original.split_whitespace().count();
    let term_count = processed.search_terms.len();

    // Always enrich short/terse queries regardless of intent
    if word_count <= 3 {
        return true;
    }

    // Always enrich when very few meaningful terms were extracted
    if term_count < 3 {
        return true;
    }

    // Always enrich summarize intent (needs broad vocabulary coverage)
    if matches!(processed.intent, QueryIntent::Summarize) {
        return true;
    }

    // Skip enrichment for long, rich queries (already well-specified)
    if word_count >= 8 && term_count >= 4 {
        return false;
    }

    // Skip for comparison/list extraction with sufficient terms (already structured)
    if matches!(processed.intent, QueryIntent::Comparison | QueryIntent::ListExtract)
        && term_count >= 3
    {
        return false;
    }

    // Skip for entity/factual queries with many terms (specific enough)
    if matches!(processed.intent, QueryIntent::Entity | QueryIntent::Factual)
        && term_count >= 4
    {
        return false;
    }

    // Default: enrich
    true
}

/// Main deterministic chat pipeline with LLM-powered query enrichment.
///
/// Flow: heuristic preprocess → query rewrite + HyDE + StepBack (3 LLM calls)
///       → deterministic content fetch → ONE streaming LLM call → answer.
///
/// All events (steps, tokens, response, error) are sent via the Tauri Channel.
#[allow(clippy::too_many_arguments)]
pub async fn run_agent_chat(
    channel: &Channel<ChatEvent>,
    db: &Database,
    cancel: Arc<AtomicBool>,
    message: String,
    trees: Vec<DocumentTree>,
    provider_config: ProviderConfig,
    conv_id: Option<String>,
    doc_ids: Vec<String>,
) -> Result<(), String> {
    let model_id = provider_config.model.clone();
    let provider = create_provider(provider_config)?;
    let overall_start = tokio::time::Instant::now();

    // Accumulate all pipeline steps for trace persistence
    let mut trace_steps: Vec<PipelineStep> = Vec::new();
    // Track total tokens across all LLM calls (preprocessing + final)
    let mut total_input_tokens = 0u32;
    let mut total_output_tokens = 0u32;
    // Global step counter for UI events
    let mut step_counter = 0u32;

    // ── Phase 1: Fast heuristic preprocessing (sync) ────────────────
    let mut processed = preprocess_query(&message);

    // ── Phase 2: Check cancel ───────────────────────────────────────
    if cancel.load(Ordering::SeqCst) {
        let _ = channel.send(ChatEvent::Error { error: "Query cancelled.".to_string() });
        return Ok(());
    }

    // ── Phase 3: LLM-powered query enrichment ───────────────────────
    // Query Rewrite, HyDE, StepBack — improve retrieval term coverage.
    // Uses local candle SLM engine (free, in-process) for enrichment calls.
    // Only runs when the query is ambiguous/short/unclear (see should_enrich).
    // All three are non-fatal: errors report as "Skipped" and pipeline continues.

    if should_enrich(&processed) {
        let steps = [
            ("query_rewrite", format!("Rephrasing: \"{}\"", message)),
            ("hyde", format!("Generating hypothetical answer for: \"{}\"", message)),
            ("stepback", format!("Broadening: \"{}\"", message)),
        ];

        // Assign step numbers upfront and emit all StepStart events immediately
        let mut step_numbers = [0u32; 3];
        for (i, (tool, input_summary)) in steps.iter().enumerate() {
            step_counter += 1;
            step_numbers[i] = step_counter;
            let _ = channel.send(ChatEvent::StepStart {
                step_number: step_counter,
                tool: tool.to_string(),
                input_summary: input_summary.clone(),
            });
        }

        // The local model has a single KV cache, so the three prompts run one after
        // another on a blocking thread. Each generation is time-capped and stops on
        // cancel; results are reported to the UI as soon as each one finishes.
        let (result_tx, mut result_rx) = tokio::sync::mpsc::unbounded_channel();
        let query = message.clone();
        let worker_cancel = cancel.clone();
        let worker = tokio::task::spawn_blocking(move || {
            type Enricher = fn(&str, &AtomicBool) -> Result<EnrichmentResult, String>;
            let enrichers: [Enricher; 3] = [rewrite_query, generate_hyde, stepback_query];
            for (i, enrich) in enrichers.into_iter().enumerate() {
                let result = if worker_cancel.load(Ordering::SeqCst) {
                    Err("Query cancelled".to_string())
                } else {
                    enrich(&query, &worker_cancel)
                };
                if result_tx.send((i, result)).is_err() {
                    break;
                }
            }
        });

        let mut reported = [false; 3];
        while let Some((i, result)) = result_rx.recv().await {
            reported[i] = true;
            let (output, tokens, latency_ms) = match result {
                Ok(enrichment) => {
                    for t in extract_terms_from_text(&enrichment.text) {
                        if !processed.search_terms.contains(&t) {
                            processed.search_terms.push(t);
                        }
                    }
                    let tokens = enrichment.input_tokens + enrichment.output_tokens;
                    (enrichment.text, tokens, enrichment.latency_ms)
                }
                Err(e) => (format!("Skipped: {}", e), 0, 0),
            };
            let _ = channel.send(ChatEvent::StepComplete {
                step_number: step_numbers[i],
                output_summary: output.clone(),
                tokens_used: tokens,
                latency_ms,
                cost: 0.0, // local model — free
                node_ids: vec![],
            });
            trace_steps.push(PipelineStep {
                tool: steps[i].0.to_string(),
                input: message.clone(),
                output,
                tokens: tokens as i64,
                latency_ms: latency_ms as i64,
            });
        }

        // If the worker died, close out the steps it never reported.
        if let Err(e) = worker.await {
            for i in (0..3).filter(|&i| !reported[i]) {
                let output = format!("Skipped: enrichment worker failed ({})", e);
                let _ = channel.send(ChatEvent::StepComplete {
                    step_number: step_numbers[i],
                    output_summary: output.clone(),
                    tokens_used: 0,
                    latency_ms: 0,
                    cost: 0.0,
                    node_ids: vec![],
                });
                trace_steps.push(PipelineStep {
                    tool: steps[i].0.to_string(),
                    input: message.clone(),
                    output,
                    tokens: 0,
                    latency_ms: 0,
                });
            }
        }
    }

    // ── Cancel check: after enrichment, before fetch ────────────────
    if cancel.load(Ordering::SeqCst) {
        let _ = channel.send(ChatEvent::Error { error: "Query cancelled.".to_string() });
        return Ok(());
    }

    // ── Phase 4: Deterministic content fetch (enriched terms) ───────
    let fetched = fetch_content(&processed, &trees);

    // Send fetch steps as UI events (step numbers offset by preprocessing steps)
    for step in &fetched.fetch_steps {
        step_counter += 1;
        let output_summary = fetch_step_output_summary(step, &fetched);

        let _ = channel.send(ChatEvent::StepStart {
            step_number: step_counter,
            tool: step.operation.clone(),
            input_summary: step.description.clone(),
        });
        let _ = channel.send(ChatEvent::StepComplete {
            step_number: step_counter,
            output_summary: output_summary.clone(),
            tokens_used: 0,
            latency_ms: 0,
            cost: 0.0,
            node_ids: step.node_ids.clone(),
        });

        trace_steps.push(PipelineStep {
            tool: step.operation.clone(),
            input: step.description.clone(),
            output: output_summary,
            tokens: 0,
            latency_ms: 0,
        });
    }

    // ── Cancel check: after fetch, before prompt build ──────────────
    if cancel.load(Ordering::SeqCst) {
        let _ = channel.send(ChatEvent::Error { error: "Query cancelled.".to_string() });
        return Ok(());
    }

    // ── Phase 5: Build prompt with fetched content ──────────────────
    let content_text = format_for_prompt(&fetched);

    let doc_label = if trees.len() > 1 {
        format!("{} documents", trees.len())
    } else {
        trees.first().map(|t| t.name.clone()).unwrap_or_else(|| "document".to_string())
    };

    // Discover and persist new cross-doc relations from metadata
    if trees.len() > 1 {
        let discovered = discover_cross_doc_relations(&trees);
        for rel in &discovered {
            let record = CrossDocRelation {
                id: uuid::Uuid::new_v4().to_string(),
                source_doc_id: rel.source_doc_id.clone(),
                source_node_id: rel.source_node_id.clone(),
                target_doc_id: rel.target_doc_id.clone(),
                target_node_id: rel.target_node_id.clone(),
                relation_type: rel.relation_type.clone(),
                confidence: rel.confidence,
                description: Some(rel.description.clone()),
                created_at: chrono::Utc::now().to_rfc3339(),
            };
            let _ = db.save_cross_doc_relation(&record);
        }
    }

    // Load previously discovered cross-doc relations for context
    let relations_context = if trees.len() > 1 {
        let known = db.get_cross_doc_relations_for_docs(&doc_ids).unwrap_or_default();
        if known.is_empty() {
            String::new()
        } else {
            let lines: Vec<String> = known.iter().take(15).map(|r| {
                let desc = r.description.as_deref().unwrap_or("");
                format!("- {}: {}", r.relation_type, desc)
            }).collect();
            format!(
                "\n\nKnown relations between these documents:\n{}",
                lines.join("\n")
            )
        }
    } else {
        String::new()
    };

    // System prompt — no tools, just answer from pre-fetched content.
    let system_prompt = format!(
        r#"You are a document analysis expert. Answer based on the provided document content.

Rules:
1. Cite specific facts — names, dates, numbers from the documents.
2. If information is not found, say so explicitly.
3. For multi-document queries, address each document where relevant.
4. Use markdown: **bold**, - bullets, ## headings, tables as appropriate.
5. Never reference internal node IDs or technical metadata.
6. Be concise but thorough.{relations}

Working with: {label}"#,
        relations = relations_context,
        label = doc_label,
    );

    // ── Cancel check: after prompt build, before history load ───────
    if cancel.load(Ordering::SeqCst) {
        let _ = channel.send(ChatEvent::Error { error: "Query cancelled.".to_string() });
        return Ok(());
    }

    // ── Phase 6: Load conversation history ──────────────────────────
    let mut history_messages: Vec<Message> = Vec::new();
    if let Some(ref cid) = conv_id
        && let Ok(records) = db.get_conversation_messages(cid) {
            let relevant: Vec<_> = records.iter()
                .filter(|r| !(r.role == "user" && r.content == message))
                .collect();
            let mut budget_remaining = HISTORY_TOKEN_BUDGET;
            for r in relevant.iter().rev() {
                let msg = Message::text(&r.role, &r.content);
                let tokens = estimate_tokens(&msg);
                if tokens > budget_remaining {
                    break;
                }
                budget_remaining -= tokens;
                history_messages.push(msg);
            }
            history_messages.reverse();
        }

    let mut messages: Vec<Message> = vec![
        Message::text("system", &system_prompt),
    ];
    messages.extend(history_messages);

    // Inject deterministic seed content as a pre-loaded context exchange.
    if !content_text.is_empty() {
        messages.push(Message::text(
            "user",
            &format!("[Document content]\n\n{}", content_text),
        ));
        messages.push(Message::text(
            "assistant",
            "I have reviewed the document content and am ready to answer your question.",
        ));
    }

    messages.push(Message::text("user", &message));

    // ── Phase 7: Check cancel before LLM call ───────────────────────
    if cancel.load(Ordering::SeqCst) {
        let _ = channel.send(ChatEvent::Error { error: "Query cancelled.".to_string() });
        return Ok(());
    }

    // ── Phase 8: Stream the LLM response (no tools) ────────────────
    step_counter += 1;
    let llm_step_number = step_counter;
    let _ = channel.send(ChatEvent::StepStart {
        step_number: llm_step_number,
        tool: "llm_call".to_string(),
        input_summary: format!("Generating answer with {}", model_id),
    });

    let llm_start = tokio::time::Instant::now();

    let (token_tx, mut token_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let channel_clone = channel.clone();
    let stream_forwarder = tokio::spawn(async move {
        while let Some(token) = token_rx.recv().await {
            let _ = channel_clone.send(ChatEvent::Token { token, done: false });
        }
        let _ = channel_clone.send(ChatEvent::Token { token: String::new(), done: true });
    });

    // Pass None for tools here — we want a pure text response, not more tool calls
    let llm_response = match provider.chat_stream(messages, None, token_tx).await {
        Ok(resp) => resp,
        Err(e) => {
            let _ = channel.send(ChatEvent::Error {
                error: format!("LLM error: {}", e),
            });
            return Err(format!("LLM error: {}", e));
        }
    };

    let _ = stream_forwarder.await;
    let llm_latency_ms = llm_start.elapsed().as_millis() as u64;

    total_input_tokens += llm_response.input_tokens;
    total_output_tokens += llm_response.output_tokens;
    let llm_tokens = llm_response.tokens_used.max(
        llm_response.input_tokens + llm_response.output_tokens,
    );
    let llm_cost = estimate_cost(&model_id, llm_response.input_tokens, llm_response.output_tokens);

    let _ = channel.send(ChatEvent::StepComplete {
        step_number: llm_step_number,
        output_summary: format!(
            "{} tokens ({} in / {} out) · {}",
            llm_tokens,
            llm_response.input_tokens,
            llm_response.output_tokens,
            if llm_cost < 0.01 { format!("${:.4}", llm_cost) } else { format!("${:.2}", llm_cost) }
        ),
        tokens_used: llm_tokens,
        latency_ms: llm_latency_ms,
        cost: llm_cost,
        node_ids: vec![],
    });

    trace_steps.push(PipelineStep {
        tool: "llm_call".to_string(),
        input: format!("{} input tokens", llm_response.input_tokens),
        output: format!("{} output tokens", llm_response.output_tokens),
        tokens: llm_tokens as i64,
        latency_ms: llm_latency_ms as i64,
    });

    let answer = llm_response.content.unwrap_or_else(|| {
        "I couldn't generate a response based on the document content. \
         Try rephrasing your question or asking about a different aspect."
            .to_string()
    });

    let _ = channel.send(ChatEvent::Response { content: answer });

    // ── Phase 9: Save trace ─────────────────────────────────────────
    let trace_conv_id = conv_id.as_deref().unwrap_or(
        doc_ids.first().map(|s| s.as_str()).unwrap_or("unknown"),
    );
    let total_latency_ms = overall_start.elapsed().as_millis() as u64;

    save_trace_data(
        db,
        trace_conv_id,
        &model_id,
        &trace_steps,
        total_input_tokens,
        total_output_tokens,
        total_latency_ms,
    )?;

    Ok(())
}
