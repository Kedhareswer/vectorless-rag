//! Background metadata enrichment for document tree nodes.
//!
//! After parsing, this module runs over each top-level node to generate:
//! - summary: 1-2 sentence description of the node's content
//! - entities: extracted names, dates, amounts, organizations
//! - topics: 3-5 keyword tags
//!
//! When a local GGUF model is loaded (via llm/local.rs), LLM-generated
//! summaries are used. Otherwise falls back to heuristic extraction.

use crate::document::tree::DocumentTree;
use crate::llm::local;
use std::collections::HashSet;

/// Minimum content length (chars) to bother enriching a node.
const MIN_CONTENT_LEN: usize = 50;
/// Maximum content length to feed to the summarizer.
const MAX_CONTENT_FOR_SUMMARY: usize = 2000;

/// Enrich a document tree's top-level nodes with metadata.
/// Modifies the tree in-place. Returns the number of nodes enriched.
pub fn enrich_tree_metadata(tree: &mut DocumentTree) -> usize {
    let root_id = tree.root_id.clone();
    let child_ids: Vec<String> = tree
        .get_node(&root_id)
        .map(|r| r.children.clone())
        .unwrap_or_default();

    let mut enriched_count = 0;

    for child_id in &child_ids {
        // Gather content from this node and its children
        let content = gather_node_content(tree, child_id);
        if content.len() < MIN_CONTENT_LEN {
            continue;
        }

        // Use LLM-generated summary if a local model is loaded, otherwise
        // fall back to the extractive heuristic.
        let summary = if local::is_sidecar_running() {
            llm_summary(&content).unwrap_or_else(|_| extractive_summary(&content))
        } else {
            extractive_summary(&content)
        };
        let entities = extract_entities(&content);
        let topics = extract_topics(&content);

        if let Some(node) = tree.nodes.get_mut(child_id) {
            // Only set summary if node doesn't already have one
            if node.summary.is_none() {
                node.summary = Some(summary);
            }
            if !entities.is_empty() {
                node.metadata.insert(
                    "entities".to_string(),
                    serde_json::json!(entities),
                );
            }
            if !topics.is_empty() {
                node.metadata.insert(
                    "topics".to_string(),
                    serde_json::json!(topics),
                );
            }
            enriched_count += 1;
        }
    }

    enriched_count
}

/// Gather full text content from a node and its descendants (up to a limit).
fn gather_node_content(tree: &DocumentTree, node_id: &str) -> String {
    let mut parts = Vec::new();
    let mut stack = vec![node_id.to_string()];
    let mut total_len = 0;

    while let Some(id) = stack.pop() {
        if total_len >= MAX_CONTENT_FOR_SUMMARY {
            break;
        }
        if let Some(node) = tree.get_node(&id) {
            if !node.content.is_empty() {
                parts.push(node.content.clone());
                total_len += node.content.len();
            }
            // Push children in reverse order so we process them left-to-right
            for child_id in node.children.iter().rev() {
                stack.push(child_id.clone());
            }
        }
    }

    parts.join(" ")
}

/// LLM-generated summary using the loaded local GGUF model.
/// Returns a 1-2 sentence summary of the provided content.
fn llm_summary(content: &str) -> Result<String, String> {
    let truncated = if content.len() > MAX_CONTENT_FOR_SUMMARY {
        &content[..MAX_CONTENT_FOR_SUMMARY]
    } else {
        content
    };
    let system = "You are a document analysis assistant. Write a concise 1-2 sentence summary of the provided text. Only output the summary, nothing else.";
    let user = format!("Summarize this document section:\n\n{}", truncated);
    let result = local::chat_inference(system, &user, 120)?;
    if result.is_empty() {
        Err("Empty LLM output".to_string())
    } else {
        Ok(result)
    }
}

