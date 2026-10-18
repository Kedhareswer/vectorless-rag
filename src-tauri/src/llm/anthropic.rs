/// Anthropic Claude provider — uses the native Anthropic Messages API.
/// Different from OpenAI-compatible providers: uses `x-api-key` header,
/// `anthropic-version` header, and a distinct request/response schema.
use async_trait::async_trait;
use serde_json::json;
use tokio::io::AsyncBufReadExt;

use super::provider::{
    LLMError, LLMProvider, LLMResponse, Message, ProviderCapabilities, ProviderConfig, Tool,
    ToolCall,
};

/// A user message's `content` for the Messages API.
///
/// A plain string unless the message carries a cache breakpoint, in which case
/// it becomes a single text block marked `cache_control: ephemeral`. Anthropic
/// then caches the whole prompt prefix up to that block — system prompt,
/// history and the fetched document content — for five minutes, and a repeat
/// of that prefix is billed at a tenth of the input rate. Prompts under the
/// per-model minimum (1024 tokens, 2048 for Haiku) are simply not cached.
fn user_content(m: &Message) -> serde_json::Value {
    if m.cache {
        json!([{
            "type": "text",
            "text": m.content,
            "cache_control": { "type": "ephemeral" },
        }])
    } else {
        json!(m.content)
    }
}

