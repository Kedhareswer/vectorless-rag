// NOTE: Scaffolding for future ReAct agent loop. Not called by live code.
#![allow(dead_code)]

use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug)]
pub struct ExplorationContext {
    pub explored_nodes: HashSet<String>,
    pub summaries: HashMap<String, String>,
    /// How many times each node was accessed (visit frequency)
    pub visit_counts: HashMap<String, u32>,
    pub step_count: u32,
    pub max_steps: u32,
    pub total_tokens: u32,
    /// Relevance scores computed after exploration (0.0-1.0)
    pub relevance_scores: HashMap<String, f64>,
}

impl ExplorationContext {
    pub fn new(max_steps: u32) -> Self {
        Self {
            explored_nodes: HashSet::new(),
            summaries: HashMap::new(),
            visit_counts: HashMap::new(),
            step_count: 0,
            max_steps,
            total_tokens: 0,
            relevance_scores: HashMap::new(),
        }
    }

    pub fn record_step(&mut self, node_id: &str, summary: &str, tokens: u32) {
        self.explored_nodes.insert(node_id.to_string());
        *self.visit_counts.entry(node_id.to_string()).or_insert(0) += 1;
        self.summaries.insert(node_id.to_string(), summary.to_string());
        self.step_count += 1;
        self.total_tokens += tokens;
    }

    pub fn has_explored(&self, node_id: &str) -> bool {
        self.explored_nodes.contains(node_id)
    }

    pub fn budget_remaining(&self) -> u32 {
        self.max_steps.saturating_sub(self.step_count)
    }

    /// Compute relevance scores for all explored nodes based on visit frequency
    /// and content richness. Nodes visited more often or yielding larger summaries
    /// are scored higher. Call this after exploration completes.
    pub fn compute_relevance_scores(&mut self) {
        if self.explored_nodes.is_empty() {
            return;
        }

        let max_visits = self.visit_counts.values().copied().max().unwrap_or(1).max(1) as f64;

        for node_id in &self.explored_nodes {
            let visit_count = *self.visit_counts.get(node_id).unwrap_or(&1) as f64;
            let summary_len = self.summaries.get(node_id).map(|s| s.len()).unwrap_or(0) as f64;

            // Normalized visit frequency (0-1)
            let freq_score = visit_count / max_visits;

            // Content richness score (longer summaries suggest more relevant content)
            let content_score = (summary_len / 200.0).min(1.0);

            // Combined score: 60% frequency, 40% content richness
            let score = freq_score * 0.6 + content_score * 0.4;

            self.relevance_scores.insert(node_id.clone(), (score * 100.0).round() / 100.0);
        }
    }

    pub fn to_context_string(&self) -> String {
        let mut parts = Vec::new();
        parts.push(format!(
            "Exploration progress: {}/{} steps used, {} tokens consumed.",
            self.step_count, self.max_steps, self.total_tokens
        ));

        if !self.explored_nodes.is_empty() {
            let node_list: Vec<&str> = self.explored_nodes.iter().map(|s| s.as_str()).collect();
            parts.push(format!(
                "Explored {} nodes: {}",
                self.explored_nodes.len(),
                node_list.join(", ")
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
