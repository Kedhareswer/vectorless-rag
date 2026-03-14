use std::collections::HashMap;

use crate::document::tree::{DocumentTree, NodeType};
use crate::util::safe_truncate;
use super::query::{ProcessedQuery, QueryIntent};

/// A labeled section of fetched content ready for the LLM.
#[derive(Debug, Clone)]
pub struct FetchedSection {
    pub doc_name: String,
    pub section_title: String,
    pub content: String,
    /// Node IDs touched by this fetch (for trace/highlight).
    pub node_ids: Vec<String>,
}

/// Result of deterministic content fetching.
#[derive(Debug, Clone)]
pub struct FetchedContent {
    pub sections: Vec<FetchedSection>,
    pub fetch_steps: Vec<FetchStep>,
    /// Total chars of content fetched.
    pub total_chars: usize,
}

/// A trace-friendly record of what the fetcher did.
#[derive(Debug, Clone)]
pub struct FetchStep {
    pub step_number: u32,
    pub operation: String,
    pub description: String,
    pub node_ids: Vec<String>,
}

/// Maximum total content characters to fetch (keeps LLM call efficient).
/// 40k chars ≈ 10k tokens — enough for thorough coverage while keeping
/// LLM response times fast. The agentic tool loop can fetch more if needed.
const CONTENT_BUDGET: usize = 40_000;
/// Max sections to expand per document for summarize queries.
const MAX_SUMMARIZE_SECTIONS: usize = 24;
/// Max search results to expand.
const MAX_SEARCH_EXPAND: usize = 12;

/// Deterministically fetch content based on query intent and document trees.
/// No LLM involved — the code decides what to read.
pub fn fetch_content(
    query: &ProcessedQuery,
    trees: &[DocumentTree],
) -> FetchedContent {
    let mut sections = Vec::new();
    let mut steps = Vec::new();
    let mut step_num = 0u32;
    let mut total_chars = 0usize;

    match query.intent {
        QueryIntent::Summarize => {
            fetch_summarize(trees, &mut sections, &mut steps, &mut step_num, &mut total_chars);
        }
        QueryIntent::Factual | QueryIntent::Specific => {
            fetch_search_and_expand(
                trees, &query.search_terms, &query.original,
                &mut sections, &mut steps, &mut step_num, &mut total_chars,
            );
        }
        QueryIntent::Entity => {
            fetch_entity(
                trees, &query.search_terms, &query.original,
                &mut sections, &mut steps, &mut step_num, &mut total_chars,
            );
        }
        QueryIntent::Comparison => {
            // For comparisons: fetch from each doc, search terms help find relevant sections
            fetch_search_and_expand(
                trees, &query.search_terms, &query.original,
                &mut sections, &mut steps, &mut step_num, &mut total_chars,
            );
        }
        QueryIntent::ListExtract => {
            fetch_list_extract(
                trees, &query.search_terms,
                &mut sections, &mut steps, &mut step_num, &mut total_chars,
            );
        }
    }

    FetchedContent {
        sections,
        fetch_steps: steps,
        total_chars,
    }
}

/// Format fetched content into a string for the LLM prompt.
pub fn format_for_prompt(fetched: &FetchedContent) -> String {
    let mut parts = Vec::new();
    let mut current_doc = String::new();

    for section in &fetched.sections {
        if section.doc_name != current_doc {
            current_doc = section.doc_name.clone();
            parts.push(format!("\n=== Document: {} ===", current_doc));
        }
        parts.push(format!(
            "\n## {}\n{}",
            section.section_title, section.content
        ));
    }

    parts.join("\n")
}

// ── Fetch Strategies ────────────────────────────────────────────────