/// Request body for the Messages API: the system prompt split out, and
/// OpenAI-style tool messages converted to Anthropic content blocks. Shared by
/// `chat`, `chat_stream` and the Batch API, which sends this body as `params`.
pub(crate) fn request_body(model: &str, messages: &[Message]) -> serde_json::Value {
    let mut system_content = String::new();
    let mut anthropic_messages: Vec<serde_json::Value> = Vec::new();

    for m in messages {
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
                    "content": user_content(m),
                }));
            }
            _ => {}
        }
    }

    let mut body = json!({
        "model": model,
        "max_tokens": 8192,
        "messages": anthropic_messages,
    });
    if !system_content.is_empty() {
        body["system"] = json!(system_content);
    }
    body
}

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

        let mut body = request_body(&self.config.model, &messages);

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
        // Cached tokens are reported separately and are NOT part of
        // input_tokens, so the total has to add them back in.
        let cache_write_tokens = resp_json["usage"]["cache_creation_input_tokens"]
            .as_u64()
            .unwrap_or(0) as u32;
        let cache_read_tokens = resp_json["usage"]["cache_read_input_tokens"]
            .as_u64()
            .unwrap_or(0) as u32;
        let tokens_used = input_tokens + output_tokens + cache_write_tokens + cache_read_tokens;

        Ok(LLMResponse {
            content: text_content,
            tool_calls,
            raw_tool_calls,
            tokens_used,
            input_tokens,
            output_tokens,
            cache_write_tokens,
            cache_read_tokens,
        })
    }

    async fn chat_stream(
        &self,
        messages: Vec<Message>,
        tools: Option<Vec<Tool>>,
        token_tx: tokio::sync::mpsc::UnboundedSender<String>,
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

        let mut body = request_body(&self.config.model, &messages);
        body["stream"] = json!(true);

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

        // Parse Anthropic SSE stream
        // Events of interest:
        //   content_block_delta  → delta.type="text_delta", delta.text
        //   message_delta        → usage.output_tokens (final)
        //   message_start        → message.usage.input_tokens (first event)
        let byte_stream = response.bytes_stream();
        use futures_util::StreamExt;
        let reader = tokio_util::io::StreamReader::new(
            byte_stream.map(|r| r.map_err(std::io::Error::other)),
        );
        let mut lines = reader.lines();

        let mut content_parts: Vec<String> = Vec::new();
        // tool_use blocks accumulated during streaming: index → (id, name, partial_json)
        let mut tool_blocks: std::collections::HashMap<usize, (String, String, String)> =
            std::collections::HashMap::new();
        let mut input_tokens = 0u32;
        let mut cache_write_tokens = 0u32;
        let mut cache_read_tokens = 0u32;
        let mut output_tokens = 0u32;
        let mut current_event_type = String::new();

        while let Ok(Some(line)) = lines.next_line().await {
            let line = line.trim().to_string();

            if let Some(event) = line.strip_prefix("event: ") {
                current_event_type = event.to_string();
                continue;
            }

            if !line.starts_with("data: ") {
                continue;
            }

            let data = &line[6..];
            let chunk: serde_json::Value = match serde_json::from_str(data) {
                Ok(v) => v,
                Err(_) => continue,
            };

            match current_event_type.as_str() {
                "message_start" => {
                    // { type: "message_start", message: { usage: { input_tokens, output_tokens } } }
                    input_tokens = chunk["message"]["usage"]["input_tokens"]
                        .as_u64().unwrap_or(0) as u32;
                    // Cached tokens come in their own counters, not in input_tokens.
                    cache_write_tokens = chunk["message"]["usage"]
                        ["cache_creation_input_tokens"]
                        .as_u64().unwrap_or(0) as u32;
                    cache_read_tokens = chunk["message"]["usage"]["cache_read_input_tokens"]
                        .as_u64().unwrap_or(0) as u32;
                }
                "content_block_start" => {
                    // Fires at start of each content block (text or tool_use).
                    // { index: N, content_block: { type: "tool_use", id: "...", name: "..." } }
                    if chunk["content_block"]["type"].as_str() == Some("tool_use") {
                        let idx = chunk["index"].as_u64().unwrap_or(0) as usize;
                        let id   = chunk["content_block"]["id"].as_str().unwrap_or("").to_string();
                        let name = chunk["content_block"]["name"].as_str().unwrap_or("").to_string();
                        tool_blocks.insert(idx, (id, name, String::new()));
                    }
                }
                "content_block_delta" => {
                    // { index: N, delta: { type: "text_delta"|"input_json_delta", text|partial_json } }
                    match chunk["delta"]["type"].as_str() {
                        Some("text_delta") => {
                            if let Some(text) = chunk["delta"]["text"].as_str()
                                && !text.is_empty() {
                                    let owned = text.to_string();
                                    let _ = token_tx.send(owned.clone());
                                    content_parts.push(owned);
                                }
                        }
                        Some("input_json_delta") => {
                            // Accumulate partial JSON args for tool_use blocks
                            let idx = chunk["index"].as_u64().unwrap_or(0) as usize;
                            if let Some(entry) = tool_blocks.get_mut(&idx)
                                && let Some(partial) = chunk["delta"]["partial_json"].as_str() {
                                    entry.2.push_str(partial);
                                }
                        }
                        _ => {}
                    }
                }
                "message_delta" => {
                    // { type: "message_delta", usage: { output_tokens } }
                    output_tokens = chunk["usage"]["output_tokens"]
                        .as_u64().unwrap_or(0) as u32;
                }
                _ => {}
            }
        }

        let content = if content_parts.is_empty() {
            None
        } else {
            Some(content_parts.concat())
        };

        // Assemble tool_calls from accumulated tool_use blocks (sorted by index)
        let mut tool_calls: Vec<ToolCall> = Vec::new();
        let mut raw_tool_calls: Vec<serde_json::Value> = Vec::new();
        let mut indices: Vec<usize> = tool_blocks.keys().copied().collect();
        indices.sort();
        for idx in indices {
            if let Some((id, name, args_str)) = tool_blocks.remove(&idx) {
                let arguments: serde_json::Value =
                    serde_json::from_str(&args_str).unwrap_or_default();
                raw_tool_calls.push(json!({
                    "id": id,
                    "type": "function",
                    "function": { "name": name, "arguments": args_str }
                }));
                tool_calls.push(ToolCall { id, name, arguments });
            }
        }

        let tokens_used = input_tokens + output_tokens + cache_write_tokens + cache_read_tokens;

        Ok(LLMResponse {
            content,
            tool_calls,
            raw_tool_calls,
            tokens_used,
            input_tokens,
            output_tokens,
            cache_write_tokens,
            cache_read_tokens,
        })
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            supports_vision: true,
            supports_tool_calling: true,
            max_context_tokens: 200_000,
            supports_streaming: true,
        }
    }

    fn name(&self) -> &str {
        "anthropic"
    }
}

