// Tool definitions for the agentic document exploration loop.
// These tools allow the LLM to navigate the document tree autonomously
// instead of relying solely on the deterministic content fetcher.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::document::tree::{DocumentTree, NodeType, TreeNode};
use crate::llm::provider::Tool;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AgentTool {
    TreeOverview,
    ExpandNode,
    SearchContent,
    GetRelations,
    GetImage,
    CompareNodes,
    GetNodeContext,
    SearchAcrossDocs,
    RecordRelation,
}

impl AgentTool {
    /// Parse a tool name string (from LLM response) into an AgentTool enum variant.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "tree_overview" => Some(Self::TreeOverview),
            "expand_node" => Some(Self::ExpandNode),
            "search_content" => Some(Self::SearchContent),
            "get_relations" => Some(Self::GetRelations),
            "get_image" => Some(Self::GetImage),
            "compare_nodes" => Some(Self::CompareNodes),
            "get_node_context" => Some(Self::GetNodeContext),
            "search_across_docs" => Some(Self::SearchAcrossDocs),
            "record_relation" => Some(Self::RecordRelation),
            _ => None,
        }
    }

    /// Returns true if this tool requires access to multiple document trees.
    pub fn is_multi_doc(&self) -> bool {
        matches!(self, Self::SearchAcrossDocs | Self::RecordRelation)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ToolInput {
    pub tool: AgentTool,
    pub params: HashMap<String, serde_json::Value>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ToolOutput {
    pub tool: AgentTool,
    pub result: serde_json::Value,
    pub tokens_used: u32,
    pub latency_ms: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters_schema: serde_json::Value,
}

pub fn get_tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "tree_overview".to_string(),
            description: "Get the top-level structure of all loaded documents. Shows document names, their top-level sections with content previews, and child counts. Call this first to orient yourself before searching or expanding nodes.".to_string(),
            parameters_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "doc_id": {
                        "type": "string",
                        "description": "Optional: filter to a specific document ID. Omit to get overview of all documents."
                    }
                }
            }),
        },
        ToolDefinition {
            name: "expand_node".to_string(),
            description: "Expand a specific node to see its FULL content and list of immediate children. Use this after search_content or tree_overview identifies a relevant node ID. Returns the complete text content of the node and its children's IDs and previews.".to_string(),
            parameters_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "node_id": {
                        "type": "string",
                        "description": "The node ID to expand (from tree_overview or search_content results)"
                    }
                },
                "required": ["node_id"]
            }),
        },
        ToolDefinition {
            name: "search_content".to_string(),
            description: "Search for text within document nodes using substring matching. Returns up to 10 matching nodes with their IDs, document names, content previews, and node types. Use this to quickly find relevant sections before expanding them.".to_string(),
            parameters_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "The search text to look for (case-insensitive)"
                    },
                    "scope": {
                        "type": "string",
                        "description": "Optional: node ID to limit the search scope to that subtree"
                    }
                },
                "required": ["query"]
            }),
        },
        ToolDefinition {
            name: "get_relations".to_string(),
            description: "Get all relations (references, dependencies, links, cross-document connections) for a specific node.".to_string(),
            parameters_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "node_id": {
                        "type": "string",
                        "description": "The node ID to get relations for"
                    }
                },
                "required": ["node_id"]
            }),
        },
        ToolDefinition {
            name: "get_image".to_string(),
            description: "Retrieve image node information including path, description, and dimensions.".to_string(),
            parameters_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "node_id": {
                        "type": "string",
                        "description": "The image node ID to retrieve"
                    }
                },
                "required": ["node_id"]
            }),
        },
        ToolDefinition {
            name: "compare_nodes".to_string(),
            description: "Compare the content and metadata of two nodes side by side. Useful for cross-document comparisons or comparing different sections of the same document.".to_string(),
            parameters_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "node_a": {
                        "type": "string",
                        "description": "First node ID"
                    },
                    "node_b": {
                        "type": "string",
                        "description": "Second node ID"
                    }
                },
                "required": ["node_a", "node_b"]
            }),
        },
        ToolDefinition {
            name: "get_node_context".to_string(),
            description: "Get the ancestor path from the document root to a specific node. Helps understand where a node sits in the document hierarchy (e.g. Chapter > Section > Subsection).".to_string(),
            parameters_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "node_id": {
                        "type": "string",
                        "description": "The node ID to get context for"
                    }
                },
                "required": ["node_id"]
            }),
        },
        ToolDefinition {
            name: "search_across_docs".to_string(),
            description: "Search for text across ALL loaded documents simultaneously. Returns results grouped by document name with up to max_results hits per document. Use when you need to find information that may span multiple documents.".to_string(),
            parameters_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "The search query string"
                    },
                    "max_results": {
                        "type": "integer",
                        "description": "Maximum number of results per document (default: 5)"
                    }
                },
                "required": ["query"]
            }),
        },
        ToolDefinition {
            name: "record_relation".to_string(),
            description: "Record a discovered relationship between two nodes (possibly across different documents). Use this when you notice that two sections share entities, overlap in topic, contradict each other, or one supports the other.".to_string(),
            parameters_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "source_node_id": {
                        "type": "string",
                        "description": "The source node ID"
                    },
                    "target_node_id": {
                        "type": "string",
                        "description": "The target node ID"
                    },
                    "relation_type": {
                        "type": "string",
                        "enum": ["shared_entity", "topic_overlap", "contradiction", "supports", "references"],
                        "description": "The type of relationship discovered"
                    },
                    "description": {
                        "type": "string",
                        "description": "Brief description of the relationship"
                    }
                },
                "required": ["source_node_id", "target_node_id", "relation_type"]
            }),
        },
    ]
}

