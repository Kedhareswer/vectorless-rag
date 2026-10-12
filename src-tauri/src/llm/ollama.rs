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
        // A server on this machine must not be routed through a system proxy.
        let is_local = ["://localhost", "://127.0.0.1", "://[::1]"]
            .iter()
            .any(|h| config.base_url.contains(h));
        let client = if is_local {
            reqwest::Client::builder().no_proxy().build().unwrap_or_else(|_| reqwest::Client::new())
        } else {
            reqwest::Client::new()
        };
        Self { config, client }
    }
}

/// Context window requested from Ollama. Its default is only a few thousand
/// tokens and longer prompts are silently truncated from the front, which would
/// drop the system prompt and document content (up to ~10k tokens per query).
const CONTEXT_TOKENS: usize = 16_384;

fn to_ollama_messages(messages: &[Message]) -> Vec<serde_json::Value> {
    messages
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
        .collect()
}

/// One parsed line of Ollama's NDJSON chat stream.
#[derive(Debug, Default, PartialEq)]
struct StreamChunk {
    content: Option<String>,
    done: bool,
    input_tokens: u32,
    output_tokens: u32,
    error: Option<String>,
}

fn parse_stream_line(line: &str) -> Option<StreamChunk> {
    let v: serde_json::Value = serde_json::from_str(line.trim()).ok()?;
    if let Some(err) = v["error"].as_str() {
        return Some(StreamChunk { error: Some(err.to_string()), ..Default::default() });
    }
    Some(StreamChunk {
        content: v["message"]["content"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string()),
        done: v["done"].as_bool().unwrap_or(false),
        input_tokens: v["prompt_eval_count"].as_u64().unwrap_or(0) as u32,
        output_tokens: v["eval_count"].as_u64().unwrap_or(0) as u32,
        error: None,
    })
}

#[async_trait]
impl LLMProvider for OllamaProvider {
    async fn chat(
        &self,
        messages: Vec<Message>,
        tools: Option<Vec<Tool>>,
    ) -> Result<LLMResponse, LLMError> {
        let url = format!("{}/api/chat", self.config.base_url);

        let ollama_messages = to_ollama_messages(&messages);

        let mut body = json!({
            "model": self.config.model,
            "messages": ollama_messages,
            "stream": false,
            "options": { "num_ctx": CONTEXT_TOKENS },
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

    /// Streams tokens from `/api/chat` (NDJSON, one JSON object per line).
    /// Tool calls are not streamed, so requests with tools use `chat()`.
    async fn chat_stream(
        &self,
        messages: Vec<Message>,
        tools: Option<Vec<Tool>>,
        token_tx: tokio::sync::mpsc::UnboundedSender<String>,
    ) -> Result<LLMResponse, LLMError> {
        if tools.is_some() {
            let response = self.chat(messages, tools).await?;
            if let Some(ref content) = response.content {
                let _ = token_tx.send(content.clone());
            }
            return Ok(response);
        }

        let url = format!("{}/api/chat", self.config.base_url);
        let body = json!({
            "model": self.config.model,
            "messages": to_ollama_messages(&messages),
            "stream": true,
            "options": { "num_ctx": CONTEXT_TOKENS },
        });

        let response = self.client.post(&url).json(&body).send().await?;
        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(LLMError::ApiError(format!(
                "Ollama API error {}: {}",
                status, text
            )));
        }

        use futures_util::StreamExt;
        use tokio::io::AsyncBufReadExt;
        let reader = tokio_util::io::StreamReader::new(
            response
                .bytes_stream()
                .map(|r| r.map_err(std::io::Error::other)),
        );
        let mut lines = reader.lines();

        let mut content = String::new();
        let mut input_tokens = 0u32;
        let mut output_tokens = 0u32;

        while let Some(line) = lines
            .next_line()
            .await
            .map_err(|e| LLMError::ApiError(format!("Ollama stream error: {}", e)))?
        {
            let Some(chunk) = parse_stream_line(&line) else {
                continue;
            };
            if let Some(err) = chunk.error {
                return Err(LLMError::ApiError(format!("Ollama error: {}", err)));
            }
            if let Some(text) = chunk.content {
                let _ = token_tx.send(text.clone());
                content.push_str(&text);
            }
            if chunk.done {
                input_tokens = chunk.input_tokens;
                output_tokens = chunk.output_tokens;
                break;
            }
        }

        Ok(LLMResponse {
            content: if content.is_empty() { None } else { Some(content) },
            tool_calls: Vec::new(),
            raw_tool_calls: Vec::new(),
            tokens_used: input_tokens + output_tokens,
            input_tokens,
            output_tokens,
        })
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            supports_vision: false,
            supports_tool_calling: true,
            max_context_tokens: CONTEXT_TOKENS,
            supports_streaming: true,
        }
    }

    fn name(&self) -> &str {
        "ollama"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_stream_line_reads_content_and_final_counts() {
        let chunk = parse_stream_line(r#"{"message":{"role":"assistant","content":"Hel"},"done":false}"#).unwrap();
        assert_eq!(chunk.content.as_deref(), Some("Hel"));
        assert!(!chunk.done);

        let last = parse_stream_line(
            r#"{"message":{"role":"assistant","content":""},"done":true,"prompt_eval_count":12,"eval_count":3}"#,
        )
        .unwrap();
        assert_eq!(last.content, None);
        assert!(last.done);
        assert_eq!((last.input_tokens, last.output_tokens), (12, 3));
    }

    #[test]
    fn parse_stream_line_surfaces_errors_and_skips_garbage() {
        let err = parse_stream_line(r#"{"error":"model not found"}"#).unwrap();
        assert_eq!(err.error.as_deref(), Some("model not found"));
        assert!(parse_stream_line("not json").is_none());
    }

    #[tokio::test]
    async fn chat_stream_forwards_tokens_from_ndjson() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 16 * 1024];
            let _ = socket.read(&mut buf).await.unwrap();
            let body = concat!(
                r#"{"message":{"role":"assistant","content":"Hello"},"done":false}"#, "\n",
                r#"{"message":{"role":"assistant","content":", world"},"done":false}"#, "\n",
                r#"{"message":{"role":"assistant","content":""},"done":true,"prompt_eval_count":7,"eval_count":2}"#, "\n",
            );
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/x-ndjson\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            socket.write_all(response.as_bytes()).await.unwrap();
        });

        let provider = OllamaProvider::new(ProviderConfig {
            id: "test".to_string(),
            name: "ollama".to_string(),
            api_key: None,
            base_url: format!("http://{}", addr),
            model: "llama3".to_string(),
            is_active: true,
        });
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let response = provider
            .chat_stream(vec![Message::text("user", "hi")], None, tx)
            .await
            .unwrap();

        let mut tokens = Vec::new();
        while let Ok(t) = rx.try_recv() {
            tokens.push(t);
        }
        assert_eq!(tokens, vec!["Hello", ", world"]);
        assert_eq!(response.content.as_deref(), Some("Hello, world"));
        assert_eq!((response.input_tokens, response.output_tokens), (7, 2));
    }
}
