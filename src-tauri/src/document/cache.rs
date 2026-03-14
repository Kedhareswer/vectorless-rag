use std::collections::{HashMap, VecDeque};
use super::tree::DocumentTree;

/// Simple bounded cache for parsed DocumentTree objects.
/// Uses FIFO eviction when capacity is exceeded.
pub struct TreeCache {
    map: HashMap<String, DocumentTree>,
    order: VecDeque<String>,
    capacity: usize,
}

impl TreeCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            map: HashMap::with_capacity(capacity),
            order: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    pub fn get(&self, doc_id: &str) -> Option<&DocumentTree> {
        self.map.get(doc_id)
    }

    #[allow(clippy::map_entry)] // entry() borrows map, preventing eviction loop
    pub fn insert(&mut self, doc_id: String, tree: DocumentTree) {
        if self.map.contains_key(&doc_id) {
            // Update existing entry
            self.map.insert(doc_id, tree);
            return;
        }

        // Evict oldest if at capacity
        while self.map.len() >= self.capacity {
            if let Some(oldest) = self.order.pop_front() {
                self.map.remove(&oldest);
            } else {
                break;
            }
        }

        self.order.push_back(doc_id.clone());
        self.map.insert(doc_id, tree);
    }

    pub fn invalidate(&mut self, doc_id: &str) {
        self.map.remove(doc_id);
        self.order.retain(|id| id != doc_id);
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::tree::{DocType, DocumentTree};

    fn make_tree(name: &str) -> DocumentTree {
        DocumentTree::new(name.to_string(), DocType::PlainText)
    }

    #[test]
    fn insert_and_get() {
        let mut cache = TreeCache::new(5);
        let tree = make_tree("test.txt");
        let id = tree.id.clone();
        cache.insert(id.clone(), tree);
        assert!(cache.get(&id).is_some());
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn eviction_at_capacity() {
        let mut cache = TreeCache::new(2);
        let t1 = make_tree("a.txt");
        let t2 = make_tree("b.txt");
        let t3 = make_tree("c.txt");
        let id1 = t1.id.clone();
        let id2 = t2.id.clone();
        let id3 = t3.id.clone();

        cache.insert(id1.clone(), t1);
        cache.insert(id2.clone(), t2);
        assert_eq!(cache.len(), 2);

        cache.insert(id3.clone(), t3);
        assert_eq!(cache.len(), 2);
        // Oldest (id1) should be evicted
        assert!(cache.get(&id1).is_none());
        assert!(cache.get(&id2).is_some());
        assert!(cache.get(&id3).is_some());
    }

    #[test]
    fn invalidate_removes() {
        let mut cache = TreeCache::new(5);
        let tree = make_tree("test.txt");
        let id = tree.id.clone();
        cache.insert(id.clone(), tree);
        assert_eq!(cache.len(), 1);
        cache.invalidate(&id);
        assert_eq!(cache.len(), 0);
        assert!(cache.get(&id).is_none());
    }

    #[test]
    fn update_existing() {
        let mut cache = TreeCache::new(5);
        let tree = make_tree("test.txt");
        let id = tree.id.clone();
        cache.insert(id.clone(), tree);
        let updated = make_tree("updated.txt");
        cache.insert(id.clone(), updated);
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.get(&id).unwrap().name, "updated.txt");
    }
}