/// Convert tool definitions to the provider `Tool` format used by LLM calls.
pub fn get_provider_tools() -> Vec<Tool> {
    get_tool_definitions()
        .into_iter()
        .map(|td| Tool {
            name: td.name,
            description: td.description,
            parameters: td.parameters_schema,
        })
        .collect()
}

/// Convert tool definitions to OpenAI function calling format.
/// Used by Groq, OpenRouter, and Ollama providers.
pub fn get_openai_tool_definitions() -> Vec<serde_json::Value> {
    get_tool_definitions()
        .into_iter()
        .map(|td| {
            serde_json::json!({
                "type": "function",
                "function": {
                    "name": td.name,
                    "description": td.description,
                    "parameters": td.parameters_schema,
                }
            })
        })
        .collect()
}

/// Convert tool definitions to Google Gemini function declarations format.
/// Google uses uppercase type values and a slightly different structure.
pub fn get_gemini_tool_definitions() -> Vec<serde_json::Value> {
    get_tool_definitions()
        .into_iter()
        .map(|td| {
            // Convert JSON Schema types to Gemini uppercase format
            let gemini_params = convert_schema_to_gemini(&td.parameters_schema);
            serde_json::json!({
                "name": td.name,
                "description": td.description,
                "parameters": gemini_params,
            })
        })
        .collect()
}

/// Recursively convert JSON Schema types to Gemini format (uppercase type strings).
fn convert_schema_to_gemini(schema: &serde_json::Value) -> serde_json::Value {
    match schema {
        serde_json::Value::Object(map) => {
            let mut result = serde_json::Map::new();
            for (key, value) in map {
                if key == "type" {
                    if let Some(type_str) = value.as_str() {
                        result.insert(
                            key.clone(),
                            serde_json::Value::String(type_str.to_uppercase()),
                        );
                    } else {
                        result.insert(key.clone(), value.clone());
                    }
                } else if key == "properties" {
                    // Recursively convert property schemas
                    if let Some(props) = value.as_object() {
                        let mut converted_props = serde_json::Map::new();
                        for (prop_name, prop_schema) in props {
                            converted_props.insert(
                                prop_name.clone(),
                                convert_schema_to_gemini(prop_schema),
                            );
                        }
                        result.insert(
                            key.clone(),
                            serde_json::Value::Object(converted_props),
                        );
                    } else {
                        result.insert(key.clone(), value.clone());
                    }
                } else {
                    result.insert(key.clone(), value.clone());
                }
            }
            serde_json::Value::Object(result)
        }
        other => other.clone(),
    }
}

// ── Tool Execution ────────────────────────────────────────────────────────────

/// Execute a tool call from the LLM, returning the result as JSON.
/// This is the dispatch function wired into the agentic chat loop.
pub fn execute_tool(
    name: &str,
    args: &serde_json::Value,
    trees: &[DocumentTree],
    db: &crate::db::Database,
) -> serde_json::Value {
    match name {
        "tree_overview" => tool_tree_overview(args, trees),
        "expand_node" => tool_expand_node(args, trees),
        "search_content" => tool_search_content(args, trees),
        "search_across_docs" => tool_search_across_docs(args, trees),
        "get_relations" => tool_get_relations(args, trees),
        "get_node_context" => tool_get_node_context(args, trees),
        "compare_nodes" => tool_compare_nodes(args, trees),
        "get_image" => tool_get_image(args, trees),
        "record_relation" => tool_record_relation(args, db),
        _ => serde_json::json!({ "error": format!("Unknown tool: {}", name) }),
    }
}

