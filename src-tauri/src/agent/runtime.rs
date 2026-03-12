// NOTE: This file is SCAFFOLDING for a future ReAct agent loop.
// It is NOT called by any live code path — the current pipeline uses
// chat_handler.rs + deterministic.rs instead. Do not delete; planned for M5+.
#![allow(dead_code)]

use serde::Serialize;
use std::collections::HashSet;
use std::time::Instant;
use thiserror::Error;

use crate::util::safe_truncate;

use crate::document::tree::{DocumentTree, NodeType};
use super::context::ExplorationContext;
use super::tools::{AgentTool, ToolInput, ToolOutput};

/// Build the system prompt for the document exploration agent.
/// Includes the document tree overview and query-specific exploration hints.
pub fn build_system_prompt(tree_overview: &str, exploration_hint: &str, doc_count: usize) -> String {
    let multi_doc_rules = if doc_count > 1 {
        format!(
            "\n13. You have {n} documents loaded. You MUST explore ALL of them before answering — not just the first one.\n\
             14. For summarize/overview queries, provide a separate summary for EACH document.\n\
             15. Actively look for connections between documents and use record_relation to save them.",
            n = doc_count
        )
    } else {
        String::new()
    };

    let doc_label = if doc_count > 1 { "documents overview" } else { "document overview" };

    format!(
        r#"You are a document exploration agent. You navigate document trees to answer user questions with SPECIFIC, DETAILED information extracted from the actual document content.

Available tools:
- tree_overview(doc_id): See top-level structure of a document
- expand_node(node_id): Read the full content of a section and see its children
- search_content(query, scope?): Search for specific text within a single document
- search_across_docs(query, max_results?): Search ALL loaded documents at once — results grouped by document
- get_relations(node_id): See cross-references and cross-document relations for a node
- get_node_context(node_id): Understand where a node sits in the hierarchy
- get_image(node_id): Retrieve image node information
- compare_nodes(node_a, node_b): Compare two sections side by side (works across documents)
- record_relation(source_node_id, target_node_id, relation_type, description?): Record a discovered relationship between two nodes

CRITICAL exploration rules — you MUST follow these:
1. The tree overview only shows section TITLES and child counts — it does NOT contain the actual content.
2. You MUST call expand_node on relevant sections to read their actual text BEFORE answering.
3. NEVER answer based solely on section titles or the tree overview. You have NOT read the document until you call expand_node or search_content.
4. If the tree overview only has 1-3 sections, expand ALL of them — the document is small enough.
5. For broad questions (summarize, explain, what is this about), expand the 3-5 most important sections.
6. For specific questions, use search_content to find relevant content, then expand matching nodes.
7. If a section has many children, expand the most relevant children too.
8. You may call multiple tools at once (parallel tool calls) for efficiency.
9. When multiple documents are loaded, use search_across_docs to find information spanning all documents.
10. When comparing across documents, use search_across_docs first to locate relevant sections, then compare_nodes to examine them side by side.
11. When you discover meaningful relationships between nodes (shared entities, topic overlap, contradictions, or supporting evidence), use record_relation to persist them. Relation types: shared_entity, topic_overlap, contradiction, supports, references.
12. Check get_relations for previously discovered cross-document relations that may inform your answer.{multi_doc_rules}

{exploration_hint}

Current {doc_label}:
{tree_overview}

IMPORTANT rules for your final answer:
- Your answer must contain SPECIFIC FACTS extracted from the document, not vague descriptions of sections.
- BAD: "The Features section lists the features of this project"
- GOOD: "The key features include: no embeddings/vector DB (uses agentic exploration), universal document tree parsing for PDF/Markdown/code, and multi-provider LLM support."
- Write in markdown format: use **bold**, - bullet points, ## headings, tables, and `code` as appropriate.
- Do NOT include node IDs, UUIDs, or internal identifiers — reference sections by title.
- When you have gathered enough information, provide your final answer (do not call any more tools)."#,
        multi_doc_rules = multi_doc_rules,
        doc_label = doc_label,
        tree_overview = tree_overview,
        exploration_hint = exploration_hint,
    )
}