/// Summarize: expand top-level sections from ALL documents.
/// When metadata summaries are available, uses them for efficient coverage;
/// otherwise falls back to expanding full section content.
fn fetch_summarize(
    trees: &[DocumentTree],
    sections: &mut Vec<FetchedSection>,
    steps: &mut Vec<FetchStep>,
    step_num: &mut u32,
    total_chars: &mut usize,
) {
    for tree in trees {
        let rich = tree.rich_overview();
        *step_num += 1;
        steps.push(FetchStep {
            step_number: *step_num,
            operation: "tree_overview".to_string(),
            description: format!("Scanning structure of \"{}\"", tree.name),
            node_ids: rich.iter().map(|n| n.id.clone()).collect(),
        });

        // Expand each top-level section (up to budget)
        for node_summary in rich.iter().take(MAX_SUMMARIZE_SECTIONS) {
            if *total_chars >= CONTENT_BUDGET {
                break;
            }

            // If a metadata summary exists, use it for compact representation
            // (lets us fit more sections within the budget)
            let content = if let Some(ref summary_text) = node_summary.summary {
                let mut text = summary_text.clone();
                // Append entity/topic tags if available
                if !node_summary.entities.is_empty() {
                    text.push_str(&format!("\nEntities: {}", node_summary.entities.join(", ")));
                }
                if !node_summary.topics.is_empty() {
                    text.push_str(&format!("\nTopics: {}", node_summary.topics.join(", ")));
                }
                text
            } else if let Some(node) = tree.get_node(&node_summary.id) {
                // No metadata — fall back to expanding full content including all children
                let mut text = node.content.clone();
                let children = tree.get_children(&node_summary.id);
                for child in &children {
                    if !child.content.is_empty() && child.content.len() > 2 {
                        text.push_str("\n\n");
                        text.push_str(&child.content);
                        // Also include grandchildren (e.g. table rows inside a table node)
                        let grandchildren = tree.get_children(&child.id);
                        for gc in &grandchildren {
                            if !gc.content.is_empty() {
                                text.push('\n');
                                text.push_str(&gc.content);
                            }
                        }
                    }
                }
                text
            } else {
                continue;
            };

            // Respect budget
            let content = if *total_chars + content.len() > CONTENT_BUDGET {
                let remaining = CONTENT_BUDGET.saturating_sub(*total_chars);
                if remaining > 200 {
                    safe_truncate(&content, remaining).to_string()
                } else {
                    break;
                }
            } else {
                content
            };

            *step_num += 1;
            let node_ids = vec![node_summary.id.clone()];

            steps.push(FetchStep {
                step_number: *step_num,
                operation: "expand".to_string(),
                description: format!(
                    "Reading \"{}\" from \"{}\"",
                    safe_truncate(&node_summary.content_preview, 60),
                    tree.name
                ),
                node_ids: node_ids.clone(),
            });

            *total_chars += content.len();
            sections.push(FetchedSection {
                doc_name: tree.name.clone(),
                section_title: node_summary.content_preview.clone(),
                content,
                node_ids,
            });
        }
    }
}

