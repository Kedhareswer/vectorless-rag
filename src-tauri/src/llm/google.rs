use async_trait::async_trait;
use serde_json::json;

use super::provider::{
    LLMError, LLMProvider, LLMResponse, Message, ProviderCapabilities, ProviderConfig, Tool,
    ToolCall,
};

pub struct GoogleProvider {
    pub config: ProviderConfig,
    pub client: reqwest::Client,
}

impl GoogleProvider {
    pub fn new(mut config: ProviderConfig) -> Self {
        if config.base_url.is_empty() {
            config.base_url =
                "https://generativelanguage.googleapis.com/v1beta".to_string();
        }
        Self {
            config,
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl LLMProvider for GoogleProvider {
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
            .ok_or_else(|| LLMError::NoApiKey("Google".to_string()))?;

        let base = self.config.base_url.trim_end_matches('/');
        let url = format!(
            "{}/models/{}:generateContent",
            base, self.config.model
        );

        // Convert messages to Google's format
        // Google Gemini requires alternating user/model turns.
        // System messages are passed via systemInstruction.
        let mut system_instruction: Option<serde_json::Value> = None;
        let mut contents: Vec<serde_json::Value> = Vec::new();

        // Collect tool result parts so consecutive tool messages merge into one user turn.
        let mut pending_tool_parts: Vec<serde_json::Value> = Vec::new();

        let flush_tool_parts =
            |parts: &mut Vec<serde_json::Value>, contents: &mut Vec<serde_json::Value>| {
                if !parts.is_empty() {
                    contents.push(json!({
                        "role": "user",
                        "parts": std::mem::take(parts),
                    }));
                }
            };

        for m in &messages {
            match m.role.as_str() {
                "system" => {
                    flush_tool_parts(&mut pending_tool_parts, &mut contents);
                    // Google uses systemInstruction for system prompts
                    system_instruction = Some(json!({
                        "parts": [{ "text": m.content }]
                    }));
                }
                "assistant" | "model" => {
                    flush_tool_parts(&mut pending_tool_parts, &mut contents);
                    if let Some(ref tc_raw) = m.tool_calls_raw {
                        // Assistant message with function calls
                        let mut parts: Vec<serde_json::Value> = Vec::new();
                        if !m.content.is_empty() {
                            parts.push(json!({ "text": m.content }));
                        }
                        for tc in tc_raw {
                            parts.push(tc.clone());
                        }
                        contents.push(json!({
                            "role": "model",
                            "parts": parts,
                        }));
                    } else {
                        contents.push(json!({
                            "role": "model",
                            "parts": [{ "text": if m.content.is_empty() { " " } else { &m.content } }]
                        }));
                    }
                }
                "tool" => {
                    // Tool results in Google format use functionResponse.
                    // Gemini requires `response` to always be a JSON object (Struct).
                    let tool_name = match m.tool_name.as_deref() {
                        Some(name) => name,
                        None => {
                            eprintln!("[google] Tool result message missing tool_name");
                            "unspecified_tool"
                        }
                    };
                    let parsed: serde_json::Value =
                        serde_json::from_str(&m.content).unwrap_or(json!({ "result": m.content }));
                    // Ensure the value is always a JSON object (not array/primitive)
                    let response_object = match &parsed {
                        serde_json::Value::Object(_) => parsed,
                        _ => json!({ "result": parsed }),
                    };
                    // Accumulate into pending parts; they'll be flushed as a single
                    // "user" message before the next non-tool message.
                    pending_tool_parts.push(json!({
                        "functionResponse": {
                            "name": tool_name,
                            "response": response_object,
                        }
                    }));
                }
                _ => {
                    flush_tool_parts(&mut pending_tool_parts, &mut contents);
                    // user or other
                    contents.push(json!({
                        "role": "user",
                        "parts": [{ "text": m.content }]
                    }));
                }
            }
        }

        // Flush any remaining tool parts
        flush_tool_parts(&mut pending_tool_parts, &mut contents);

        let mut body = json!({
            "contents": contents,
        });

        if let Some(si) = system_instruction {
            body["systemInstruction"] = si;
        }

        if let Some(tools) = tools {
            let function_declarations: Vec<serde_json::Value> = tools
                .iter()
                .map(|t| {
                    json!({
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.parameters,
                    })
                })
                .collect();
            body["tools"] = json!([{
                "functionDeclarations": function_declarations,
            }]);
        }

        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("x-goog-api-key", &api_key)
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(LLMError::ApiError(format!(
                "Google API error {}: {}",
                status, text
            )));
        }

        let resp_json: serde_json::Value = response.json().await?;

        // Check for blocked/empty candidates
        let candidates = resp_json["candidates"].as_array();
        if candidates.is_none() || candidates.is_some_and(|c| c.is_empty()) {
            let block_reason = resp_json["promptFeedback"]["blockReason"]
                .as_str()
                .unwrap_or("unknown");
            return Err(LLMError::ApiError(format!(
                "Google API returned no candidates (blockReason: {})",
                block_reason
            )));
        }

        let candidate = &resp_json["candidates"][0];
        let finish_reason = candidate["finishReason"].as_str().unwrap_or("STOP");
        if finish_reason == "SAFETY" || finish_reason == "RECITATION" {
            return Err(LLMError::ApiError(format!(
                "Google API blocked response (finishReason: {})",
                finish_reason
            )));
        }

        let parts = &candidate["content"]["parts"];
        let mut content: Option<String> = None;
        let mut tool_calls = Vec::new();
        let mut raw_tool_calls = Vec::new();
        let mut tool_call_counter = 0u32;

        if let Some(parts_arr) = parts.as_array() {
            for part in parts_arr {
                if let Some(text) = part["text"].as_str() {
                    match &mut content {
                        Some(existing) => {
                            existing.push_str("\n\n");
                            existing.push_str(text);
                        }
                        None => {
                            content = Some(text.to_string());
                        }
                    }
                }
                if let Some(fc) = part.get("functionCall") {
                    // Store the raw part for conversation history
                    raw_tool_calls.push(part.clone());
                    if let Some(name) = fc["name"].as_str() {
                        let arguments = fc["args"].clone();
                        // Generate a stable ID using name + counter
                        tool_call_counter += 1;
                        let call_id = format!("google_{}_{}", name, tool_call_counter);
                        tool_calls.push(ToolCall {
                            id: call_id,
                            name: name.to_string(),
                            arguments,
                        });
                    }
                }
            }
        }

        let tokens_used = resp_json["usageMetadata"]["totalTokenCount"]
            .as_u64()
            .unwrap_or(0) as u32;

        Ok(LLMResponse {
            content,
            tool_calls,
            raw_tool_calls,
            tokens_used,
        })
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            supports_vision: true,
            supports_tool_calling: true,
            max_context_tokens: 1_048_576,
            supports_streaming: false,
        }
    }

    fn name(&self) -> &str {
        "google"
    }
}
