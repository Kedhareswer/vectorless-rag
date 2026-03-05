use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum LLMError {
    #[error("HTTP request failed: {0}")]
    RequestError(#[from] reqwest::Error),
    #[error("API error: {0}")]
    ApiError(String),
    #[error("No API key configured for provider: {0}")]
    NoApiKey(String),
    #[error("Deserialization error: {0}")]
    DeserializeError(String),
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Message {
    pub role: String,
    pub content: String,
    /// For assistant messages that include tool calls (OpenAI format)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls_raw: Option<Vec<serde_json::Value>>,
    /// For tool result messages (role="tool"), the tool call ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// For tool result messages, the tool name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
}

impl Message {
    /// Create a simple text message (system, user, or assistant).
    pub fn text(role: &str, content: &str) -> Self {
        Self {
            role: role.to_string(),
            content: content.to_string(),
            tool_calls_raw: None,
            tool_call_id: None,
            tool_name: None,
        }
    }

    /// Create an assistant message that requested tool calls.
    /// `tool_calls_raw` stores the raw JSON from the API response so it can be echoed back.
    pub fn assistant_with_tool_calls(content: Option<&str>, tool_calls_raw: Vec<serde_json::Value>) -> Self {
        Self {
            role: "assistant".to_string(),
            content: content.unwrap_or("").to_string(),
            tool_calls_raw: Some(tool_calls_raw),
            tool_call_id: None,
            tool_name: None,
        }
    }

    /// Create a tool result message (OpenAI format: role="tool").
    pub fn tool_result(tool_call_id: &str, tool_name: &str, content: &str) -> Self {
        Self {
            role: "tool".to_string(),
            content: content.to_string(),
            tool_calls_raw: None,
            tool_call_id: Some(tool_call_id.to_string()),
            tool_name: Some(tool_name.to_string()),
        }
    }
}

#[derive(Serialize, Clone, Debug)]
pub struct Tool {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Deserialize, Clone, Debug)]
pub struct ToolCall {
    /// The unique ID for this tool call (from OpenAI-format APIs).
    /// Used to correlate tool results back to tool calls.
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

#[derive(Clone, Debug)]
pub struct LLMResponse {
    pub content: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    /// Raw JSON of tool_calls from the API response, for echoing back in conversation history.
    pub raw_tool_calls: Vec<serde_json::Value>,
    pub tokens_used: u32,
    pub input_tokens: u32,
    pub output_tokens: u32,
}

#[derive(Serialize, Clone, Debug)]
pub struct ProviderCapabilities {
    pub supports_vision: bool,
    pub supports_tool_calling: bool,
    pub max_context_tokens: usize,
    pub supports_streaming: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ProviderConfig {
    pub id: String,
    pub name: String,
    pub api_key: Option<String>,
    pub base_url: String,
    pub model: String,
    pub is_active: bool,
}

#[async_trait]
pub trait LLMProvider: Send + Sync {
    async fn chat(
        &self,
        messages: Vec<Message>,
        tools: Option<Vec<Tool>>,
    ) -> Result<LLMResponse, LLMError>;

    fn capabilities(&self) -> ProviderCapabilities;

    fn name(&self) -> &str;
}