/// Factual/Specific/Comparison: search for relevant content, then expand matching nodes.
fn fetch_search_and_expand(
    trees: &[DocumentTree],
    search_terms: &[String],
    original_query: &str,
    sections: &mut Vec<FetchedSection>,
    steps: &mut Vec<FetchStep>,
    step_num: &mut u32,
    total_chars: &mut usize,
) {
    // Build search queries: use extracted terms, fall back to full query words
    let queries: Vec<String> = if search_terms.is_empty() {
        original_query
            .split_whitespace()
            .filter(|w| w.len() > 3)
            .take(3)
            .map(|w| w.to_lowercase())
            .collect()
    } else {
        search_terms.iter().take(4).cloned().collect()
    };

    let mut expanded_ids = std::collections::HashSet::new();

    for tree in trees {
        // Build parent map ONCE per tree — shared across all find_* calls
        let parent_map = build_parent_map(tree);

        for query_term in &queries {
            if *total_chars >= CONTENT_BUDGET {
                break;
            }

            let query_lower = query_term.to_lowercase();
            let mut matches: Vec<(String, String)> = Vec::new(); // (node_id, preview)

            for (id, node) in &tree.nodes {
                if node.content.to_lowercase().contains(&query_lower)
                    && !matches!(node.node_type, NodeType::Root)
                {
                    let preview = if node.content.len() > 80 {
                        format!("{}...", safe_truncate(&node.content, 80))
                    } else {
                        node.content.clone()
                    };
                    matches.push((id.clone(), preview));
                }
            }

            *step_num += 1;
            steps.push(FetchStep {
                step_number: *step_num,
                operation: "search".to_string(),
                description: format!(
                    "Searching \"{}\" in \"{}\" — {} matches",
                    query_term, tree.name, matches.len()
                ),
                node_ids: matches.iter().map(|(id, _)| id.clone()).collect(),
            });

            // Expand the best matches
            for (node_id, _preview) in matches.iter().take(MAX_SEARCH_EXPAND) {
                if *total_chars >= CONTENT_BUDGET || expanded_ids.contains(node_id) {
                    continue;
                }
                expanded_ids.insert(node_id.clone());

                if let Some(node) = tree.get_node(node_id) {
                    let content = if matches!(node.node_type, NodeType::TableRow) {
                        let header = find_table_header_with_map(tree, node_id, &parent_map);
                        if let Some(h) = header {
                            format!("Columns: {}\nRow: {}", h, node.content)
                        } else {
                            node.content.clone()
                        }
                    } else {
                        node.content.clone()
                    };
                    if content.len() < 20 {
                        continue;
                    }

                    let section_title = find_section_title_with_map(tree, node_id, &parent_map);

                    *step_num += 1;
                    steps.push(FetchStep {
                        step_number: *step_num,
                        operation: "expand".to_string(),
                        description: format!(
                            "Reading matched content from \"{}\"",
                            tree.name
                        ),
                        node_ids: vec![node_id.clone()],
                    });

                    let truncated = if content.len() + *total_chars > CONTENT_BUDGET {
                        let remaining = CONTENT_BUDGET.saturating_sub(*total_chars);
                        safe_truncate(&content, remaining).to_string()
                    } else {
                        content.clone()
                    };

                    *total_chars += truncated.len();
                    sections.push(FetchedSection {
                        doc_name: tree.name.clone(),
                        section_title,
                        content: truncated,
                        node_ids: vec![node_id.clone()],
                    });
                }
            }
        }

        // If no search matches found, fall back to summarize strategy for this doc
        if sections.iter().all(|s| s.doc_name != tree.name) {
            fetch_summarize(
                std::slice::from_ref(tree),
                sections, steps, step_num, total_chars,
            );
        }
    }
}

/// Entity: search for entity-like terms (names, orgs) across all trees.
fn fetch_entity(
    trees: &[DocumentTree],
    search_terms: &[String],
    original_query: &str,
    sections: &mut Vec<FetchedSection>,
    steps: &mut Vec<FetchStep>,
    step_num: &mut u32,
    total_chars: &mut usize,
) {
    // Entity queries: search terms are the entity names
    // Fall back to search_and_expand which already handles this well
    fetch_search_and_expand(
        trees, search_terms, original_query,
        sections, steps, step_num, total_chars,
    );
}

/// ListExtract: find sections with list/table nodes and expand them.
fn fetch_list_extract(
    trees: &[DocumentTree],
    search_terms: &[String],
    sections: &mut Vec<FetchedSection>,
    steps: &mut Vec<FetchStep>,
    step_num: &mut u32,
    total_chars: &mut usize,
) {
    for tree in trees {
        // Build parent map ONCE per tree
        let parent_map = build_parent_map(tree);

        // Find nodes that are lists, tables, or contain list-like content
        let mut list_nodes: Vec<String> = Vec::new();

        for (id, node) in &tree.nodes {
            if matches!(
                node.node_type,
                NodeType::Table | NodeType::TableRow | NodeType::ListItem
            ) {
                // Find the parent section rather than individual items
                if let Some(parent_id) = find_parent_section_id_with_map(tree, id, &parent_map) {
                    if !list_nodes.contains(&parent_id) {
                        list_nodes.push(parent_id);
                    }
                }
            }
        }

        // If search terms provided, filter to matching sections
        if !search_terms.is_empty() {
            list_nodes.retain(|id| {
                tree.get_node(id)
                    .map(|n| {
                        let lower = n.content.to_lowercase();
                        search_terms.iter().any(|t| lower.contains(&t.to_lowercase()))
                    })
                    .unwrap_or(false)
            });
        }

        *step_num += 1;
        steps.push(FetchStep {
            step_number: *step_num,
            operation: "scan_lists".to_string(),
            description: format!(
                "Found {} list/table sections in \"{}\"",
                list_nodes.len(), tree.name
            ),
            node_ids: list_nodes.clone(),
        });

        // Expand list sections
        for node_id in list_nodes.iter().take(MAX_SEARCH_EXPAND) {
            if *total_chars >= CONTENT_BUDGET {
                break;
            }

            if let Some(node) = tree.get_node(node_id) {
                let mut content = node.content.clone();

                // Include children (list items, table rows)
                let children = tree.get_children(node_id);
                for child in &children {
                    if !child.content.is_empty() {
                        content.push('\n');
                        content.push_str(&child.content);
                    }
                }

                let section_title = find_section_title_with_map(tree, node_id, &parent_map);

                *step_num += 1;
                let node_ids: Vec<String> = std::iter::once(node_id.clone())
                    .chain(children.iter().map(|c| c.id.clone()))
                    .collect();

                steps.push(FetchStep {
                    step_number: *step_num,
                    operation: "expand".to_string(),
                    description: format!(
                        "Reading list/table \"{}\" from \"{}\"",
                        safe_truncate(&section_title, 40),
                        tree.name
                    ),
                    node_ids: node_ids.clone(),
                });

                let truncated = if content.len() + *total_chars > CONTENT_BUDGET {
                    let remaining = CONTENT_BUDGET.saturating_sub(*total_chars);
                    safe_truncate(&content, remaining).to_string()
                } else {
                    content
                };

                *total_chars += truncated.len();
                sections.push(FetchedSection {
                    doc_name: tree.name.clone(),
                    section_title,
                    content: truncated,
                    node_ids,
                });
            }
        }

        // Fall back to summarize if no lists found
        if list_nodes.is_empty() {
            fetch_summarize(
                std::slice::from_ref(tree),
                sections, steps, step_num, total_chars,
            );
        }
    }
}

