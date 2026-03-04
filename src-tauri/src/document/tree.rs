use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

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
                        format!("{}...", &node.content[..100])
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