/// Find a node by ID across all document trees.
/// Returns (node reference, document name) if found.
fn find_node_in_trees<'a>(
    node_id: &str,
    trees: &'a [DocumentTree],
) -> Option<(&'a TreeNode, &'a str)> {
    for tree in trees {
        if let Some(node) = tree.nodes.get(node_id) {
            return Some((node, &tree.name));
        }
    }
    None
}

/// Recursively collect all nodes in a subtree rooted at `node_id`.
fn collect_subtree_nodes<'a>(
    node_id: &str,
    tree: &'a DocumentTree,
    out: &mut Vec<(&'a TreeNode, &'a str)>,
    doc_name: &'a str,
) {
    if let Some(node) = tree.nodes.get(node_id) {
        out.push((node, doc_name));
        for child_id in &node.children {
            collect_subtree_nodes(child_id, tree, out, doc_name);
        }
    }
}

/// Truncate content for previews.
fn preview(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", crate::util::safe_truncate(s, max))
    }
}

fn tool_tree_overview(args: &serde_json::Value, trees: &[DocumentTree]) -> serde_json::Value {
    let filter_doc_id = args["doc_id"].as_str();

    let documents: Vec<serde_json::Value> = trees
        .iter()
        .filter(|t| filter_doc_id.is_none_or(|id| t.id == id))
        .map(|tree| {
            let sections: Vec<serde_json::Value> = tree
                .rich_overview()
                .into_iter()
                .map(|s| {
                    serde_json::json!({
                        "id": s.id,
                        "type": format!("{:?}", s.node_type).to_lowercase(),
                        "title": s.content_preview,
                        "summary": s.summary,
                        "entities": s.entities,
                        "topics": s.topics,
                        "children_count": s.children_count,
                    })
                })
                .collect();
            serde_json::json!({
                "doc_id": tree.id,
                "doc_name": tree.name,
                "sections": sections,
            })
        })
        .collect();

    serde_json::json!({ "documents": documents })
}

fn tool_expand_node(args: &serde_json::Value, trees: &[DocumentTree]) -> serde_json::Value {
    let node_id = match args["node_id"].as_str() {
        Some(id) => id,
        None => return serde_json::json!({ "error": "node_id is required" }),
    };

    match find_node_in_trees(node_id, trees) {
        None => serde_json::json!({ "error": format!("Node '{}' not found", node_id) }),
        Some((node, doc_name)) => {
            // Find the tree that owns this node to get children details
            let children: Vec<serde_json::Value> = trees
                .iter()
                .find(|t| t.nodes.contains_key(node_id))
                .map(|tree| {
                    node.children
                        .iter()
                        .filter_map(|cid| tree.nodes.get(cid))
                        .map(|c| serde_json::json!({
                            "id": c.id,
                            "type": format!("{:?}", c.node_type).to_lowercase(),
                            "content_preview": preview(&c.content, 150),
                            "children_count": c.children.len(),
                        }))
                        .collect()
                })
                .unwrap_or_default();

            let summary = node.summary.clone()
                .or_else(|| node.metadata.get("summary").and_then(|v| v.as_str()).map(String::from));
            let entities: Vec<&str> = node.metadata.get("entities")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|e| e.as_str()).collect())
                .unwrap_or_default();
            let topics: Vec<&str> = node.metadata.get("topics")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|t| t.as_str()).collect())
                .unwrap_or_default();

            serde_json::json!({
                "id": node.id,
                "doc_name": doc_name,
                "type": format!("{:?}", node.node_type).to_lowercase(),
                "content": node.content,
                "summary": summary,
                "entities": entities,
                "topics": topics,
                "children": children,
                "relations_count": node.relations.len(),
            })
        }
    }
}