// ── Message Batches API ─────────────────────────────────────────────
//
// Asynchronous, half-price processing: submit many requests at once, poll
// until the batch ends (usually minutes, at most 24 hours), then download
// one result per request. https://docs.anthropic.com/en/api/creating-message-batches

/// One request in a batch. `custom_id` comes back with its result.
pub struct BatchRequest {
    pub custom_id: String,
    pub messages: Vec<Message>,
}

/// A batch's progress as reported by the API.
#[derive(Debug, Clone, serde::Serialize)]
pub struct BatchStatus {
    /// Anthropic's batch id (`msgbatch_...`).
    pub id: String,
    /// `in_progress`, `canceling` or `ended`.
    pub processing_status: String,
    pub processing: u32,
    pub succeeded: u32,
    pub errored: u32,
    pub canceled: u32,
    pub expired: u32,
    /// Where to download results; set once the batch has ended.
    pub results_url: Option<String>,
}

/// The outcome of one request in an ended batch.
#[derive(Debug, Clone)]
pub struct BatchItemResult {
    pub custom_id: String,
    pub text: Option<String>,
    pub error: Option<String>,
    pub input_tokens: u32,
    pub output_tokens: u32,
}

fn parse_batch_status(v: &serde_json::Value) -> BatchStatus {
    let count = |k: &str| v["request_counts"][k].as_u64().unwrap_or(0) as u32;
    BatchStatus {
        id: v["id"].as_str().unwrap_or_default().to_string(),
        processing_status: v["processing_status"].as_str().unwrap_or("in_progress").to_string(),
        processing: count("processing"),
        succeeded: count("succeeded"),
        errored: count("errored"),
        canceled: count("canceled"),
        expired: count("expired"),
        results_url: v["results_url"].as_str().map(str::to_string),
    }
}

/// Parse the results file: JSON Lines, one object per request.
fn parse_batch_results(jsonl: &str) -> Vec<BatchItemResult> {
    jsonl
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .map(|v| {
            let custom_id = v["custom_id"].as_str().unwrap_or_default().to_string();
            let result = &v["result"];
            match result["type"].as_str().unwrap_or("") {
                "succeeded" => {
                    let msg = &result["message"];
                    let text: String = msg["content"]
                        .as_array()
                        .map(|blocks| {
                            blocks
                                .iter()
                                .filter_map(|b| b["text"].as_str())
                                .collect::<Vec<_>>()
                                .join("")
                        })
                        .unwrap_or_default();
                    BatchItemResult {
                        custom_id,
                        text: Some(text),
                        error: None,
                        input_tokens: msg["usage"]["input_tokens"].as_u64().unwrap_or(0) as u32,
                        output_tokens: msg["usage"]["output_tokens"].as_u64().unwrap_or(0) as u32,
                    }
                }
                other => BatchItemResult {
                    custom_id,
                    text: None,
                    error: Some(match other {
                        "errored" => result["error"]["error"]["message"]
                            .as_str()
                            .or_else(|| result["error"]["message"].as_str())
                            .unwrap_or("Request failed")
                            .to_string(),
                        "canceled" => "Batch was canceled before this request ran".to_string(),
                        "expired" => "Batch expired (24 h) before this request ran".to_string(),
                        _ => "Unknown result".to_string(),
                    }),
                    input_tokens: 0,
                    output_tokens: 0,
                },
            }
        })
        .collect()
}

impl AnthropicProvider {
    fn api_key(&self) -> Result<String, LLMError> {
        self.config
            .api_key
            .as_ref()
            .map(|k| k.trim().to_string())
            .filter(|k| !k.is_empty())
            .ok_or_else(|| LLMError::NoApiKey("Anthropic".to_string()))
    }