// ── Helpers ─────────────────────────────────────────────────────────

/// Build a child→parent map for a tree. Called once per tree, then shared
/// across all helper lookups (avoids O(n) rebuild per call).
fn build_parent_map(tree: &DocumentTree) -> HashMap<String, String> {
    let mut parent_map: HashMap<String, String> = HashMap::with_capacity(tree.nodes.len());
    for (id, node) in &tree.nodes {
        for child_id in &node.children {
            parent_map.insert(child_id.clone(), id.clone());
        }
    }
    parent_map
}

/// Walk up the tree to find the nearest section title for a node.
fn find_section_title_with_map(tree: &DocumentTree, node_id: &str, parent_map: &HashMap<String, String>) -> String {
    let mut current = node_id.to_string();
    loop {
        if let Some(node) = tree.get_node(&current) {
            if matches!(node.node_type, NodeType::Section | NodeType::Heading | NodeType::Root) {
                let title = if node.content.len() > 80 {
                    format!("{}...", safe_truncate(&node.content, 80))
                } else {
                    node.content.clone()
                };
                return title;
            }
        }
        match parent_map.get(&current) {
            Some(parent_id) => current = parent_id.clone(),
            None => break,
        }
    }

    tree.get_node(node_id)
        .map(|n| {
            if n.content.len() > 80 {
                format!("{}...", safe_truncate(&n.content, 80))
            } else {
                n.content.clone()
            }
        })
        .unwrap_or_else(|| "Untitled".to_string())
}

/// For a TableRow node, find the header row content from the same Table parent.
fn find_table_header_with_map(tree: &DocumentTree, node_id: &str, parent_map: &HashMap<String, String>) -> Option<String> {
    let table_id = {
        let mut current = node_id.to_string();
        loop {
            match parent_map.get(&current) {
                Some(pid) => {
                    if let Some(p) = tree.get_node(pid) {
                        if matches!(p.node_type, NodeType::Table) {
                            break Some(pid.clone());
                        }
                    }
                    current = pid.clone();
                }
                None => break None,
            }
        }
    };

    let table_id = table_id?;
    let table = tree.get_node(&table_id)?;

    if let Some(cols) = table.metadata.get("columns") {
        if let Some(arr) = cols.as_array() {
            let names: Vec<&str> = arr.iter().filter_map(|v| v.as_str()).collect();
            if !names.is_empty() {
                return Some(names.join(" | "));
            }
        }
    }

    let children = tree.get_children(&table_id);
    for child in &children {
        if child.metadata.get("is_header").and_then(|v| v.as_bool()).unwrap_or(false) {
            return Some(child.content.clone());
        }
    }

    None
}

