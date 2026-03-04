use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct ExplorationContext {
    pub explored_nodes: Vec<String>,
    pub summaries: HashMap<String, String>,
    pub step_count: u32,
    pub max_steps: u32,
    pub total_tokens: u32,
}

impl ExplorationContext {
    pub fn new(max_steps: u32) -> Self {
        Self {
            explored_nodes: Vec::new(),
            summaries: HashMap::new(),
            step_count: 0,
            max_steps,
            total_tokens: 0,
        }
    }

    pub fn record_step(&mut self, node_id: &str, summary: &str, tokens: u32) {
        if !self.explored_nodes.contains(&node_id.to_string()) {
            self.explored_nodes.push(node_id.to_string());
        }
        self.summaries.insert(node_id.to_string(), summary.to_string());
        self.step_count += 1;
        self.total_tokens += tokens;
    }

    pub fn has_explored(&self, node_id: &str) -> bool {
        self.explored_nodes.contains(&node_id.to_string())
    }

    pub fn budget_remaining(&self) -> u32 {
        self.max_steps.saturating_sub(self.step_count)
    }

    pub fn to_context_string(&self) -> String {
        let mut parts = Vec::new();
        parts.push(format!(
            "Exploration progress: {}/{} steps used, {} tokens consumed.",
            self.step_count, self.max_steps, self.total_tokens
        ));

        if !self.explored_nodes.is_empty() {
            parts.push(format!(
                "Explored {} nodes: {}",
                self.explored_nodes.len(),
                self.explored_nodes.join(", ")
            ));
        }

        if !self.summaries.is_empty() {
            parts.push("Node summaries:".to_string());
            for (node_id, summary) in &self.summaries {
                parts.push(format!("  - {}: {}", node_id, summary));
            }
        }

        parts.join("\n")
    }
}