fn tool_search_content(args: &serde_json::Value, trees: &[DocumentTree]) -> serde_json::Value {
    let query = match args["query"].as_str() {
        Some(q) if !q.is_empty() => q.to_lowercase(),
        _ => return serde_json::json!({ "error": "query is required" }),
    };
    let scope = args["scope"].as_str();

    let mut matches: Vec<serde_json::Value> = Vec::new();

    'outer: for tree in trees {
        // Collect nodes to search: either full tree or scoped subtree
        let mut candidates: Vec<(&TreeNode, &str)> = Vec::new();
        if let Some(scope_id) = scope {
            collect_subtree_nodes(scope_id, tree, &mut candidates, &tree.name);
        } else {
            for node in tree.nodes.values() {
                candidates.push((node, &tree.name));
            }
        }

        for (node, doc_name) in candidates {
            // Skip root nodes and very short nodes
            if matches!(node.node_type, NodeType::Root) {
                continue;
            }
            if node.content.to_lowercase().contains(&query) {
                let content_preview = {
                    // Show context around the match
                    let lower = node.content.to_lowercase();
                    let pos = lower.find(&query).unwrap_or(0);
                    let start = pos.saturating_sub(60);
                    let end = (pos + query.len() + 60).min(node.content.len());
                    let snippet = &node.content[start..end];
                    if start > 0 { format!("…{}", snippet) } else { snippet.to_string() }
                };

                matches.push(serde_json::json!({
                    "id": node.id,
                    "doc_name": doc_name,
                    "type": format!("{:?}", node.node_type).to_lowercase(),
                    "content_preview": content_preview,
                    "children_count": node.children.len(),
                }));

                if matches.len() >= 10 {
                    break 'outer;
                }
            }
        }
    }

    serde_json::json!({
        "query": args["query"].as_str().unwrap_or(""),
        "count": matches.len(),
        "results": matches,
    })
}

fn tool_search_across_docs(args: &serde_json::Value, trees: &[DocumentTree]) -> serde_json::Value {
    let query = match args["query"].as_str() {
        Some(q) if !q.is_empty() => q.to_lowercase(),
        _ => return serde_json::json!({ "error": "query is required" }),
    };
    let max_per_doc = args["max_results"].as_u64().unwrap_or(5).min(20) as usize;

    let documents: Vec<serde_json::Value> = trees
        .iter()
        .map(|tree| {
            let mut results: Vec<serde_json::Value> = Vec::new();
            for node in tree.nodes.values() {
                if matches!(node.node_type, NodeType::Root) {
                    continue;
                }
                if node.content.to_lowercase().contains(&query) {
                    let lower = node.content.to_lowercase();
                    let pos = lower.find(&query).unwrap_or(0);
                    let start = pos.saturating_sub(60);
                    let end = (pos + query.len() + 60).min(node.content.len());
                    let snippet = &node.content[start..end];
                    let content_preview = if start > 0 { format!("…{}", snippet) } else { snippet.to_string() };

                    results.push(serde_json::json!({
                        "id": node.id,
                        "type": format!("{:?}", node.node_type).to_lowercase(),
                        "content_preview": content_preview,
                    }));
                    if results.len() >= max_per_doc {
                        break;
                    }
                }
            }
            serde_json::json!({
                "doc_name": tree.name,
                "doc_id": tree.id,
                "match_count": results.len(),
                "results": results,
            })
        })
        .collect();

    serde_json::json!({
        "query": args["query"].as_str().unwrap_or(""),
        "documents": documents,
    })
}

fn tool_get_relations(args: &serde_json::Value, trees: &[DocumentTree]) -> serde_json::Value {
    let node_id = match args["node_id"].as_str() {
        Some(id) => id,
        None => return serde_json::json!({ "error": "node_id is required" }),
    };

    match find_node_in_trees(node_id, trees) {
        None => serde_json::json!({ "error": format!("Node '{}' not found", node_id) }),
        Some((node, doc_name)) => {
            let relations: Vec<serde_json::Value> = node.relations
                .iter()
                .map(|r| serde_json::json!({
                    "target_id": r.target_id,
                    "relation_type": format!("{:?}", r.relation_type).to_lowercase(),
                    "label": r.label,
                }))
                .collect();
            serde_json::json!({
                "node_id": node_id,
                "doc_name": doc_name,
                "relations": relations,
            })
        }
    }
}

fn tool_get_node_context(args: &serde_json::Value, trees: &[DocumentTree]) -> serde_json::Value {
    let node_id = match args["node_id"].as_str() {
        Some(id) => id,
        None => return serde_json::json!({ "error": "node_id is required" }),
    };

    // Find which tree owns this node
    for tree in trees {
        if !tree.nodes.contains_key(node_id) {
            continue;
        }

        // Build parent map: child_id → parent_id
        let mut parent_map: HashMap<&str, &str> = HashMap::new();
        for (pid, pnode) in &tree.nodes {
            for cid in &pnode.children {
                parent_map.insert(cid.as_str(), pid.as_str());
            }
        }

        // Walk up from node_id to root, collecting ancestors
        let mut path: Vec<&str> = Vec::new();
        let mut current = node_id;
        let mut depth = 0;
        loop {
            path.push(current);
            if current == tree.root_id {
                break;
            }
            match parent_map.get(current) {
                Some(parent) => current = parent,
                None => break,
            }
            depth += 1;
            if depth > 50 { break; } // safety: prevent infinite loop on malformed tree
        }
        path.reverse(); // root first

        let context: Vec<serde_json::Value> = path
            .iter()
            .filter_map(|id| tree.nodes.get(*id))
            .map(|n| serde_json::json!({
                "id": n.id,
                "type": format!("{:?}", n.node_type).to_lowercase(),
                "content_preview": preview(&n.content, 80),
            }))
            .collect();

        return serde_json::json!({
            "node_id": node_id,
            "doc_name": tree.name,
            "ancestor_path": context,
        });
    }

    serde_json::json!({ "error": format!("Node '{}' not found", node_id) })
}

