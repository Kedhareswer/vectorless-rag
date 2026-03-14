/// Generic OpenAI-compatible provider.
/// Used for: OpenAI, DeepSeek, xAI (Grok), Alibaba Qwen, and any other
/// provider that exposes a standard `/v1/chat/completions` endpoint.
use async_trait::async_trait;
use serde_json::json;
use tokio::io::AsyncBufReadExt;

use super::provider::{
    LLMError, LLMProvider, LLMResponse, Message, ProviderCapabilities, ProviderConfig, Tool,
    ToolCall,
};

pub struct OpenAICompatProvider {
    pub config: ProviderConfig,
    pub client: reqwest::Client,
    /// Display name used in error messages.
    pub display_name: String,
}

impl OpenAICompatProvider {
    pub fn new(mut config: ProviderConfig, display_name: &str, default_base_url: &str) -> Self {
        if config.base_url.is_empty() {
            config.base_url = default_base_url.to_string();
        }
        Self {
            config,
            client: reqwest::Client::new(),
            display_name: display_name.to_string(),
        }
    }

    fn get_api_key(&self) -> Result<String, LLMError> {
        self.config
            .api_key
            .as_ref()
            .map(|k| k.trim().to_string())
            .filter(|k| !k.is_empty())
            .ok_or_else(|| LLMError::NoApiKey(self.display_name.clone()))
    }

    fn build_messages(messages: &[Message]) -> Vec<serde_json::Value> {
        messages
            .iter()
            .map(|m| {
                if m.role == "tool" {
                    json!({
                        "role": "tool",
                        "tool_call_id": m.tool_call_id.as_deref().unwrap_or(""),
                        "content": m.content,
                    })
                } else if let Some(ref tc_raw) = m.tool_calls_raw {
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
            .collect()
    }

    fn build_tools(tools: &[Tool]) -> Vec<serde_json::Value> {
        tools
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
            .collect()
    }
}

#[async_trait]
impl LLMProvider for OpenAICompatProvider {
    async fn chat(
        &self,
        messages: Vec<Message>,
        tools: Option<Vec<Tool>>,
    ) -> Result<LLMResponse, LLMError> {
        let api_key = self.get_api_key()?;
        let base = self.config.base_url.trim_end_matches('/');
        let url = format!("{}/chat/completions", base);

        let mut body = json!({
            "model": self.config.model,
            "messages": Self::build_messages(&messages),
        });

        if let Some(ref tools) = tools {
            body["tools"] = json!(Self::build_tools(tools));
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
                "{} API error {}: {}",
                self.display_name, status, text
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
                    let args_str = call["function"]["arguments"].as_str().unwrap_or("{}");
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

    async fn chat_stream(
        &self,
        messages: Vec<Message>,
        tools: Option<Vec<Tool>>,
        token_tx: tokio::sync::mpsc::UnboundedSender<String>,
    ) -> Result<LLMResponse, LLMError> {
        let api_key = self.get_api_key()?;
        let base = self.config.base_url.trim_end_matches('/');
        let url = format!("{}/chat/completions", base);

        let mut body = json!({
            "model": self.config.model,
            "messages": Self::build_messages(&messages),
            "stream": true,
            "stream_options": { "include_usage": true },
        });

        if let Some(ref tools) = tools {
            body["tools"] = json!(Self::build_tools(tools));
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
                "{} API error {}: {}",
                self.display_name, status, text
            )));
        }

        // Parse SSE stream
        let byte_stream = response.bytes_stream();
        use futures_util::StreamExt;
        let reader = tokio_util::io::StreamReader::new(
            byte_stream.map(|r| r.map_err(std::io::Error::other)),
        );
        let mut lines = reader.lines();

        let mut content_parts: Vec<String> = Vec::new();
        // Tool call accumulators: index -> (id, name, arguments_str)
        let mut tc_accum: std::collections::HashMap<usize, (String, String, String)> =
            std::collections::HashMap::new();
        let mut input_tokens = 0u32;
        let mut output_tokens = 0u32;
        let mut tokens_used = 0u32;

        while let Ok(Some(line)) = lines.next_line().await {
            let line = line.trim().to_string();
            if line.is_empty() || line.starts_with(':') {
                continue;
            }
            if !line.starts_with("data: ") {
                continue;
            }
            let data = &line[6..];
            if data == "[DONE]" {
                break;
            }

            let chunk: serde_json::Value = match serde_json::from_str(data) {
                Ok(v) => v,
                Err(_) => continue,
            };

            // Extract usage from the final chunk (stream_options.include_usage)
            if let Some(usage) = chunk.get("usage") {
                if !usage.is_null() {
                    input_tokens = usage["prompt_tokens"].as_u64().unwrap_or(0) as u32;
                    output_tokens = usage["completion_tokens"].as_u64().unwrap_or(0) as u32;
                    tokens_used = usage["total_tokens"].as_u64().unwrap_or(0) as u32;
                }
            }

            let delta = &chunk["choices"][0]["delta"];

            // Content tokens
            if let Some(text) = delta["content"].as_str() {
                if !text.is_empty() {
                    let owned = text.to_string();
                    let _ = token_tx.send(owned.clone());
                    content_parts.push(owned);
                }
            }

            // Tool call deltas
            if let Some(tool_calls) = delta["tool_calls"].as_array() {
                for tc_delta in tool_calls {
                    let idx = tc_delta["index"].as_u64().unwrap_or(0) as usize;
                    let entry = tc_accum.entry(idx).or_insert_with(|| {
                        let id = tc_delta["id"].as_str().unwrap_or("").to_string();
                        let name = tc_delta["function"]["name"]
                            .as_str()
                            .unwrap_or("")
                            .to_string();
                        (id, name, String::new())
                    });
                    if let Some(args_chunk) = tc_delta["function"]["arguments"].as_str() {
                        entry.2.push_str(args_chunk);
                    }
                }
            }
        }

        let content = if content_parts.is_empty() {
            None
        } else {
            Some(content_parts.concat())
        };

        let mut tool_calls = Vec::new();
        let mut raw_tool_calls = Vec::new();
        let mut indices: Vec<usize> = tc_accum.keys().copied().collect();
        indices.sort();
        for idx in indices {
            if let Some((id, name, args_str)) = tc_accum.remove(&idx) {
                let arguments: serde_json::Value =
                    serde_json::from_str(&args_str).unwrap_or_default();
                raw_tool_calls.push(json!({
                    "id": id,
                    "type": "function",
                    "function": {
                        "name": name,
                        "arguments": args_str,
                    }
                }));
                tool_calls.push(ToolCall {
                    id,
                    name,
                    arguments,
                });
            }
        }

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
            supports_vision: true,
            supports_tool_calling: true,
            max_context_tokens: 128_000,
            supports_streaming: true,
        }
    }

    fn name(&self) -> &str {
        &self.config.name
    }
}
