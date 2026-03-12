use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

use crate::util::safe_truncate;

#[derive(Error, Debug)]
pub enum TreeError {
    #[error("Node not found: {0}")]
    NodeNotFound(String),
    #[error("Parent node not found: {0}")]
    ParentNotFound(String),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DocType {
    Pdf,
    Markdown,
    PlainText,
    Code,
    Word,
    Csv,
    Spreadsheet,
    Image,
    Unknown,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum NodeType {
    Root,
    Section,
    Paragraph,
    Heading,
    Table,
    TableRow,
    TableCell,
    Image,
    CodeBlock,
    ListItem,
    Link,
    Unknown,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RelationType {
    References,
    DependsOn,
    SimilarTo,
    Contains,
    LinkedTo,
    // Cross-document relation types
    SharedEntity,
    TopicOverlap,
    Contradiction,
    Supports,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Relation {
    pub target_id: String,
    pub relation_type: RelationType,
    pub label: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TreeNode {
    pub id: String,
    pub node_type: NodeType,
    pub content: String,
    pub metadata: HashMap<String, serde_json::Value>,
    pub children: Vec<String>,
    pub relations: Vec<Relation>,
    pub summary: Option<String>,
    pub raw_image_path: Option<String>,
}

impl TreeNode {
    pub fn new(node_type: NodeType, content: String) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            node_type,
            content,
            metadata: HashMap::new(),
            children: Vec::new(),
            relations: Vec::new(),
            summary: None,
            raw_image_path: None,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TreeNodeSummary {
    pub id: String,
    pub node_type: NodeType,
    pub content_preview: String,
    pub children_count: usize,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RichNodeSummary {
    pub id: String,
    pub node_type: NodeType,
    pub content_preview: String,
    pub children_count: usize,
    pub summary: Option<String>,
    pub entities: Vec<String>,
    pub topics: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DocumentTree {
    pub id: String,
    pub name: String,
    pub doc_type: DocType,
    pub root_id: String,
    pub nodes: HashMap<String, TreeNode>,
    pub created_at: String,
    pub updated_at: String,
}

impl DocumentTree {
    pub fn new(name: String, doc_type: DocType) -> Self {
        let root = TreeNode::new(NodeType::Root, name.clone());
        let root_id = root.id.clone();
        let mut nodes = HashMap::new();
        nodes.insert(root_id.clone(), root);
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            doc_type,
            root_id,
            nodes,
            created_at: now.clone(),
            updated_at: now,
        }
    }

    pub fn add_node(
        &mut self,
        parent_id: &str,
        node: TreeNode,
    ) -> Result<String, TreeError> {
        let node_id = node.id.clone();
        if !self.nodes.contains_key(parent_id) {
            return Err(TreeError::ParentNotFound(parent_id.to_string()));
        }
        self.nodes.insert(node_id.clone(), node);
        if let Some(parent) = self.nodes.get_mut(parent_id) {
            parent.children.push(node_id.clone());
        }
        self.updated_at = chrono::Utc::now().to_rfc3339();
        Ok(node_id)
    }

    pub fn get_node(&self, id: &str) -> Option<&TreeNode> {
        self.nodes.get(id)
    }

    pub fn get_children(&self, id: &str) -> Vec<&TreeNode> {
        match self.nodes.get(id) {
            Some(node) => node
                .children
                .iter()
                .filter_map(|child_id| self.nodes.get(child_id))
                .collect(),
            None => Vec::new(),
        }
    }

    /// Rich overview that includes metadata (summary, entities, topics) when available.
    /// Used by the deterministic fetcher and UI to understand document structure
    /// without expanding every node.
    pub fn rich_overview(&self) -> Vec<RichNodeSummary> {
        let root = match self.nodes.get(&self.root_id) {
            Some(r) => r,
            None => return Vec::new(),
        };
        root.children
            .iter()
            .filter_map(|child_id| {
                self.nodes.get(child_id).map(|node| {
                    let preview = if node.content.len() > 100 {
                        format!("{}...", safe_truncate(&node.content, 100))
                    } else {
                        node.content.clone()
                    };
                    let summary = node.summary.clone()
                        .or_else(|| node.metadata.get("summary").and_then(|v| v.as_str()).map(String::from));
                    let entities: Vec<String> = node.metadata.get("entities")
                        .and_then(|v| v.as_array())
                        .map(|arr| arr.iter().filter_map(|e| e.as_str().map(String::from)).collect())
                        .unwrap_or_default();
                    let topics: Vec<String> = node.metadata.get("topics")
                        .and_then(|v| v.as_array())
                        .map(|arr| arr.iter().filter_map(|t| t.as_str().map(String::from)).collect())
                        .unwrap_or_default();
                    RichNodeSummary {
                        id: node.id.clone(),
                        node_type: node.node_type.clone(),
                        content_preview: preview,
                        children_count: node.children.len(),
                        summary,
                        entities,
                        topics,
                    }
                })
            })
            .collect()
    }

    pub fn tree_overview(&self) -> Vec<TreeNodeSummary> {
        let root = match self.nodes.get(&self.root_id) {
            Some(r) => r,
            None => return Vec::new(),
        };
        root.children
            .iter()
            .filter_map(|child_id| {
                self.nodes.get(child_id).map(|node| {
                    let preview = if node.content.len() > 100 {
                        format!("{}...", safe_truncate(&node.content, 100))
                    } else {
                        node.content.clone()
                    };
                    TreeNodeSummary {
                        id: node.id.clone(),
                        node_type: node.node_type.clone(),
                        content_preview: preview,
                        children_count: node.children.len(),
                    }
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tree_node_new_creates_correct_node() {
        let node = TreeNode::new(NodeType::Section, "Hello".to_string());
        assert_eq!(node.node_type, NodeType::Section);
        assert_eq!(node.content, "Hello");
        assert!(!node.id.is_empty());
        assert!(node.children.is_empty());
        assert!(node.relations.is_empty());
        assert!(node.summary.is_none());
        assert!(node.raw_image_path.is_none());
        assert!(node.metadata.is_empty());
    }

    #[test]
    fn document_tree_new_creates_tree_with_root() {
        let tree = DocumentTree::new("test.md".to_string(), DocType::Markdown);
        assert_eq!(tree.name, "test.md");
        assert_eq!(tree.doc_type, DocType::Markdown);
        assert!(!tree.id.is_empty());
        assert!(!tree.root_id.is_empty());

        let root = tree.get_node(&tree.root_id).expect("root should exist");
        assert_eq!(root.node_type, NodeType::Root);
        assert_eq!(root.content, "test.md");
    }

    #[test]
    fn add_node_to_valid_parent() {
        let mut tree = DocumentTree::new("doc".to_string(), DocType::PlainText);
        let root_id = tree.root_id.clone();
        let child = TreeNode::new(NodeType::Paragraph, "para 1".to_string());
        let child_id_expected = child.id.clone();

        let result = tree.add_node(&root_id, child);
        assert!(result.is_ok());
        let child_id = result.unwrap();
        assert_eq!(child_id, child_id_expected);

        // Parent's children list should contain the new node
        let root = tree.get_node(&root_id).unwrap();
        assert!(root.children.contains(&child_id));

        // Node should be retrievable
        let fetched = tree.get_node(&child_id).unwrap();
        assert_eq!(fetched.content, "para 1");
    }

    #[test]
    fn add_node_with_invalid_parent_returns_error() {
        let mut tree = DocumentTree::new("doc".to_string(), DocType::PlainText);
        let child = TreeNode::new(NodeType::Paragraph, "orphan".to_string());

        let result = tree.add_node("nonexistent-parent-id", child);
        assert!(result.is_err());
        let err = result.unwrap_err();
        match err {
            TreeError::ParentNotFound(id) => assert_eq!(id, "nonexistent-parent-id"),
            other => panic!("Expected ParentNotFound, got: {:?}", other),
        }
    }

    #[test]
    fn get_node_returns_some_for_existing_and_none_for_missing() {
        let tree = DocumentTree::new("doc".to_string(), DocType::Code);
        assert!(tree.get_node(&tree.root_id).is_some());
        assert!(tree.get_node("does-not-exist").is_none());
    }

    #[test]
    fn get_children_returns_children_in_order() {
        let mut tree = DocumentTree::new("doc".to_string(), DocType::Markdown);
        let root_id = tree.root_id.clone();

        let c1 = TreeNode::new(NodeType::Section, "first".to_string());
        let c2 = TreeNode::new(NodeType::Section, "second".to_string());
        let c3 = TreeNode::new(NodeType::Section, "third".to_string());

        let id1 = tree.add_node(&root_id, c1).unwrap();
        let id2 = tree.add_node(&root_id, c2).unwrap();
        let id3 = tree.add_node(&root_id, c3).unwrap();

        let children = tree.get_children(&root_id);
        assert_eq!(children.len(), 3);
        assert_eq!(children[0].id, id1);
        assert_eq!(children[1].id, id2);
        assert_eq!(children[2].id, id3);
        assert_eq!(children[0].content, "first");
        assert_eq!(children[1].content, "second");
        assert_eq!(children[2].content, "third");
    }

    #[test]
    fn get_children_with_nonexistent_id_returns_empty() {
        let tree = DocumentTree::new("doc".to_string(), DocType::PlainText);
        let children = tree.get_children("no-such-id");
        assert!(children.is_empty());
    }

    #[test]
    fn tree_overview_returns_root_children_summaries() {
        let mut tree = DocumentTree::new("doc".to_string(), DocType::Pdf);
        let root_id = tree.root_id.clone();

        let s1 = TreeNode::new(NodeType::Section, "Introduction".to_string());
        let s2 = TreeNode::new(NodeType::Section, "Methods".to_string());

        let id1 = tree.add_node(&root_id, s1).unwrap();
        tree.add_node(&root_id, s2).unwrap();

        // Add a child to s1 to verify children_count
        let sub = TreeNode::new(NodeType::Paragraph, "sub-para".to_string());
        tree.add_node(&id1, sub).unwrap();

        let overview = tree.tree_overview();
        assert_eq!(overview.len(), 2);

        assert_eq!(overview[0].node_type, NodeType::Section);
        assert_eq!(overview[0].content_preview, "Introduction");
        assert_eq!(overview[0].children_count, 1);

        assert_eq!(overview[1].node_type, NodeType::Section);
        assert_eq!(overview[1].content_preview, "Methods");
        assert_eq!(overview[1].children_count, 0);
    }

    #[test]
    fn tree_overview_truncates_long_content() {
        let mut tree = DocumentTree::new("doc".to_string(), DocType::PlainText);
        let root_id = tree.root_id.clone();

        let long_content = "a".repeat(200);
        let node = TreeNode::new(NodeType::Paragraph, long_content);
        tree.add_node(&root_id, node).unwrap();

        let overview = tree.tree_overview();
        assert_eq!(overview.len(), 1);
        assert!(overview[0].content_preview.ends_with("..."));
        // The preview before "..." should be at most 100 chars
        let without_dots = overview[0].content_preview.trim_end_matches("...");
        assert!(without_dots.len() <= 100);
    }

    #[test]
    fn tree_overview_on_empty_tree_returns_empty() {
        let tree = DocumentTree::new("empty".to_string(), DocType::Unknown);
        let overview = tree.tree_overview();
        assert!(overview.is_empty());
    }

    #[test]
    fn rich_overview_without_metadata() {
        let mut tree = DocumentTree::new("doc".to_string(), DocType::Pdf);
        let root_id = tree.root_id.clone();
        let s1 = TreeNode::new(NodeType::Section, "Introduction".to_string());
        tree.add_node(&root_id, s1).unwrap();

        let rich = tree.rich_overview();
        assert_eq!(rich.len(), 1);
        assert_eq!(rich[0].content_preview, "Introduction");
        assert!(rich[0].summary.is_none());
        assert!(rich[0].entities.is_empty());
        assert!(rich[0].topics.is_empty());
    }

    #[test]
    fn rich_overview_with_metadata() {
        let mut tree = DocumentTree::new("doc".to_string(), DocType::Pdf);
        let root_id = tree.root_id.clone();
        let mut s1 = TreeNode::new(NodeType::Section, "Financial Data".to_string());
        s1.summary = Some("Q3 revenue figures for ACME Corp".to_string());
        s1.metadata.insert(
            "entities".to_string(),
            serde_json::json!(["ACME Corp", "$1.2M", "Q3 2025"]),
        );
        s1.metadata.insert(
            "topics".to_string(),
            serde_json::json!(["finance", "revenue", "quarterly"]),
        );
        tree.add_node(&root_id, s1).unwrap();

        let rich = tree.rich_overview();
        assert_eq!(rich.len(), 1);
        assert_eq!(rich[0].summary.as_deref(), Some("Q3 revenue figures for ACME Corp"));
        assert_eq!(rich[0].entities, vec!["ACME Corp", "$1.2M", "Q3 2025"]);
        assert_eq!(rich[0].topics, vec!["finance", "revenue", "quarterly"]);
    }

    #[test]
    fn rich_overview_prefers_summary_field_over_metadata() {
        let mut tree = DocumentTree::new("doc".to_string(), DocType::Pdf);
        let root_id = tree.root_id.clone();
        let mut s1 = TreeNode::new(NodeType::Section, "Test".to_string());
        s1.summary = Some("from summary field".to_string());
        s1.metadata.insert(
            "summary".to_string(),
            serde_json::json!("from metadata"),
        );
        tree.add_node(&root_id, s1).unwrap();

        let rich = tree.rich_overview();
        assert_eq!(rich[0].summary.as_deref(), Some("from summary field"));
    }
}