/// Extractive summary: pick the first 1-2 meaningful sentences.
fn extractive_summary(content: &str) -> String {
    let truncated = if content.len() > MAX_CONTENT_FOR_SUMMARY {
        &content[..MAX_CONTENT_FOR_SUMMARY]
    } else {
        content
    };

    // Split into sentences (simple heuristic)
    let sentences: Vec<&str> = truncated
        .split(|c: char| c == '.' || c == '!' || c == '?')
        .map(|s| s.trim())
        .filter(|s| s.len() > 15) // skip very short fragments
        .collect();

    if sentences.is_empty() {
        // Fall back to first N chars
        let end = content.len().min(150);
        return content[..end].to_string();
    }

    // Take first 2 sentences
    let summary: Vec<String> = sentences.iter().take(2).map(|s| {
        format!("{}.", s)
    }).collect();

    summary.join(" ")
}

/// Extract entity-like patterns from text.
/// Looks for: capitalized multi-word names, dates, monetary amounts, percentages.
fn extract_entities(content: &str) -> Vec<String> {
    let mut entities = HashSet::new();

    // Monetary amounts: $1,234.56, $1.2M, etc.
    for word in content.split_whitespace() {
        let clean = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '$' && c != '.' && c != ',' && c != '%');
        if clean.starts_with('$') && clean.len() > 1 {
            entities.insert(clean.to_string());
        }
        // Percentages
        if clean.ends_with('%') && clean.len() > 1 {
            let num_part = &clean[..clean.len()-1];
            if num_part.chars().all(|c| c.is_ascii_digit() || c == '.') {
                entities.insert(clean.to_string());
            }
        }
    }

    // Dates: simple patterns like "January 2025", "Q3 2025", "2025-01-15"
    let words: Vec<&str> = content.split_whitespace().collect();
    let months = ["january", "february", "march", "april", "may", "june",
                  "july", "august", "september", "october", "november", "december"];
    for window in words.windows(2) {
        let w0_lower = window[0].to_lowercase();
        let w0_clean = w0_lower.trim_matches(|c: char| !c.is_alphanumeric());
        let w1_clean = window[1].trim_matches(|c: char| !c.is_alphanumeric());

        // "January 2025"
        if months.contains(&&*w0_clean) && w1_clean.len() == 4 && w1_clean.chars().all(|c| c.is_ascii_digit()) {
            entities.insert(format!("{} {}", window[0].trim_matches(|c: char| !c.is_alphanumeric()), w1_clean));
        }
        // "Q3 2025"
        if w0_clean.len() == 2
            && w0_clean.starts_with('q')
            && w0_clean.chars().nth(1).map(|c| c.is_ascii_digit()).unwrap_or(false)
            && w1_clean.len() == 4
            && w1_clean.chars().all(|c| c.is_ascii_digit())
        {
            entities.insert(format!("{} {}", w0_clean.to_uppercase(), w1_clean));
        }
    }

    // Capitalized multi-word names (2-4 consecutive capitalized words)
    let mut i = 0;
    while i < words.len() {
        let clean = words[i].trim_matches(|c: char| !c.is_alphanumeric());
        if clean.len() > 1 && clean.chars().next().map(|c| c.is_uppercase()).unwrap_or(false)
            && !clean.chars().all(|c| c.is_uppercase()) // skip ALL CAPS
        {
            let mut name_parts = vec![clean.to_string()];
            let mut j = i + 1;
            while j < words.len() && j - i < 4 {
                let next_clean = words[j].trim_matches(|c: char| !c.is_alphanumeric());
                if next_clean.len() > 1 && next_clean.chars().next().map(|c| c.is_uppercase()).unwrap_or(false)
                    && !next_clean.chars().all(|c| c.is_uppercase())
                {
                    name_parts.push(next_clean.to_string());
                    j += 1;
                } else {
                    break;
                }
            }
            if name_parts.len() >= 2 {
                // Skip common sentence starters
                let skip_words = ["The", "This", "That", "These", "Those", "When", "Where",
                    "What", "Which", "How", "There", "Here", "After", "Before", "During",
                    "While", "Since", "Until", "Because", "Although", "However", "Therefore",
                    "Furthermore", "Moreover", "Additionally", "Finally", "Section", "Chapter"];
                if !skip_words.contains(&name_parts[0].as_str()) {
                    entities.insert(name_parts.join(" "));
                }
            }
            i = j;
        } else {
            i += 1;
        }
    }

    let mut result: Vec<String> = entities.into_iter().collect();
    result.sort();
    result.truncate(10);
    result
}

