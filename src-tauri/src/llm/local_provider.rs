//! Answer provider backed by the in-process SLM (`slm.rs`): fully offline,
//! no API key. Uses whichever local model is downloaded (default
//! LFM2.5-1.2B-Instruct). Same contract as every provider: one streamed
//! completion, never tools.

use std::time::Duration;

use async_trait::async_trait;

use super::local;
use super::provider::{
    LLMError, LLMProvider, LLMResponse, Message, ProviderCapabilities, ProviderConfig, Tool,
};
use super::slm::GenerateOptions;

/// Longest answer, in tokens.
const ANSWER_MAX_TOKENS: u32 = 1024;
/// Wall-clock cap for one answer on CPU; the text so far is kept.
const ANSWER_TIME_LIMIT: Duration = Duration::from_secs(240);
/// Context window of the LFM2 / LFM2.5 models.
pub const LOCAL_CONTEXT_TOKENS: usize = 32_768;

pub struct LocalProvider {
    config: ProviderConfig,
}

impl LocalProvider {
    pub fn new(config: ProviderConfig) -> Self {
        Self { config }
    }
}

fn not_ready() -> LLMError {
    LLMError::ApiError(
        "The offline model isn't loaded. Download it in Settings → Local AI \
         (if you just opened the app, it may still be loading — try again in a moment)."
            .to_string(),
    )
}

#[async_trait]
impl LLMProvider for LocalProvider {
    async fn chat(
        &self,
        messages: Vec<Message>,
        tools: Option<Vec<Tool>>,
    ) -> Result<LLMResponse, LLMError> {
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        self.chat_stream(messages, tools, tx).await
    }

    async fn chat_stream(
        &self,
        messages: Vec<Message>,
        _tools: Option<Vec<Tool>>,
        token_tx: tokio::sync::mpsc::UnboundedSender<String>,
    ) -> Result<LLMResponse, LLMError> {
        if !local::is_engine_loaded() {
            return Err(not_ready());
        }
        let turns: Vec<(String, String)> = messages
            .into_iter()
            .map(|m| (m.role, m.content))
            .collect();

        let generation = tokio::task::spawn_blocking(move || {
            let turns: Vec<(&str, &str)> =
                turns.iter().map(|(r, c)| (r.as_str(), c.as_str())).collect();
            // Stop early if nobody is listening any more.
            let mut on_text = |piece: &str| token_tx.send(piece.to_string()).is_ok();
            local::generate_conversation(
                &turns,
                GenerateOptions {
                    max_tokens: ANSWER_MAX_TOKENS,
                    time_limit: ANSWER_TIME_LIMIT,
                    cancel: None,
                    on_text: Some(&mut on_text),
                },
            )
        })
        .await
        .map_err(|e| LLMError::ApiError(format!("Local model task failed: {}", e)))?
        .map_err(|e| {
            if e.contains("not loaded") { not_ready() } else { LLMError::ApiError(e) }
        })?;

        Ok(LLMResponse {
            content: (!generation.text.is_empty()).then_some(generation.text),
            tool_calls: Vec::new(),
            raw_tool_calls: Vec::new(),
            tokens_used: generation.prompt_tokens + generation.output_tokens,
            input_tokens: generation.prompt_tokens,
            output_tokens: generation.output_tokens,
        })
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            supports_vision: false,
            supports_tool_calling: false,
            max_context_tokens: LOCAL_CONTEXT_TOKENS,
            supports_streaming: true,
        }
    }

    fn name(&self) -> &str {
        &self.config.name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider() -> LocalProvider {
        LocalProvider::new(ProviderConfig {
            id: "local".to_string(),
            name: "local".to_string(),
            api_key: None,
            base_url: String::new(),
            model: "on-device".to_string(),
            is_active: true,
        })
    }

    #[tokio::test]
    async fn explains_how_to_get_the_model_when_none_is_loaded() {
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let err = provider()
            .chat_stream(vec![Message::text("user", "hi")], None, tx)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("Settings → Local AI"));
    }

    #[test]
    fn never_offers_tools() {
        assert!(!provider().capabilities().supports_tool_calling);
    }
}
