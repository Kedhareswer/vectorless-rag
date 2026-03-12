use async_trait::async_trait;
use std::time::Duration;

use super::provider::{
    LLMError, LLMProvider, LLMResponse, Message, ProviderCapabilities, Tool,
};

const MAX_RETRIES: u32 = 3;
const BASE_DELAY_MS: u64 = 300;

/// Wraps any `LLMProvider` with automatic retry and exponential backoff.
/// Retries on transient errors (429, 500, 502, 503, 529) up to MAX_RETRIES times.
pub struct RetryProvider {
    inner: Box<dyn LLMProvider>,
}

impl RetryProvider {
    pub fn new(provider: Box<dyn LLMProvider>) -> Self {
        Self { inner: provider }
    }
}

fn is_retryable(err: &LLMError) -> bool {
    match err {
        LLMError::ApiError(msg) => {
            // Check for retryable HTTP status codes in the error message
            msg.contains("429")
                || msg.contains("500")
                || msg.contains("502")
                || msg.contains("503")
                || msg.contains("529")
                || msg.contains("rate limit")
                || msg.contains("overloaded")
        }
        LLMError::RequestError(_) => true, // network errors are retryable
        _ => false,
    }
}

fn parse_retry_after(err: &LLMError) -> Option<Duration> {
    if let LLMError::ApiError(msg) = err {
        // Some providers include retry-after info in the error message
        if let Some(pos) = msg.to_lowercase().find("retry-after") {
            let after = &msg[pos + 12..];
            if let Some(secs) = after
                .trim_start_matches(|c: char| !c.is_ascii_digit())
                .split(|c: char| !c.is_ascii_digit())
                .next()
                .and_then(|s| s.parse::<u64>().ok())
            {
                return Some(Duration::from_secs(secs.min(30)));
            }
        }
    }
    None
}

#[async_trait]
impl LLMProvider for RetryProvider {
    async fn chat(
        &self,
        messages: Vec<Message>,
        tools: Option<Vec<Tool>>,
    ) -> Result<LLMResponse, LLMError> {
        let mut last_err = None;

        for attempt in 0..=MAX_RETRIES {
            match self.inner.chat(messages.clone(), tools.clone()).await {
                Ok(resp) => return Ok(resp),
                Err(e) => {
                    if attempt == MAX_RETRIES || !is_retryable(&e) {
                        return Err(e);
                    }
                    let delay = parse_retry_after(&e).unwrap_or_else(|| {
                        Duration::from_millis(BASE_DELAY_MS * 2u64.pow(attempt))
                    });
                    tokio::time::sleep(delay).await;
                    last_err = Some(e);
                }
            }
        }

        Err(last_err.unwrap_or_else(|| LLMError::ApiError("Max retries exceeded".to_string())))
    }

    async fn chat_stream(
        &self,
        messages: Vec<Message>,
        tools: Option<Vec<Tool>>,
        token_tx: tokio::sync::mpsc::UnboundedSender<String>,
    ) -> Result<LLMResponse, LLMError> {
        let mut last_err = None;

        for attempt in 0..=MAX_RETRIES {
            // Create a fresh channel for each attempt since the sender can't be reused
            // after a failed streaming attempt
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
            let token_tx_clone = token_tx.clone();

            // Forward tokens from inner channel to outer channel
            let forwarder = tokio::spawn(async move {
                while let Some(token) = rx.recv().await {
                    let _ = token_tx_clone.send(token);
                }
            });

            match self.inner.chat_stream(messages.clone(), tools.clone(), tx).await {
                Ok(resp) => {
                    let _ = forwarder.await;
                    return Ok(resp);
                }
                Err(e) => {
                    forwarder.abort();
                    if attempt == MAX_RETRIES || !is_retryable(&e) {
                        return Err(e);
                    }
                    let delay = parse_retry_after(&e).unwrap_or_else(|| {
                        Duration::from_millis(BASE_DELAY_MS * 2u64.pow(attempt))
                    });
                    tokio::time::sleep(delay).await;
                    last_err = Some(e);
                }
            }
        }

        Err(last_err.unwrap_or_else(|| LLMError::ApiError("Max retries exceeded".to_string())))
    }

    fn capabilities(&self) -> ProviderCapabilities {
        self.inner.capabilities()
    }

    fn name(&self) -> &str {
        self.inner.name()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn api_err(msg: &str) -> LLMError {
        LLMError::ApiError(msg.to_string())
    }

    #[test]
    fn test_is_retryable_429() {
        assert!(is_retryable(&api_err("API error 429: rate limited")));
    }

    #[test]
    fn test_is_retryable_500() {
        assert!(is_retryable(&api_err("API error 500: internal server error")));
    }

    #[test]
    fn test_is_retryable_overloaded() {
        assert!(is_retryable(&api_err("API error: overloaded")));
    }

    #[test]
    fn test_not_retryable_401() {
        assert!(!is_retryable(&api_err("API error 401: unauthorized")));
    }

    #[test]
    fn test_not_retryable_no_api_key() {
        assert!(!is_retryable(&LLMError::NoApiKey("test".to_string())));
    }

    #[test]
    fn test_parse_retry_after_seconds() {
        let err = api_err("429 Too Many Requests. Retry-After: 5");
        assert_eq!(parse_retry_after(&err), Some(Duration::from_secs(5)));
    }

    #[test]
    fn test_parse_retry_after_missing() {
        let err = api_err("500 Internal Server Error");
        assert_eq!(parse_retry_after(&err), None);
    }

    #[test]
    fn test_parse_retry_after_capped_at_30() {
        let err = api_err("429 Rate limited. Retry-After: 120");
        assert_eq!(parse_retry_after(&err), Some(Duration::from_secs(30)));
    }
}
