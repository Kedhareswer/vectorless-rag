use serde::Serialize;

/// Unified event type sent over the Tauri Channel during agent chat.
/// Replaces the old separate event structs + `app.emit()` approach with
/// a single ordered Channel for all chat-related events.
#[derive(Serialize, Clone, Debug)]
#[serde(tag = "type")]
pub enum ChatEvent {
    /// An exploration step has started (tool call begins).
    #[serde(rename = "step-start")]
    StepStart {
        #[serde(rename = "stepNumber")]
        step_number: u32,
        tool: String,
        #[serde(rename = "inputSummary")]
        input_summary: String,
    },

    /// An exploration step has completed (tool call finished).
    #[serde(rename = "step-complete")]
    StepComplete {
        #[serde(rename = "stepNumber")]
        step_number: u32,
        #[serde(rename = "outputSummary")]
        output_summary: String,
        #[serde(rename = "tokensUsed")]
        tokens_used: u32,
        #[serde(rename = "latencyMs")]
        latency_ms: u64,
        cost: f64,
        #[serde(rename = "nodeIds")]
        node_ids: Vec<String>,
    },

    /// A streaming token from the LLM response.
    #[serde(rename = "token")]
    Token {
        token: String,
        done: bool,
    },

    /// The complete final response (sent after all tokens).
    #[serde(rename = "response")]
    Response {
        content: String,
    },

    /// An error occurred during the chat.
    #[serde(rename = "error")]
    Error {
        error: String,
    },
}
