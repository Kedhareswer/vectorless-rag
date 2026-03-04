use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ImageNode {
    pub id: String,
    pub path: String,
    pub mime_type: String,
    pub description: Option<String>,
    pub dimensions: Option<(u32, u32)>,
}

/// Extract images from a file path. Currently a stub that returns an empty vec.
/// Future implementations will support PDF image extraction, embedded images, etc.
pub fn extract_images_from_path(_path: &str) -> Vec<ImageNode> {
    Vec::new()
}
