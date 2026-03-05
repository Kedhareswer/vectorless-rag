use async_trait::async_trait;
use serde_json::json;

use super::provider::{
    LLMError, LLMProvider, LLMResponse, Message, ProviderCapabilities, ProviderConfig, Tool,
    ToolCall,
};

pub struct OllamaProvider {
    pub config: ProviderConfig,
    pub client: reqwest::Client,
}

impl OllamaProvider {
    pub fn new(mut config: ProviderConfig) -> Self {
        if config.base_url.is_empty() {
            config.base_url = "http://localhost:11434".to_string();
        }
        Self {
            config,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl LLMProvider for OllamaProvider {
    async fn chat(
        &self,
        messages: Vec<Message>,
        tools: Option<Vec<Tool>>,
    ) -> Result<LLMResponse, LLMError> {
        let url = format!("{}/api/chat", self.config.base_url);

        let ollama_messages: Vec<serde_json::Value> = messages
            .iter()
            .map(|m| {
                if m.role == "tool" {
                    // Ollama expects tool results as role "tool" with content
                    json!({
                        "role": "tool",
                        "content": m.content,
                    })
                } else if let Some(ref tc_raw) = m.tool_calls_raw {
                    // Assistant message that requested tool calls
                    json!({
                        "role": "assistant",
                        "content": m.content,
                        "tool_calls": tc_raw,
                    })
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
            "messages": ollama_messages,
            "stream": false,
        });

        if let Some(tools) = tools {
            let ollama_tools: Vec<serde_json::Value> = tools
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
            body["tools"] = json!(ollama_tools);
        }

        let response = self.client.post(&url).json(&body).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(LLMError::ApiError(format!(
                "Ollama API error {}: {}",
                status, text
            )));
        }

        let resp_json: serde_json::Value = response.json().await?;

        let content = resp_json["message"]["content"].as_str().map(|s| s.to_string());

        let mut tool_calls = Vec::new();
        let mut raw_tool_calls = Vec::new();
        if let Some(calls) = resp_json["message"]["tool_calls"].as_array() {
            for (i, call) in calls.iter().enumerate() {
                raw_tool_calls.push(call.clone());
                if let (Some(name), Some(args)) = (
                    call["function"]["name"].as_str(),
                    call["function"]["arguments"].as_object(),
                ) {
                    // Ollama doesn't provide tool call IDs, so generate one
                    let call_id = format!("ollama_call_{}", i);
                    tool_calls.push(ToolCall {
                        id: call_id,
                        name: name.to_string(),
                        arguments: serde_json::Value::Object(args.clone()),
                    });
                }
            }
        }

        let input_tokens = resp_json["prompt_eval_count"].as_u64().unwrap_or(0) as u32;
        let output_tokens = resp_json["eval_count"].as_u64().unwrap_or(0) as u32;
        let tokens_used = input_tokens + output_tokens;

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
            max_context_tokens: 8192,
            supports_streaming: true,
        }
    }

    fn name(&self) -> &str {
        "ollama"
    }
}
