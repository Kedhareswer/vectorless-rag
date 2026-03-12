use crate::llm::provider::ProviderConfig;

/// Validate a provider config before saving.
pub fn validate_provider(config: &ProviderConfig) -> Result<(), String> {
    if config.name.trim().is_empty() {
        return Err("Provider name is required.".to_string());
    }
    if config.model.trim().is_empty() {
        return Err("Model name is required.".to_string());
    }

    // Ollama doesn't require an API key or base URL validation
    let is_ollama = config.name.to_lowercase() == "ollama";

    if !is_ollama {
        if let Some(ref key) = config.api_key {
            if key.trim().is_empty() {
                return Err("API key is required for this provider.".to_string());
            }
        } else {
            return Err("API key is required for this provider.".to_string());
        }
    }

    if !config.base_url.is_empty() {
        if !config.base_url.starts_with("http://") && !config.base_url.starts_with("https://") {
            return Err("Base URL must start with http:// or https://".to_string());
        }
    }

    Ok(())
}

/// Validate a file path before ingestion.
pub fn validate_file_path(path: &str) -> Result<(), String> {
    if path.trim().is_empty() {
        return Err("File path is required.".to_string());
    }

    // Reject path traversal patterns
    if path.contains("..") {
        return Err("File path must not contain '..' traversal.".to_string());
    }

    let path = std::path::Path::new(path);
    if !path.exists() {
        return Err(format!("File does not exist: {}", path.display()));
    }
    if !path.is_file() {
        return Err(format!("Path is not a file: {}", path.display()));
    }

    Ok(())
}

/// Validate chat input before sending to agent.
pub fn validate_chat_input(
    message: &str,
    doc_ids: &[String],
    provider_id: &str,
) -> Result<(), String> {
    if message.trim().is_empty() {
        return Err("Message cannot be empty.".to_string());
    }
    if doc_ids.is_empty() {
        return Err("At least one document must be selected.".to_string());
    }
    if provider_id.trim().is_empty() {
        return Err("Provider ID is required.".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_config(name: &str, model: &str, api_key: Option<&str>, base_url: &str) -> ProviderConfig {
        ProviderConfig {
            id: "test-id".to_string(),
            name: name.to_string(),
            model: model.to_string(),
            api_key: api_key.map(|k| k.to_string()),
            base_url: base_url.to_string(),
            is_active: true,
        }
    }

    #[test]
    fn valid_provider_passes() {
        let config = make_config("openai", "gpt-4o", Some("sk-abc123"), "https://api.openai.com/v1");
        assert!(validate_provider(&config).is_ok());
    }

    #[test]
    fn empty_name_fails() {
        let config = make_config("", "gpt-4o", Some("sk-abc"), "");
        assert!(validate_provider(&config).is_err());
    }

    #[test]
    fn empty_model_fails() {
        let config = make_config("openai", "", Some("sk-abc"), "");
        assert!(validate_provider(&config).is_err());
    }

    #[test]
    fn missing_api_key_fails_for_non_ollama() {
        let config = make_config("openai", "gpt-4o", None, "");
        assert!(validate_provider(&config).is_err());
    }

    #[test]
    fn empty_api_key_fails_for_non_ollama() {
        let config = make_config("openai", "gpt-4o", Some("  "), "");
        assert!(validate_provider(&config).is_err());
    }

    #[test]
    fn ollama_without_api_key_passes() {
        let config = make_config("Ollama", "llama3", None, "http://localhost:11434");
        assert!(validate_provider(&config).is_ok());
    }

    #[test]
    fn bad_base_url_fails() {
        let config = make_config("openai", "gpt-4o", Some("sk-abc"), "ftp://bad.com");
        assert!(validate_provider(&config).is_err());
    }

    #[test]
    fn empty_base_url_is_ok() {
        let config = make_config("openai", "gpt-4o", Some("sk-abc"), "");
        assert!(validate_provider(&config).is_ok());
    }

    #[test]
    fn path_traversal_rejected() {
        assert!(validate_file_path("../../../etc/passwd").is_err());
    }

    #[test]
    fn empty_path_rejected() {
        assert!(validate_file_path("").is_err());
    }

    #[test]
    fn empty_message_rejected() {
        assert!(validate_chat_input("", &["doc1".to_string()], "prov1").is_err());
    }

    #[test]
    fn empty_doc_ids_rejected() {
        assert!(validate_chat_input("hello", &[], "prov1").is_err());
    }

    #[test]
    fn empty_provider_id_rejected() {
        assert!(validate_chat_input("hello", &["doc1".to_string()], "").is_err());
    }

    #[test]
    fn valid_chat_input_passes() {
        assert!(validate_chat_input("hello", &["doc1".to_string()], "prov1").is_ok());
    }
}
