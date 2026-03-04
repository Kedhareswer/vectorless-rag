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
            "{}/models/{}:generateContent?key={}",
            base, self.config.model, api_key
        );

        // Convert messages to Google's format
        // Google Gemini requires alternating user/model turns.
        // System messages are passed via systemInstruction.
        let mut system_instruction: Option<serde_json::Value> = None;
        let mut contents: Vec<serde_json::Value> = Vec::new();

        for m in &messages {
            match m.role.as_str() {
                "system" => {
                    // Google uses systemInstruction for system prompts
                    system_instruction = Some(json!({
                        "parts": [{ "text": m.content }]
                    }));
                }
                "assistant" | "model" => {
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
                            "parts": [{ "text": m.content }]
                        }));
                    }
                }
                "tool" => {
                    // Tool results in Google format use functionResponse
                    let tool_name = m.tool_name.as_deref().unwrap_or("unknown");
                    // Try to parse content as JSON, fall back to wrapping as string
                    let response_value: serde_json::Value =
                        serde_json::from_str(&m.content).unwrap_or(json!({ "result": m.content }));
                    contents.push(json!({
                        "role": "user",
                        "parts": [{
                            "functionResponse": {
                                "name": tool_name,
                                "response": response_value,
                            }
                        }]
                    }));
                }
                _ => {
                    // user or other
                    contents.push(json!({
                        "role": "user",
                        "parts": [{ "text": m.content }]
                    }));
                }
            }
        }

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

        let parts = &resp_json["candidates"][0]["content"]["parts"];
        let mut content = None;
        let mut tool_calls = Vec::new();
        let mut raw_tool_calls = Vec::new();

        if let Some(parts_arr) = parts.as_array() {
            for (i, part) in parts_arr.iter().enumerate() {
                if let Some(text) = part["text"].as_str() {
                    content = Some(text.to_string());
                }
                if let Some(fc) = part.get("functionCall") {
                    // Store the raw part for conversation history
                    raw_tool_calls.push(part.clone());
                    if let Some(name) = fc["name"].as_str() {
                        let arguments = fc["args"].clone();
                        // Google doesn't have tool call IDs, generate one
                        let call_id = format!("google_call_{}", i);
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
            supports_streaming: true,
        }
    }

    fn name(&self) -> &str {
        "google"
    }
}