/// Find the parent section ID for a node (for grouping list items under their section).
fn find_parent_section_id_with_map(tree: &DocumentTree, node_id: &str, parent_map: &HashMap<String, String>) -> Option<String> {
    let mut current = node_id.to_string();
    loop {
        match parent_map.get(&current) {
            Some(parent_id) => {
                if let Some(parent) = tree.get_node(parent_id) {
                    if matches!(parent.node_type, NodeType::Section | NodeType::Root) {
                        return Some(parent_id.clone());
                    }
                }
                current = parent_id.clone();
            }
            None => return None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::tree::{DocumentTree, DocType, TreeNode, NodeType};

    fn make_test_tree(name: &str, section_count: usize) -> DocumentTree {
        let mut tree = DocumentTree::new(name.to_string(), DocType::Markdown);
        let root_id = tree.root_id.clone();

        for i in 0..section_count {
            let section = TreeNode::new(
                NodeType::Section,
                format!("Section {} Title", i + 1),
            );
            let section_id = section.id.clone();
            tree.add_node(&root_id, section).unwrap();

            // Add a paragraph child
            let para = TreeNode::new(
                NodeType::Paragraph,
                format!("This is the detailed content of section {}. It contains important information about topic {}.", i + 1, i + 1),
            );
            tree.add_node(&section_id, para).unwrap();
        }

        tree
    }

    #[test]
    fn fetch_summarize_single_doc() {
        let tree = make_test_tree("test.md", 3);
        let query = ProcessedQuery {
            original: "summarize".to_string(),
            intent: QueryIntent::Summarize,
            search_terms: vec![],
            exploration_hint: String::new(),
            min_tool_calls: 3,
            recommended_max_steps: 10,
        };

        let result = fetch_content(&query, &[tree]);
        assert_eq!(result.sections.len(), 3, "Should fetch all 3 sections");
        assert!(result.total_chars > 0);
        assert!(!result.fetch_steps.is_empty());
        // All sections should be from the same doc
        for section in &result.sections {
            assert_eq!(section.doc_name, "test.md");
        }
    }

    #[test]
    fn fetch_summarize_multi_doc() {
        let tree1 = make_test_tree("doc1.md", 2);
        let tree2 = make_test_tree("doc2.md", 2);
        let query = ProcessedQuery {
            original: "summarize".to_string(),
            intent: QueryIntent::Summarize,
            search_terms: vec![],
            exploration_hint: String::new(),
            min_tool_calls: 3,
            recommended_max_steps: 10,
        };

        let result = fetch_content(&query, &[tree1, tree2]);
        assert_eq!(result.sections.len(), 4, "Should fetch all sections from both docs");

        let doc1_sections: Vec<_> = result.sections.iter().filter(|s| s.doc_name == "doc1.md").collect();
        let doc2_sections: Vec<_> = result.sections.iter().filter(|s| s.doc_name == "doc2.md").collect();
        assert_eq!(doc1_sections.len(), 2);
        assert_eq!(doc2_sections.len(), 2);
    }

    #[test]
    fn fetch_factual_finds_matching_content() {
        let mut tree = DocumentTree::new("test.md".to_string(), DocType::Markdown);
        let root_id = tree.root_id.clone();

        let s1 = TreeNode::new(NodeType::Section, "Introduction to Rust Programming".to_string());
        let s1_id = s1.id.clone();
        tree.add_node(&root_id, s1).unwrap();
        let p1 = TreeNode::new(NodeType::Paragraph, "Rust is a systems programming language focused on safety and performance.".to_string());
        tree.add_node(&s1_id, p1).unwrap();

        let s2 = TreeNode::new(NodeType::Section, "Python Overview".to_string());
        let s2_id = s2.id.clone();
        tree.add_node(&root_id, s2).unwrap();
        let p2 = TreeNode::new(NodeType::Paragraph, "Python is an interpreted language used for scripting and data science.".to_string());
        tree.add_node(&s2_id, p2).unwrap();

        let query = ProcessedQuery {
            original: "what is rust".to_string(),
            intent: QueryIntent::Factual,
            search_terms: vec!["rust".to_string()],
            exploration_hint: String::new(),
            min_tool_calls: 2,
            recommended_max_steps: 8,
        };

        let result = fetch_content(&query, &[tree]);
        // Should find the Rust section, not the Python section
        assert!(!result.sections.is_empty());
        let rust_sections: Vec<_> = result.sections.iter()
            .filter(|s| s.content.to_lowercase().contains("rust"))
            .collect();
        assert!(!rust_sections.is_empty(), "Should find Rust-related content");
    }

    #[test]
    fn fetch_respects_content_budget() {
        // Create a tree with very large sections
        let mut tree = DocumentTree::new("big.md".to_string(), DocType::Markdown);
        let root_id = tree.root_id.clone();

        for i in 0..20 {
            let content = format!("Section {} content: {}", i, "x".repeat(5000));
            let section = TreeNode::new(NodeType::Section, content);
            tree.add_node(&root_id, section).unwrap();
        }

        let query = ProcessedQuery {
            original: "summarize".to_string(),
            intent: QueryIntent::Summarize,
            search_terms: vec![],
            exploration_hint: String::new(),
            min_tool_calls: 3,
            recommended_max_steps: 10,
        };

        let result = fetch_content(&query, &[tree]);
        assert!(
            result.total_chars <= CONTENT_BUDGET + 1000, // small overflow tolerance
            "Total chars {} should be near budget {}",
            result.total_chars, CONTENT_BUDGET
        );
    }

    #[test]
    fn format_for_prompt_labels_documents() {
        let tree1 = make_test_tree("report.pdf", 1);
        let tree2 = make_test_tree("notes.md", 1);
        let query = ProcessedQuery {
            original: "summarize".to_string(),
            intent: QueryIntent::Summarize,
            search_terms: vec![],
            exploration_hint: String::new(),
            min_tool_calls: 3,
            recommended_max_steps: 10,
        };

        let result = fetch_content(&query, &[tree1, tree2]);
        let formatted = format_for_prompt(&result);

        assert!(formatted.contains("=== Document: report.pdf ==="));
        assert!(formatted.contains("=== Document: notes.md ==="));
    }

    #[test]
    fn fetch_summarize_uses_metadata_when_available() {
        let mut tree = DocumentTree::new("enriched.pdf".to_string(), DocType::Pdf);
        let root_id = tree.root_id.clone();

        let mut s1 = TreeNode::new(NodeType::Section, "Financial Results".to_string());
        s1.summary = Some("Q3 2025 revenue was $1.2M, up 15% YoY.".to_string());
        s1.metadata.insert("entities".to_string(), serde_json::json!(["ACME Corp", "$1.2M"]));
        s1.metadata.insert("topics".to_string(), serde_json::json!(["finance", "revenue"]));
        tree.add_node(&root_id, s1).unwrap();

        let s2 = TreeNode::new(NodeType::Section, "No metadata here".to_string());
        let s2_id = s2.id.clone();
        tree.add_node(&root_id, s2).unwrap();
        let p = TreeNode::new(NodeType::Paragraph, "Raw content from paragraph.".to_string());
        tree.add_node(&s2_id, p).unwrap();

        let query = ProcessedQuery {
            original: "summarize".to_string(),
            intent: QueryIntent::Summarize,
            search_terms: vec![],
            exploration_hint: String::new(),
            min_tool_calls: 3,
            recommended_max_steps: 10,
        };

        let result = fetch_content(&query, &[tree]);
        assert_eq!(result.sections.len(), 2);

        // First section should use metadata summary
        assert!(result.sections[0].content.contains("Q3 2025 revenue"));
        assert!(result.sections[0].content.contains("ACME Corp"));
        assert!(result.sections[0].content.contains("finance"));

        // Second section should use raw content fallback
        assert!(result.sections[1].content.contains("Raw content from paragraph"));
    }

    #[test]
    fn fetch_search_falls_back_to_summarize_on_no_matches() {
        let tree = make_test_tree("test.md", 3);
        let query = ProcessedQuery {
            original: "what about quantum physics".to_string(),
            intent: QueryIntent::Factual,
            search_terms: vec!["quantum".to_string(), "physics".to_string()],
            exploration_hint: String::new(),
            min_tool_calls: 2,
            recommended_max_steps: 8,
        };

        let result = fetch_content(&query, &[tree]);
        // No search matches, but should fall back to summarize
        assert!(!result.sections.is_empty(), "Should fall back to summarize when no search matches");
    }
}