/// Extract topic keywords from content using TF heuristics.
fn extract_topics(content: &str) -> Vec<String> {
    let stop_words: HashSet<&str> = [
        "the", "a", "an", "and", "or", "but", "in", "on", "at", "to", "for",
        "of", "with", "by", "from", "is", "are", "was", "were", "be", "been",
        "being", "have", "has", "had", "do", "does", "did", "will", "would",
        "could", "should", "may", "might", "shall", "can", "this", "that",
        "these", "those", "it", "its", "they", "them", "their", "we", "our",
        "you", "your", "he", "she", "his", "her", "not", "no", "if", "than",
        "then", "so", "as", "also", "which", "what", "when", "where", "who",
        "how", "all", "each", "every", "both", "few", "more", "most", "other",
        "some", "such", "only", "into", "over", "after", "before", "between",
        "about", "through", "during", "above", "below", "up", "down", "out",
        "off", "very", "just", "now", "here", "there", "new", "used", "using",
        "one", "two", "three", "four", "five", "first", "second", "third",
    ].into_iter().collect();

    let lower = content.to_lowercase();
    let mut freq: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

    for word in lower.split(|c: char| !c.is_alphanumeric()) {
        if word.len() > 3 && !stop_words.contains(word) && !word.chars().all(|c| c.is_ascii_digit()) {
            *freq.entry(word.to_string()).or_insert(0) += 1;
        }
    }

    let mut sorted: Vec<(String, usize)> = freq.into_iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1));

    sorted.into_iter().take(5).map(|(w, _)| w).collect()
}

/// A discovered cross-document relation (before persisting to DB).
#[derive(Debug, Clone)]
pub struct DiscoveredRelation {
    pub source_doc_id: String,
    pub source_node_id: String,
    pub target_doc_id: String,
    pub target_node_id: String,
    pub relation_type: String,
    pub confidence: f64,
    pub description: String,
}

