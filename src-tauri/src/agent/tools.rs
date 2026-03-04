use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
            _ => None,
        }
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
            description: "Get the top-level structure of a document tree, showing immediate children of the root with content previews.".to_string(),
            parameters_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "doc_id": {
                        "type": "string",
                        "description": "The document ID to get the overview of"
                    }
                },
                "required": ["doc_id"]
            }),
        },
        ToolDefinition {
            name: "expand_node".to_string(),
            description: "Expand a specific node to see its full content and immediate children.".to_string(),
            parameters_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "node_id": {
                        "type": "string",
                        "description": "The node ID to expand"
                    }
                },
                "required": ["node_id"]
            }),
        },
        ToolDefinition {
            name: "search_content".to_string(),
            description: "Search for text content within document nodes using substring matching.".to_string(),
            parameters_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "The search query string"
                    },
                    "scope": {
                        "type": "string",
                        "description": "Optional node ID to limit search scope"
                    }
                },
                "required": ["query"]
            }),
        },
        ToolDefinition {
            name: "get_relations".to_string(),
            description: "Get all relations (references, dependencies, links) for a specific node.".to_string(),
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
            description: "Compare the content and metadata of two nodes side by side.".to_string(),
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
            description: "Get the parent chain (ancestor path) from root to a specific node.".to_string(),
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
    ]
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
