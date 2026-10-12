//! Find model servers already running on this machine (Ollama, LM Studio) and
//! list the models the user has installed in them, so existing local models
//! can answer without another download.

use std::time::Duration;

use serde::Serialize;

pub const OLLAMA_URL: &str = "http://localhost:11434";
pub const LMSTUDIO_URL: &str = "http://localhost:1234/v1";

/// A model server found on this machine.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct LocalServer {
    /// Provider name to use for it: "ollama" or "lmstudio".
    pub provider: String,
    pub label: String,
    pub base_url: String,
    /// Installed model IDs, as the server names them.
    pub models: Vec<String>,
}

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        // Local only; a system proxy must never see these requests.
        .no_proxy()
        .timeout(Duration::from_millis(1500))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

/// `GET {base}/api/tags` → `{"models":[{"name":"llama3.2:3b"},…]}`
pub async fn ollama_models(base_url: &str) -> Option<Vec<String>> {
    let url = format!("{}/api/tags", base_url.trim_end_matches('/'));
    let json: serde_json::Value = client().get(url).send().await.ok()?.json().await.ok()?;
    Some(
        json["models"]
            .as_array()?
            .iter()
            .filter_map(|m| m["name"].as_str().map(str::to_string))
            .collect(),
    )
}

/// `GET {base}/models` (OpenAI style) → `{"data":[{"id":"qwen2.5-7b-instruct"},…]}`.
/// Embedding models are skipped; they can't answer questions.
pub async fn openai_style_models(base_url: &str) -> Option<Vec<String>> {
    let url = format!("{}/models", base_url.trim_end_matches('/'));
    let json: serde_json::Value = client().get(url).send().await.ok()?.json().await.ok()?;
    Some(
        json["data"]
            .as_array()?
            .iter()
            .filter_map(|m| m["id"].as_str())
            .filter(|id| !id.to_lowercase().contains("embed"))
            .map(str::to_string)
            .collect(),
    )
}

/// Probe the default Ollama and LM Studio addresses in parallel. Servers that
/// aren't running are simply left out.
pub async fn detect() -> Vec<LocalServer> {
    detect_at(OLLAMA_URL, LMSTUDIO_URL).await
}

async fn detect_at(ollama_url: &str, lmstudio_url: &str) -> Vec<LocalServer> {
    let (ollama, lmstudio) = tokio::join!(ollama_models(ollama_url), openai_style_models(lmstudio_url));
    let mut servers = Vec::new();
    if let Some(models) = ollama {
        servers.push(LocalServer {
            provider: "ollama".to_string(),
            label: "Ollama".to_string(),
            base_url: ollama_url.to_string(),
            models,
        });
    }
    if let Some(models) = lmstudio {
        servers.push(LocalServer {
            provider: "lmstudio".to_string(),
            label: "LM Studio".to_string(),
            base_url: lmstudio_url.to_string(),
            models,
        });
    }
    servers
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// Serve one JSON body on a random local port; returns its base URL.
    async fn serve_json(body: &'static str) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 4096];
            let _ = socket.read(&mut buf).await;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = socket.write_all(response.as_bytes()).await;
        });
        format!("http://{}", addr)
    }

    /// A port with nothing listening.
    async fn closed_port() -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        format!("http://{}", addr)
    }

    #[tokio::test]
    async fn lists_ollama_models() {
        let url = serve_json(r#"{"models":[{"name":"llama3.2:3b"},{"name":"qwen3:8b"}]}"#).await;
        assert_eq!(ollama_models(&url).await.unwrap(), vec!["llama3.2:3b", "qwen3:8b"]);
    }

    #[tokio::test]
    async fn lists_lmstudio_models_without_embedders() {
        let url = serve_json(
            r#"{"data":[{"id":"qwen2.5-7b-instruct"},{"id":"text-embedding-nomic-embed-text-v1.5"}]}"#,
        )
        .await;
        assert_eq!(openai_style_models(&url).await.unwrap(), vec!["qwen2.5-7b-instruct"]);
    }

    #[tokio::test]
    async fn reports_only_servers_that_answer() {
        let ollama = serve_json(r#"{"models":[{"name":"phi4"}]}"#).await;
        let lmstudio = closed_port().await;
        let servers = detect_at(&ollama, &lmstudio).await;
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].provider, "ollama");
        assert_eq!(servers[0].models, vec!["phi4"]);
    }

    #[tokio::test]
    async fn nothing_running_means_no_servers() {
        let a = closed_port().await;
        let b = closed_port().await;
        assert!(detect_at(&a, &b).await.is_empty());
    }
}