fn tool_compare_nodes(args: &serde_json::Value, trees: &[DocumentTree]) -> serde_json::Value {
    let id_a = match args["node_a"].as_str() {
        Some(id) => id,
        None => return serde_json::json!({ "error": "node_a is required" }),
    };
    let id_b = match args["node_b"].as_str() {
        Some(id) => id,
        None => return serde_json::json!({ "error": "node_b is required" }),
    };

    let node_a = find_node_in_trees(id_a, trees);
    let node_b = find_node_in_trees(id_b, trees);

    match (node_a, node_b) {
        (None, _) => serde_json::json!({ "error": format!("Node '{}' not found", id_a) }),
        (_, None) => serde_json::json!({ "error": format!("Node '{}' not found", id_b) }),
        (Some((na, doc_a)), Some((nb, doc_b))) => {
            let summary_a = na.summary.clone()
                .or_else(|| na.metadata.get("summary").and_then(|v| v.as_str()).map(String::from));
            let summary_b = nb.summary.clone()
                .or_else(|| nb.metadata.get("summary").and_then(|v| v.as_str()).map(String::from));

            serde_json::json!({
                "node_a": {
                    "id": id_a,
                    "doc_name": doc_a,
                    "type": format!("{:?}", na.node_type).to_lowercase(),
                    "content": na.content,
                    "summary": summary_a,
                },
                "node_b": {
                    "id": id_b,
                    "doc_name": doc_b,
                    "type": format!("{:?}", nb.node_type).to_lowercase(),
                    "content": nb.content,
                    "summary": summary_b,
                },
            })
        }
    }
}

fn tool_get_image(args: &serde_json::Value, trees: &[DocumentTree]) -> serde_json::Value {
    let node_id = match args["node_id"].as_str() {
        Some(id) => id,
        None => return serde_json::json!({ "error": "node_id is required" }),
    };

    match find_node_in_trees(node_id, trees) {
        None => serde_json::json!({ "error": format!("Node '{}' not found", node_id) }),
        Some((node, doc_name)) => {
            if !matches!(node.node_type, NodeType::Image) {
                return serde_json::json!({
                    "error": format!("Node '{}' is not an image node (type: {:?})", node_id, node.node_type)
                });
            }
            serde_json::json!({
                "id": node.id,
                "doc_name": doc_name,
                "raw_image_path": node.raw_image_path,
                "alt_text": node.content,
                "metadata": node.metadata,
            })
        }
    }
}

fn tool_record_relation(args: &serde_json::Value, db: &crate::db::Database) -> serde_json::Value {
    let source_node_id = match args["source_node_id"].as_str() {
        Some(id) if !id.is_empty() => id,
        _ => return serde_json::json!({ "error": "source_node_id is required" }),
    };
    let target_node_id = match args["target_node_id"].as_str() {
        Some(id) if !id.is_empty() => id,
        _ => return serde_json::json!({ "error": "target_node_id is required" }),
    };
    let relation_type = match args["relation_type"].as_str() {
        Some(rt) => rt,
        None => return serde_json::json!({ "error": "relation_type is required" }),
    };
    let description = args["description"].as_str().unwrap_or("").to_string();

    let record = crate::db::CrossDocRelation {
        id: uuid::Uuid::new_v4().to_string(),
        source_doc_id: String::new(), // will be populated by DB lookup if needed
        source_node_id: source_node_id.to_string(),
        target_doc_id: String::new(),
        target_node_id: target_node_id.to_string(),
        relation_type: relation_type.to_string(),
        confidence: 0.8, // LLM-discovered relations get high confidence
        description: Some(description),
        created_at: chrono::Utc::now().to_rfc3339(),
    };

    match db.save_cross_doc_relation(&record) {
        Ok(_) => serde_json::json!({ "ok": true, "id": record.id }),
        Err(e) => serde_json::json!({ "error": format!("Failed to save relation: {}", e) }),
    }
}