#[derive(Error, Debug)]
pub enum RuntimeError {
    #[error("Node not found: {0}")]
    NodeNotFound(String),
    #[error("Missing parameter: {0}")]
    MissingParam(String),
    #[error("Budget exhausted: no steps remaining")]
    BudgetExhausted,
}

#[derive(Serialize, Clone, Debug)]
pub struct ExplorationStep {
    pub step_number: u32,
    pub tool: String,
    pub input_summary: String,
    pub output_summary: String,
    pub tokens_used: u32,
    pub latency_ms: u64,
}

#[derive(Serialize, Clone, Debug)]
pub struct AgentResponse {
    pub answer: String,
    pub steps: Vec<ExplorationStep>,
    pub total_tokens: u32,
    pub total_latency_ms: u64,
}

pub struct AgentRuntime {
    pub context: ExplorationContext,
    pub steps: Vec<ExplorationStep>,
}

impl AgentRuntime {
    pub fn new(max_steps: u32) -> Self {
        Self {
            context: ExplorationContext::new(max_steps),
            steps: Vec::new(),
        }
    }

    pub fn execute_tool(
        &mut self,
        tree: &DocumentTree,
        input: &ToolInput,
    ) -> Result<ToolOutput, RuntimeError> {
        if self.context.budget_remaining() == 0 {
            return Err(RuntimeError::BudgetExhausted);
        }

        let start = Instant::now();

        let result = match input.tool {
            AgentTool::TreeOverview => {
                let overview = tree.tree_overview();
                serde_json::to_value(&overview).unwrap_or_default()
            }
            AgentTool::ExpandNode => {
                let node_id = input
                    .params
                    .get("node_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| RuntimeError::MissingParam("node_id".to_string()))?;

                let node = tree
                    .get_node(node_id)
                    .ok_or_else(|| RuntimeError::NodeNotFound(node_id.to_string()))?;

                let children = tree.get_children(node_id);
                serde_json::json!({
                    "node": node,
                    "children": children,
                })
            }
            AgentTool::SearchContent => {
                let query = input
                    .params
                    .get("query")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| RuntimeError::MissingParam("query".to_string()))?
                    .to_lowercase();

                let scope = input
                    .params
                    .get("scope")
                    .and_then(|v| v.as_str());

                let mut matches = Vec::new();
                for (id, node) in &tree.nodes {
                    // If scope is specified, only search within that subtree
                    if let Some(scope_id) = scope {
                        if !is_descendant_of(tree, id, scope_id) && id != scope_id {
                            continue;
                        }
                    }
                    if node.content.to_lowercase().contains(&query) {
                        matches.push(serde_json::json!({
                            "id": id,
                            "node_type": node.node_type,
                            "content_preview": if node.content.len() > 200 {
                                format!("{}...", safe_truncate(&node.content, 200))
                            } else {
                                node.content.clone()
                            },
                        }));
                    }
                }
                serde_json::json!({ "matches": matches, "count": matches.len() })
            }
            AgentTool::GetRelations => {
                let node_id = input
                    .params
                    .get("node_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| RuntimeError::MissingParam("node_id".to_string()))?;

                let node = tree
                    .get_node(node_id)
                    .ok_or_else(|| RuntimeError::NodeNotFound(node_id.to_string()))?;

                serde_json::to_value(&node.relations).unwrap_or_default()
            }
            AgentTool::GetImage => {
                let node_id = input
                    .params
                    .get("node_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| RuntimeError::MissingParam("node_id".to_string()))?;

                let node = tree
                    .get_node(node_id)
                    .ok_or_else(|| RuntimeError::NodeNotFound(node_id.to_string()))?;

                if node.node_type != NodeType::Image {
                    return Err(RuntimeError::NodeNotFound(format!(
                        "Node {} is not an image node",
                        node_id
                    )));
                }

                serde_json::json!({
                    "id": node.id,
                    "content": node.content,
                    "raw_image_path": node.raw_image_path,
                    "metadata": node.metadata,
                })
            }
            AgentTool::CompareNodes => {
                let node_a_id = input
                    .params
                    .get("node_a")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| RuntimeError::MissingParam("node_a".to_string()))?;

                let node_b_id = input
                    .params
                    .get("node_b")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| RuntimeError::MissingParam("node_b".to_string()))?;

                let node_a = tree
                    .get_node(node_a_id)
                    .ok_or_else(|| RuntimeError::NodeNotFound(node_a_id.to_string()))?;

                let node_b = tree
                    .get_node(node_b_id)
                    .ok_or_else(|| RuntimeError::NodeNotFound(node_b_id.to_string()))?;

                serde_json::json!({
                    "node_a": node_a,
                    "node_b": node_b,
                })
            }
            AgentTool::SearchAcrossDocs => {
                // Single-tree fallback: search this tree only.
                // For full multi-doc search, use execute_multi_doc_tool instead.
                let query = input
                    .params
                    .get("query")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| RuntimeError::MissingParam("query".to_string()))?
                    .to_lowercase();

                let max_results = input
                    .params
                    .get("max_results")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(5) as usize;

                let mut matches = Vec::new();
                for (id, node) in &tree.nodes {
                    if node.content.to_lowercase().contains(&query) {
                        matches.push(serde_json::json!({
                            "id": id,
                            "node_type": node.node_type,
                            "content_preview": if node.content.len() > 200 {
                                format!("{}...", safe_truncate(&node.content, 200))
                            } else {
                                node.content.clone()
                            },
                        }));
                        if matches.len() >= max_results {
                            break;
                        }
                    }
                }
                serde_json::json!({
                    "documents": [{
                        "doc_name": tree.name,
                        "doc_id": tree.id,
                        "matches": matches,
                        "match_count": matches.len(),
                    }]
                })
            }
            AgentTool::RecordRelation => {
                // RecordRelation is handled in chat_handler.rs (needs DB access).
                // This branch should never be reached.
                serde_json::json!({ "error": "record_relation must be executed via chat_handler" })
            }
            AgentTool::GetNodeContext => {
                let node_id = input
                    .params
                    .get("node_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| RuntimeError::MissingParam("node_id".to_string()))?;

                // Build parent chain from root to node
                let chain = build_parent_chain(tree, node_id);
                serde_json::json!({
                    "path": chain,
                    "depth": chain.len(),
                })
            }
        };

        let latency_ms = start.elapsed().as_millis() as u64;

        // Record the step in the exploration context
        let output_preview = result.to_string();
        let output_summary = if output_preview.len() > 100 {
            format!("{}...", safe_truncate(&output_preview, 100))
        } else {
            output_preview
        };

        let step = ExplorationStep {
            step_number: self.context.step_count + 1,
            tool: format!("{:?}", input.tool),
            input_summary: format!("{:?}", input.params),
            output_summary: output_summary.clone(),
            tokens_used: 0, // Tokens are tracked by the LLM layer, not the tool
            latency_ms,
        };
        self.steps.push(step);

        // Record in context
        if let Some(node_id) = input.params.get("node_id").and_then(|v| v.as_str()) {
            self.context.record_step(node_id, &output_summary, 0);
        } else {
            self.context.record_step("_tool_call", &output_summary, 0);
        }

        Ok(ToolOutput {
            tool: input.tool.clone(),
            result,
            tokens_used: 0,
            latency_ms,
        })
    }
}

