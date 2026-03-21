//! Optional LiteParse integration for enhanced PDF text extraction.
//!
//! LiteParse (`@llamaindex/liteparse`) provides layout-aware PDF parsing with
//! spatial grid text extraction and optional OCR. It is NOT a dependency — the
//! app detects it at runtime and falls back to Rust parsers if unavailable.
//!
//! Requires: Node.js + npx on PATH. Detection result is cached per session.

use std::process::{Command, Stdio};
use std::sync::OnceLock;

/// Cached detection result: None = not checked, Some(bool) = result.
static AVAILABLE: OnceLock<bool> = OnceLock::new();

/// Check if LiteParse is available on this system.
/// Result is cached for the session lifetime (no re-check on every ingest).
pub fn is_available() -> bool {
    *AVAILABLE.get_or_init(|| {
        // Quick check: is npx on PATH at all?
        let npx_check = Command::new("npx")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        if npx_check.map(|s| s.success()).unwrap_or(false) {
            // Try the actual LiteParse package
            Command::new("npx")
                .args(["@llamaindex/liteparse", "--version"])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
        } else {
            false
        }
    })
}

/// Run LiteParse on a PDF file and return the raw JSON output.
/// Falls back to Err if LiteParse is not available or the command fails.
pub fn parse_pdf(file_path: &str) -> Result<String, String> {
    if !is_available() {
        return Err("LiteParse not available".to_string());
    }

    let output = Command::new("npx")
        .args([
            "@llamaindex/liteparse",
            "parse",
            file_path,
            "--format",
            "json",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("LiteParse execution failed: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("LiteParse error: {}", stderr.trim()));
    }

    String::from_utf8(output.stdout).map_err(|e| format!("LiteParse output encoding error: {}", e))
}

/// Parse LiteParse JSON output into text blocks with optional page numbers.
/// Returns a flat list of (text, page_number) tuples.
pub fn extract_text_blocks(json: &str) -> Vec<(String, usize)> {
    // LiteParse outputs a JSON array of page objects with text content.
    // We parse conservatively — if the format changes, we return empty and
    // the caller falls back to the Rust parser.
    let parsed: serde_json::Value = match serde_json::from_str(json) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };

    let mut blocks = Vec::new();

    // Expected format: { "pages": [ { "page": 1, "text": "..." }, ... ] }
    // or an array of page objects directly.
    let pages = if let Some(arr) = parsed.as_array() {
        arr.clone()
    } else if let Some(arr) = parsed.get("pages").and_then(|p| p.as_array()) {
        arr.clone()
    } else {
        return Vec::new();
    };

    for (idx, page) in pages.iter().enumerate() {
        let page_num = page
            .get("page")
            .and_then(|p| p.as_u64())
            .map(|p| p as usize)
            .unwrap_or(idx + 1);

        let text = page
            .get("text")
            .and_then(|t| t.as_str())
            .unwrap_or("");

        if !text.trim().is_empty() {
            blocks.push((text.to_string(), page_num));
        }
    }

    blocks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_text_blocks_from_valid_json() {
        let json = r#"{"pages":[{"page":1,"text":"Hello world"},{"page":2,"text":"Page two"}]}"#;
        let blocks = extract_text_blocks(json);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].0, "Hello world");
        assert_eq!(blocks[0].1, 1);
        assert_eq!(blocks[1].1, 2);
    }

    #[test]
    fn extract_text_blocks_from_array_format() {
        let json = r#"[{"page":1,"text":"Content"}]"#;
        let blocks = extract_text_blocks(json);
        assert_eq!(blocks.len(), 1);
    }

    #[test]
    fn extract_text_blocks_invalid_json_returns_empty() {
        let blocks = extract_text_blocks("not json");
        assert!(blocks.is_empty());
    }

    #[test]
    fn extract_text_blocks_empty_pages_returns_empty() {
        let blocks = extract_text_blocks(r#"{"pages":[]}"#);
        assert!(blocks.is_empty());
    }
}