/// Discover cross-document relations by comparing metadata across enriched trees.
/// Finds shared entities and overlapping topics between top-level nodes of different docs.
pub fn discover_cross_doc_relations(trees: &[DocumentTree]) -> Vec<DiscoveredRelation> {
    use std::collections::HashSet;

    if trees.len() < 2 {
        return Vec::new();
    }

    let mut relations = Vec::new();

    // Build per-node metadata index with pre-lowercased HashSets for O(1) lookups
    struct NodeEntry {
        doc_id: String,
        node_id: String,
        entities_original: Vec<String>,
        entities_lower: HashSet<String>,
        topics: HashSet<String>,
    }

    let mut node_meta: Vec<NodeEntry> = Vec::new();

    for tree in trees {
        let rich = tree.rich_overview();
        for node in &rich {
            if !node.entities.is_empty() || !node.topics.is_empty() {
                node_meta.push(NodeEntry {
                    doc_id: tree.id.clone(),
                    node_id: node.id.clone(),
                    entities_original: node.entities.clone(),
                    entities_lower: node.entities.iter().map(|e| e.to_lowercase()).collect(),
                    topics: node.topics.iter().cloned().collect(),
                });
            }
        }
    }

    // Compare every pair of nodes from different documents
    for i in 0..node_meta.len() {
        for j in (i + 1)..node_meta.len() {
            let a = &node_meta[i];
            let b = &node_meta[j];

            // Skip same-document pairs
            if a.doc_id == b.doc_id {
                continue;
            }

            // Shared entities — set intersection on pre-lowercased sets
            let shared_entities: Vec<&String> = a.entities_original
                .iter()
                .filter(|e| b.entities_lower.contains(&e.to_lowercase()))
                .collect();

            if !shared_entities.is_empty() {
                relations.push(DiscoveredRelation {
                    source_doc_id: a.doc_id.clone(),
                    source_node_id: a.node_id.clone(),
                    target_doc_id: b.doc_id.clone(),
                    target_node_id: b.node_id.clone(),
                    relation_type: "shared_entity".to_string(),
                    confidence: (shared_entities.len() as f64 / a.entities_original.len().max(1) as f64).min(1.0),
                    description: format!("Shared entities: {}", shared_entities.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")),
                });
            }

            // Topic overlap — set intersection (at least 2 shared topics)
            let shared_topics: Vec<&String> = a.topics.intersection(&b.topics).collect();

            if shared_topics.len() >= 2 {
                relations.push(DiscoveredRelation {
                    source_doc_id: a.doc_id.clone(),
                    source_node_id: a.node_id.clone(),
                    target_doc_id: b.doc_id.clone(),
                    target_node_id: b.node_id.clone(),
                    relation_type: "topic_overlap".to_string(),
                    confidence: (shared_topics.len() as f64 / a.topics.len().max(1) as f64).min(1.0),
                    description: format!("Shared topics: {}", shared_topics.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")),
                });
            }
        }
    }

    relations
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::tree::{DocType, NodeType, TreeNode};

    #[test]
    fn enrich_adds_metadata_to_nodes() {
        let mut tree = DocumentTree::new("test.pdf".to_string(), DocType::Pdf);
        let root_id = tree.root_id.clone();

        let section = TreeNode::new(
            NodeType::Section,
            "Financial Results for Q3 2025".to_string(),
        );
        let section_id = section.id.clone();
        tree.add_node(&root_id, section).unwrap();

        let para = TreeNode::new(
            NodeType::Paragraph,
            "ACME Corporation reported revenue of $1.2M in Q3 2025, representing a 15% increase year-over-year. The company's operating margin improved to 23%, driven by cost reduction initiatives.".to_string(),
        );
        tree.add_node(&section_id, para).unwrap();

        let count = enrich_tree_metadata(&mut tree);
        assert_eq!(count, 1);

        let node = tree.get_node(&section_id).unwrap();
        assert!(node.summary.is_some(), "Should have a summary");
        assert!(
            node.metadata.contains_key("entities"),
            "Should have entities"
        );
        assert!(
            node.metadata.contains_key("topics"),
            "Should have topics"
        );
    }

    #[test]
    fn enrich_skips_short_content() {
        let mut tree = DocumentTree::new("test.pdf".to_string(), DocType::Pdf);
        let root_id = tree.root_id.clone();

        let section = TreeNode::new(NodeType::Section, "Short".to_string());
        let section_id = section.id.clone();
        tree.add_node(&root_id, section).unwrap();

        let count = enrich_tree_metadata(&mut tree);
        assert_eq!(count, 0);

        let node = tree.get_node(&section_id).unwrap();
        assert!(node.summary.is_none());
    }

    #[test]
    fn extract_entities_finds_money_and_names() {
        let text = "ACME Corporation reported revenue of $1.2M in Q3 2025. CEO John Smith announced expansion plans for January 2026.";
        let entities = extract_entities(text);

        assert!(
            entities.iter().any(|e| e.contains("$1.2M")),
            "Should find monetary amount, got: {:?}",
            entities
        );
    }

    #[test]
    fn extract_topics_returns_frequent_words() {
        let text = "Machine learning algorithms process data through neural networks. Deep learning models use layered neural network architectures for pattern recognition. Training involves processing large datasets through these networks.";
        let topics = extract_topics(text);

        assert!(!topics.is_empty());
        assert!(
            topics.iter().any(|t| t.contains("learning") || t.contains("network")),
            "Should find relevant topics, got: {:?}",
            topics
        );
    }

    #[test]
    fn enrich_preserves_existing_summary() {
        let mut tree = DocumentTree::new("test.pdf".to_string(), DocType::Pdf);
        let root_id = tree.root_id.clone();

        let mut section = TreeNode::new(
            NodeType::Section,
            "Some section with enough content to be enriched by the metadata system".to_string(),
        );
        section.summary = Some("Pre-existing summary".to_string());
        let section_id = section.id.clone();
        tree.add_node(&root_id, section).unwrap();

        enrich_tree_metadata(&mut tree);

        let node = tree.get_node(&section_id).unwrap();
        assert_eq!(node.summary.as_deref(), Some("Pre-existing summary"));
    }

    #[test]
    fn extractive_summary_picks_first_sentences() {
        let text = "This is the first sentence about finance. The second sentence covers revenue growth. A third sentence about market expansion. Fourth about profits.";
        let summary = extractive_summary(text);
        assert!(summary.contains("first sentence"));
        assert!(summary.contains("second sentence"));
        // Should not contain 3rd or 4th
        assert!(!summary.contains("Fourth"));
    }

    #[test]
    fn discover_relations_finds_shared_entities() {
        let mut tree_a = DocumentTree::new("report.pdf".to_string(), DocType::Pdf);
        let root_a = tree_a.root_id.clone();
        let mut s1 = TreeNode::new(NodeType::Section, "Revenue Analysis".to_string());
        s1.metadata.insert("entities".to_string(), serde_json::json!(["ACME Corp", "$1.2M", "Q3 2025"]));
        s1.metadata.insert("topics".to_string(), serde_json::json!(["revenue", "finance", "quarterly"]));
        tree_a.add_node(&root_a, s1).unwrap();

        let mut tree_b = DocumentTree::new("contract.docx".to_string(), DocType::Word);
        let root_b = tree_b.root_id.clone();
        let mut s2 = TreeNode::new(NodeType::Section, "Agreement Terms".to_string());
        s2.metadata.insert("entities".to_string(), serde_json::json!(["ACME Corp", "January 2026"]));
        s2.metadata.insert("topics".to_string(), serde_json::json!(["contract", "terms", "finance"]));
        tree_b.add_node(&root_b, s2).unwrap();

        let relations = discover_cross_doc_relations(&[tree_a, tree_b]);
        assert!(!relations.is_empty(), "Should find at least one relation");

        let shared_entity = relations.iter().find(|r| r.relation_type == "shared_entity");
        assert!(shared_entity.is_some(), "Should find shared_entity relation");
        assert!(
            shared_entity.unwrap().description.contains("ACME Corp"),
            "Should mention shared entity"
        );
    }

    #[test]
    fn discover_relations_skips_single_doc() {
        let tree = DocumentTree::new("only.pdf".to_string(), DocType::Pdf);
        let relations = discover_cross_doc_relations(&[tree]);
        assert!(relations.is_empty());
    }

    #[test]
    fn discover_relations_requires_two_shared_topics() {
        let mut tree_a = DocumentTree::new("a.pdf".to_string(), DocType::Pdf);
        let root_a = tree_a.root_id.clone();
        let mut s1 = TreeNode::new(NodeType::Section, "Section A".to_string());
        s1.metadata.insert("topics".to_string(), serde_json::json!(["finance", "revenue", "growth"]));
        s1.metadata.insert("entities".to_string(), serde_json::json!([]));
        tree_a.add_node(&root_a, s1).unwrap();

        let mut tree_b = DocumentTree::new("b.pdf".to_string(), DocType::Pdf);
        let root_b = tree_b.root_id.clone();
        let mut s2 = TreeNode::new(NodeType::Section, "Section B".to_string());
        // Only 1 shared topic — should NOT create a relation
        s2.metadata.insert("topics".to_string(), serde_json::json!(["finance", "marketing", "branding"]));
        s2.metadata.insert("entities".to_string(), serde_json::json!([]));
        tree_b.add_node(&root_b, s2).unwrap();

        let relations = discover_cross_doc_relations(&[tree_a, tree_b]);
        let topic_relations: Vec<_> = relations.iter().filter(|r| r.relation_type == "topic_overlap").collect();
        assert!(topic_relations.is_empty(), "Should require at least 2 shared topics");
    }
}
