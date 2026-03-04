/// Anthropic Claude provider — uses the native Anthropic Messages API.
/// Different from OpenAI-compatible providers: uses `x-api-key` header,
/// `anthropic-version` header, and a distinct request/response schema.
use async_trait::async_trait;
use serde_json::json;

use super::provider::{
    LLMError, LLMProvider, LLMResponse, Message, ProviderCapabilities, ProviderConfig, Tool,
    ToolCall,
};

pub struct AnthropicProvider {
    pub config: ProviderConfig,
    pub client: reqwest::Client,
}

impl AnthropicProvider {
    pub fn new(mut config: ProviderConfig) -> Self {
        if config.base_url.is_empty() {
            config.base_url = "https://api.anthropic.com/v1".to_string();
        }
        if config.model.is_empty() {
            config.model = "claude-sonnet-4-6".to_string();
        }
        Self {
            config,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl LLMProvider for AnthropicProvider {
    async fn chat(
        &self,
        messages: Vec<Message>,
        tools: Option<Vec<Tool>>,
    ) -> Result<LLMResponse, LLMError> {
        let api_key = self
            .config
            .api_key
            .as_ref()
            .map(|k| k.trim().to_string())
            .filter(|k| !k.is_empty())
            .ok_or_else(|| LLMError::NoApiKey("Anthropic".to_string()))?;

        let base = self.config.base_url.trim_end_matches('/');
        let url = format!("{}/messages", base);

        // Separate system prompt from conversation messages
        let mut system_content = String::new();
        let mut anthropic_messages: Vec<serde_json::Value> = Vec::new();

        for m in &messages {
            match m.role.as_str() {
                "system" => {
                    if !system_content.is_empty() {
                        system_content.push_str("\n\n");
                    }
                    system_content.push_str(&m.content);
                }
                "tool" => {
                    // Anthropic tool_result goes into a user turn with content blocks
                    let tool_call_id = m.tool_call_id.as_deref().unwrap_or("");
                    let tool_name = m.tool_name.as_deref().unwrap_or("unknown");
                    let parsed: serde_json::Value =
                        serde_json::from_str(&m.content).unwrap_or(json!({ "result": m.content }));
                    anthropic_messages.push(json!({
                        "role": "user",
                        "content": [{
                            "type": "tool_result",
                            "tool_use_id": tool_call_id,
                            "tool_name": tool_name,
                            "content": [{ "type": "text", "text": serde_json::to_string(&parsed).unwrap_or_default() }],
                        }]
                    }));
                }
                "assistant" => {
                    if let Some(ref tc_raw) = m.tool_calls_raw {
                        // Tool call: Anthropic uses tool_use content blocks
                        let mut content_blocks: Vec<serde_json::Value> = Vec::new();
                        if !m.content.is_empty() {
                            content_blocks.push(json!({ "type": "text", "text": m.content }));
                        }
                        // tc_raw is stored in OpenAI format; convert to Anthropic tool_use blocks
                        for tc in tc_raw {
                            let id = tc["id"].as_str().unwrap_or("").to_string();
                            let name = tc["function"]["name"].as_str().unwrap_or("").to_string();
                            let args_str = tc["function"]["arguments"].as_str().unwrap_or("{}");
                            let input: serde_json::Value =
                                serde_json::from_str(args_str).unwrap_or_default();
                            content_blocks.push(json!({
                                "type": "tool_use",
                                "id": id,
                                "name": name,
                                "input": input,
                            }));
                        }
                        anthropic_messages.push(json!({
                            "role": "assistant",
                            "content": content_blocks,
                        }));
                    } else {
                        let text = if m.content.is_empty() { " " } else { &m.content };
                        anthropic_messages.push(json!({
                            "role": "assistant",
                            "content": [{ "type": "text", "text": text }],
                        }));
                    }
                }
                "user" => {
                    anthropic_messages.push(json!({
                        "role": "user",
                        "content": m.content,
                    }));
                }
                _ => {}
            }
        }

        let mut body = json!({
            "model": self.config.model,
            "max_tokens": 8192,
            "messages": anthropic_messages,
        });

        if !system_content.is_empty() {
            body["system"] = json!(system_content);
        }

        if let Some(tools) = tools {
            let anthropic_tools: Vec<serde_json::Value> = tools
                .iter()
                .map(|t| {
                    json!({
                        "name": t.name,
                        "description": t.description,
                        "input_schema": t.parameters,
                    })
                })
                .collect();
            body["tools"] = json!(anthropic_tools);
        }

        let response = self
            .client
            .post(&url)
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(LLMError::ApiError(format!(
                "Anthropic API error {}: {}",
                status, text
            )));
        }

        let resp_json: serde_json::Value = response.json().await?;

        // Anthropic response: { content: [{type, text/id/name/input}], stop_reason, usage }
        let content_blocks = resp_json["content"].as_array();

        let mut text_content: Option<String> = None;
        let mut tool_calls: Vec<ToolCall> = Vec::new();
        let mut raw_tool_calls: Vec<serde_json::Value> = Vec::new();

        if let Some(blocks) = content_blocks {
            for block in blocks {
                match block["type"].as_str() {
                    Some("text") => {
                        if let Some(t) = block["text"].as_str() {
                            match &mut text_content {
                                Some(existing) => {
                                    existing.push_str("\n\n");
                                    existing.push_str(t);
                                }
                                None => text_content = Some(t.to_string()),
                            }
                        }
                    }
                    Some("tool_use") => {
                        let call_id = block["id"].as_str().unwrap_or("").to_string();
                        let name = block["name"].as_str().unwrap_or("").to_string();
                        let input = block["input"].clone();

                        // Store in OpenAI-compatible format for our conversation history
                        let args_str = serde_json::to_string(&input).unwrap_or_else(|_| "{}".to_string());
                        raw_tool_calls.push(json!({
                            "id": call_id,
                            "type": "function",
                            "function": {
                                "name": name,
                                "arguments": args_str,
                            }
                        }));

                        tool_calls.push(ToolCall {
                            id: call_id,
                            name,
                            arguments: input,
                        });
                    }
                    _ => {}
                }
            }
        }

        let input_tokens = resp_json["usage"]["input_tokens"].as_u64().unwrap_or(0) as u32;
        let output_tokens = resp_json["usage"]["output_tokens"].as_u64().unwrap_or(0) as u32;
        let tokens_used = input_tokens + output_tokens;

        Ok(LLMResponse {
            content: text_content,
            tool_calls,
            raw_tool_calls,
            tokens_used,
        })
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            supports_vision: true,
            supports_tool_calling: true,
            max_context_tokens: 200_000,
            supports_streaming: false,
        }
    }

    fn name(&self) -> &str {
        "anthropic"
    }
}