impl AgentRuntime {
    /// Execute a tool that operates across multiple document trees.
    /// Currently handles SearchAcrossDocs; other tools fall back to single-tree execution.
    pub fn execute_multi_doc_tool(
        &mut self,
        trees: &[DocumentTree],
        input: &ToolInput,
    ) -> Result<ToolOutput, RuntimeError> {
        if !input.tool.is_multi_doc() || trees.len() <= 1 {
            return self.execute_tool(&trees[0], input);
        }

        if self.context.budget_remaining() == 0 {
            return Err(RuntimeError::BudgetExhausted);
        }

        let start = Instant::now();

        let result = match input.tool {
            AgentTool::SearchAcrossDocs => {
                let query = input
                    .params
                    .get("query")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| RuntimeError::MissingParam("query".to_string()))?
                    .to_lowercase();

                let max_results = input
                    .params
                    .get("max_results")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(5) as usize;

                let mut doc_results = Vec::new();
                for tree in trees {
                    let mut matches = Vec::new();
                    for (id, node) in &tree.nodes {
                        if node.content.to_lowercase().contains(&query) {
                            matches.push(serde_json::json!({
                                "id": id,
                                "node_type": node.node_type,
                                "content_preview": if node.content.len() > 200 {
                                    format!("{}...", safe_truncate(&node.content, 200))
                                } else {
                                    node.content.clone()
                                },
                            }));
                            if matches.len() >= max_results {
                                break;
                            }
                        }
                    }
                    doc_results.push(serde_json::json!({
                        "doc_name": tree.name,
                        "doc_id": tree.id,
                        "matches": matches,
                        "match_count": matches.len(),
                    }));
                }
                serde_json::json!({ "documents": doc_results })
            }
            _ => return self.execute_tool(&trees[0], input),
        };

