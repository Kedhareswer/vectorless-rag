use async_trait::async_trait;
use serde_json::json;

use super::provider::{
    LLMError, LLMProvider, LLMResponse, Message, ProviderCapabilities, ProviderConfig, Tool,
    ToolCall,
};

/// AgentRouter is an Anthropic-compatible API proxy.
/// It uses the Anthropic Messages API format with `x-api-key` auth.
pub struct AgentRouterProvider {
    pub config: ProviderConfig,
    pub client: reqwest::Client,
}

impl AgentRouterProvider {
    pub fn new(mut config: ProviderConfig) -> Self {
        if config.base_url.is_empty() {
            config.base_url = "https://agentrouter.org/v1".to_string();
        }
        // Default to a Claude model since AgentRouter proxies to Anthropic
        if config.model.is_empty() {
            config.model = "claude-sonnet-4-5-20250514".to_string();
        }
        Self {
            config,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl LLMProvider for AgentRouterProvider {
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
            .ok_or_else(|| LLMError::NoApiKey("AgentRouter".to_string()))?;

        let base = self.config.base_url.trim_end_matches('/');
        let url = format!("{}/messages", base);

        // Separate system messages from conversation messages (Anthropic format)
        let mut system_parts: Vec<String> = Vec::new();
        let mut anthropic_messages: Vec<serde_json::Value> = Vec::new();

        for m in &messages {
            match m.role.as_str() {
                "system" => {
                    system_parts.push(m.content.clone());
                }
                "assistant" | "model" => {
                    if let Some(ref tc_raw) = m.tool_calls_raw {
                        // Convert OpenAI-style tool_calls to Anthropic tool_use blocks
                        let mut content_blocks: Vec<serde_json::Value> = Vec::new();
                        if !m.content.is_empty() {
                            content_blocks.push(json!({ "type": "text", "text": m.content }));
                        }
                        for tc in tc_raw {
                            // tc may be OpenAI format {id, type, function:{name,arguments}}
                            // or a direct tool call object — normalise to Anthropic format
                            let (call_id, name, input) = if tc.get("function").is_some() {
                                let id = tc["id"].as_str().unwrap_or("").to_string();
                                let fn_name = tc["function"]["name"]
                                    .as_str()
                                    .unwrap_or("")
                                    .to_string();
                                let args_str =
                                    tc["function"]["arguments"].as_str().unwrap_or("{}");
                                let input: serde_json::Value =
                                    serde_json::from_str(args_str).unwrap_or_default();
                                (id, fn_name, input)
                            } else {
                                // Already Anthropic-style or unknown — pass through
                                let id = tc["id"].as_str().unwrap_or("").to_string();
                                let name =
                                    tc["name"].as_str().unwrap_or("").to_string();
                                let input = tc["input"].clone();
                                (id, name, input)
                            };
                            content_blocks.push(json!({
                                "type": "tool_use",
                                "id": call_id,
                                "name": name,
                                "input": input,
                            }));
                        }
                        anthropic_messages.push(json!({
                            "role": "assistant",
                            "content": content_blocks,
                        }));
                    } else {
                        anthropic_messages.push(json!({
                            "role": "assistant",
                            "content": m.content,
                        }));
                    }
                }
                "tool" => {
                    // Anthropic tool results live inside a user message as tool_result blocks.
                    // Merge consecutive tool results into a single user message.
                    let tool_call_id = m.tool_call_id.as_deref().unwrap_or("").to_string();
                    let result_block = json!({
                        "type": "tool_result",
                        "tool_use_id": tool_call_id,
                        "content": m.content,
                    });

                    // Append to the last user message if it already contains tool_result blocks,
                    // otherwise start a new user message.
                    if let Some(last) = anthropic_messages.last_mut() {
                        if last["role"] == "user" {
                            if let Some(arr) = last["content"].as_array_mut() {
                                if arr.iter().any(|b| b["type"] == "tool_result") {
                                    arr.push(result_block);
                                    continue;
                                }
                            }
                        }
                    }
                    anthropic_messages.push(json!({
                        "role": "user",
                        "content": [result_block],
                    }));
                }
                _ => {
                    // user or other
                    anthropic_messages.push(json!({
                        "role": "user",
                        "content": m.content,
                    }));
                }
            }
        }

        let mut body = json!({
            "model": self.config.model,
            "max_tokens": 8192,
            "messages": anthropic_messages,
        });

        if !system_parts.is_empty() {
            body["system"] = json!(system_parts.join("\n\n"));
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
            .header("x-api-key", &api_key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            let hint = if status.as_u16() == 401 {
                " — API key rejected. Generate a new key at https://agentrouter.org/console/token"
            } else if status.as_u16() == 429 {
                " — Rate limited. Please wait and try again."
            } else if status.as_u16() == 403 {
                " — Access forbidden. Check your AgentRouter account and key permissions."
            } else {
                ""
            };
            return Err(LLMError::ApiError(format!(
                "AgentRouter API error {}: {}{}",
                status, text, hint
            )));
        }

        let resp_json: serde_json::Value = response.json().await?;

        // Anthropic Messages API response format
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
                                None => {
                                    text_content = Some(t.to_string());
                                }
                            }
                        }
                    }
                    Some("tool_use") => {
                        // Store as OpenAI-compatible raw format for our conversation history
                        let call_id = block["id"].as_str().unwrap_or("").to_string();
                        let name = block["name"].as_str().unwrap_or("").to_string();
                        let input = block["input"].clone();

                        // Keep raw in Anthropic format for re-sending
                        raw_tool_calls.push(block.clone());

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
            input_tokens,
            output_tokens,
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
        "agentrouter"
    }
}
