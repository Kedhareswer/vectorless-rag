use async_trait::async_trait;
use serde_json::json;

use super::provider::{
    LLMError, LLMProvider, LLMResponse, Message, ProviderCapabilities, ProviderConfig, Tool,
    ToolCall,
};

pub struct GroqProvider {
    pub config: ProviderConfig,
    pub client: reqwest::Client,
}

impl GroqProvider {
    pub fn new(mut config: ProviderConfig) -> Self {
        if config.base_url.is_empty() {
            config.base_url = "https://api.groq.com/openai/v1".to_string();
        }
        Self {
            config,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl LLMProvider for GroqProvider {
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
            .ok_or_else(|| LLMError::NoApiKey("Groq".to_string()))?;

        let base = self.config.base_url.trim_end_matches('/');
        let url = format!("{}/chat/completions", base);

        let openai_messages: Vec<serde_json::Value> = messages
            .iter()
            .map(|m| {
                if m.role == "tool" {
                    // Tool result message
                    json!({
                        "role": "tool",
                        "tool_call_id": m.tool_call_id.as_deref().unwrap_or(""),
                        "content": m.content,
                    })
                } else if let Some(ref tc_raw) = m.tool_calls_raw {
                    // Assistant message that requested tool calls
                    let mut msg = json!({
                        "role": "assistant",
                        "tool_calls": tc_raw,
                    });
                    if !m.content.is_empty() {
                        msg["content"] = json!(m.content);
                    }
                    msg
                } else {
                    json!({
                        "role": m.role,
                        "content": m.content,
                    })
                }
            })
            .collect();

        let mut body = json!({
            "model": self.config.model,
            "messages": openai_messages,
        });

        if let Some(tools) = tools {
            let openai_tools: Vec<serde_json::Value> = tools
                .iter()
                .map(|t| {
                    json!({
                        "type": "function",
                        "function": {
                            "name": t.name,
                            "description": t.description,
                            "parameters": t.parameters,
                        }
                    })
                })
                .collect();
            body["tools"] = json!(openai_tools);
        }

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(LLMError::ApiError(format!(
                "Groq API error {}: {}",
                status, text
            )));
        }

        let resp_json: serde_json::Value = response.json().await?;

        let choice = &resp_json["choices"][0]["message"];
        let content = choice["content"].as_str().map(|s| s.to_string());

        let mut tool_calls = Vec::new();
        let mut raw_tool_calls = Vec::new();
        if let Some(calls) = choice["tool_calls"].as_array() {
            for call in calls {
                raw_tool_calls.push(call.clone());
                let call_id = call["id"].as_str().unwrap_or("").to_string();
                if let Some(name) = call["function"]["name"].as_str() {
                    let args_str = call["function"]["arguments"]
                        .as_str()
                        .unwrap_or("{}");
                    let arguments: serde_json::Value =
                        serde_json::from_str(args_str).unwrap_or_default();
                    tool_calls.push(ToolCall {
                        id: call_id,
                        name: name.to_string(),
                        arguments,
                    });
                }
            }
        }

        let input_tokens = resp_json["usage"]["prompt_tokens"].as_u64().unwrap_or(0) as u32;
        let output_tokens = resp_json["usage"]["completion_tokens"].as_u64().unwrap_or(0) as u32;
        let tokens_used = resp_json["usage"]["total_tokens"].as_u64().unwrap_or(0) as u32;

        Ok(LLMResponse {
            content,
            tool_calls,
            raw_tool_calls,
            tokens_used,
            input_tokens,
            output_tokens,
        })
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            supports_vision: false,
            supports_tool_calling: true,
            max_context_tokens: 32768,
            supports_streaming: true,
        }
    }

    fn name(&self) -> &str {
        "groq"
    }
}