        let latency_ms = start.elapsed().as_millis() as u64;

        let output_preview = result.to_string();
        let output_summary = if output_preview.len() > 100 {
            format!("{}...", safe_truncate(&output_preview, 100))
        } else {
            output_preview
        };

        let step = ExplorationStep {
            step_number: self.context.step_count + 1,
            tool: format!("{:?}", input.tool),
            input_summary: format!("{:?}", input.params),
            output_summary: output_summary.clone(),
            tokens_used: 0,
            latency_ms,
        };
        self.steps.push(step);
        self.context.record_step("_multi_doc_search", &output_summary, 0);

        Ok(ToolOutput {
            tool: input.tool.clone(),
            result,
            tokens_used: 0,
            latency_ms,
        })
    }
}

/// Check if a node is a descendant of a potential ancestor in the tree.
fn is_descendant_of(tree: &DocumentTree, node_id: &str, ancestor_id: &str) -> bool {
    let mut queue = vec![ancestor_id.to_string()];
    let mut visited = HashSet::new();
    visited.insert(ancestor_id.to_string());
    while let Some(current) = queue.pop() {
        if let Some(node) = tree.get_node(&current) {
            for child_id in &node.children {
                if child_id == node_id {
                    return true;
                }
                if visited.insert(child_id.clone()) {
                    queue.push(child_id.clone());
                }
            }
        }
    }
    false
}

/// Build the parent chain from root to a given node.
fn build_parent_chain(tree: &DocumentTree, target_id: &str) -> Vec<serde_json::Value> {
    // Build a parent map
    let mut parent_map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for (id, node) in &tree.nodes {
        for child_id in &node.children {
            parent_map.insert(child_id.clone(), id.clone());
        }
    }

    // Walk from target to root with cycle guard
    let mut chain = Vec::new();
    let mut current = target_id.to_string();
    let mut visited = HashSet::new();
    loop {
        if !visited.insert(current.clone()) {
            break; // cycle detected
        }
        if let Some(node) = tree.get_node(&current) {
            chain.push(serde_json::json!({
                "id": node.id,
                "node_type": node.node_type,
                "content_preview": if node.content.len() > 80 {
                    format!("{}...", safe_truncate(&node.content, 80))
                } else {
                    node.content.clone()
                },
            }));
        }
        match parent_map.get(&current) {
            Some(parent_id) => current = parent_id.clone(),
            None => break,
        }
    }

    chain.reverse();
    chain
}