    async fn batch_get(&self, url: &str) -> Result<reqwest::Response, LLMError> {
        let response = self
            .client
            .get(url)
            .header("x-api-key", self.api_key()?)
            .header("anthropic-version", "2023-06-01")
            .send()
            .await?;
        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(LLMError::ApiError(format!("Anthropic Batch API error {}: {}", status, text)));
        }
        Ok(response)
    }

    /// Submit a batch. Each request uses the same body as a normal `chat` call
    /// (system prompt, cache breakpoint, guardrails), so batched answers match
    /// interactive ones.
    pub async fn create_batch(&self, requests: Vec<BatchRequest>) -> Result<BatchStatus, LLMError> {
        let base = self.config.base_url.trim_end_matches('/');
        let body = json!({
            "requests": requests
                .iter()
                .map(|r| json!({
                    "custom_id": r.custom_id,
                    "params": request_body(&self.config.model, &r.messages),
                }))
                .collect::<Vec<_>>(),
        });
        let response = self
            .client
            .post(format!("{}/messages/batches", base))
            .header("x-api-key", self.api_key()?)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;
        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(LLMError::ApiError(format!("Anthropic Batch API error {}: {}", status, text)));
        }
        Ok(parse_batch_status(&response.json().await?))
    }

    /// Current status of a batch.
    pub async fn get_batch(&self, batch_id: &str) -> Result<BatchStatus, LLMError> {
        let base = self.config.base_url.trim_end_matches('/');
        let response = self
            .batch_get(&format!("{}/messages/batches/{}", base, batch_id))
            .await?;
        Ok(parse_batch_status(&response.json().await?))
    }

    /// Download the results of an ended batch.
    pub async fn batch_results(&self, results_url: &str) -> Result<Vec<BatchItemResult>, LLMError> {
        let text = self.batch_get(results_url).await?.text().await?;
        Ok(parse_batch_results(&text))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_user_message_stays_a_string() {
        let v = user_content(&Message::text("user", "What is the revenue?"));
        assert_eq!(v, json!("What is the revenue?"));
    }

    #[test]
    fn cached_user_message_carries_a_cache_breakpoint() {
        let v = user_content(&Message::cached_text("user", "[Document content]"));
        assert_eq!(v[0]["type"], "text");
        assert_eq!(v[0]["text"], "[Document content]");
        assert_eq!(v[0]["cache_control"]["type"], "ephemeral");
    }
    #[test]
    fn batch_status_reads_counts_and_results_url() {
        let v = json!({
            "id": "msgbatch_01",
            "processing_status": "ended",
            "request_counts": { "processing": 0, "succeeded": 2, "errored": 1, "canceled": 0, "expired": 0 },
            "results_url": "https://api.anthropic.com/v1/messages/batches/msgbatch_01/results"
        });
        let s = parse_batch_status(&v);
        assert_eq!(s.id, "msgbatch_01");
        assert_eq!(s.processing_status, "ended");
        assert_eq!((s.succeeded, s.errored), (2, 1));
        assert!(s.results_url.is_some());
    }

    #[test]
    fn batch_results_parse_successes_and_failures() {
        let jsonl = concat!(
            r#"{"custom_id":"q-0","result":{"type":"succeeded","message":{"content":[{"type":"text","text":"Revenue was $1.2M."}],"usage":{"input_tokens":900,"output_tokens":12}}}}"#,
            "\n",
            r#"{"custom_id":"q-1","result":{"type":"errored","error":{"type":"error","error":{"type":"invalid_request_error","message":"prompt is too long"}}}}"#,
            "\n",
            r#"{"custom_id":"q-2","result":{"type":"expired"}}"#,
            "\n"
        );
        let r = parse_batch_results(jsonl);
        assert_eq!(r.len(), 3);
        assert_eq!(r[0].text.as_deref(), Some("Revenue was $1.2M."));
        assert_eq!((r[0].input_tokens, r[0].output_tokens), (900, 12));
        assert_eq!(r[1].error.as_deref(), Some("prompt is too long"));
        assert!(r[2].error.as_deref().unwrap().contains("expired"));
    }
}
