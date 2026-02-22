use std::collections::HashMap;

use crate::algebra::address::ChunkId;

/// Deduplicating chunk store indexed by content address.
/// Identical chunks (by SHA-256) are stored once.
pub struct ChunkStore {
    /// Map from ChunkId to (index in chunks vec, chunk data).
    index: HashMap<ChunkId, u32>,
    /// Stored chunk data in insertion order.
    chunks: Vec<StoredChunk>,
}

/// A chunk stored with its metadata.
pub struct StoredChunk {
    pub id: ChunkId,
    pub data: Vec<u8>,
    pub ref_count: u32,
}

impl ChunkStore {
    pub fn new() -> Self {
        Self {
            index: HashMap::new(),
            chunks: Vec::new(),
        }
    }

    /// Insert a chunk. Returns (index, is_new).
    /// If the chunk already exists, increments ref_count and returns false.
    pub fn insert(&mut self, id: ChunkId, data: Vec<u8>) -> (u32, bool) {
        if let Some(&idx) = self.index.get(&id) {
            self.chunks[idx as usize].ref_count += 1;
            (idx, false)
        } else {
            let idx = self.chunks.len() as u32;
            self.index.insert(id, idx);
            self.chunks.push(StoredChunk {
                id,
                data,
                ref_count: 1,
            });
            (idx, true)
        }
    }

    /// Look up a chunk by ID.
    pub fn get(&self, id: &ChunkId) -> Option<&StoredChunk> {
        self.index.get(id).map(|&idx| &self.chunks[idx as usize])
    }

    /// Get a chunk by its index.
    pub fn get_by_index(&self, idx: u32) -> Option<&StoredChunk> {
        self.chunks.get(idx as usize)
    }

    /// Number of unique chunks stored.
    pub fn len(&self) -> usize {
        self.chunks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.chunks.is_empty()
    }

    /// Total deduplicated references (including duplicates).
    pub fn total_refs(&self) -> u32 {
        self.chunks.iter().map(|c| c.ref_count).sum()
    }

    /// Iterate over all stored chunks.
    pub fn iter(&self) -> impl Iterator<Item = &StoredChunk> {
        self.chunks.iter()
    }

    /// Check if a chunk ID exists in the store.
    pub fn contains(&self, id: &ChunkId) -> bool {
        self.index.contains_key(id)
    }
}

impl Default for ChunkStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_and_retrieve() {
        let mut store = ChunkStore::new();
        let id = ChunkId::from_data(b"hello");
        let (idx, is_new) = store.insert(id, b"hello".to_vec());
        assert!(is_new);
        assert_eq!(idx, 0);
        assert_eq!(store.len(), 1);

        let chunk = store.get(&id).unwrap();
        assert_eq!(chunk.data, b"hello");
        assert_eq!(chunk.ref_count, 1);
    }

    #[test]
    fn deduplication() {
        let mut store = ChunkStore::new();
        let id = ChunkId::from_data(b"hello");

        let (idx1, new1) = store.insert(id, b"hello".to_vec());
        let (idx2, new2) = store.insert(id, b"hello".to_vec());

        assert!(new1);
        assert!(!new2);
        assert_eq!(idx1, idx2);
        assert_eq!(store.len(), 1);
        assert_eq!(store.get(&id).unwrap().ref_count, 2);
    }

    #[test]
    fn distinct_chunks() {
        let mut store = ChunkStore::new();
        let id_a = ChunkId::from_data(b"aaa");
        let id_b = ChunkId::from_data(b"bbb");

        store.insert(id_a, b"aaa".to_vec());
        store.insert(id_b, b"bbb".to_vec());

        assert_eq!(store.len(), 2);
        assert_eq!(store.total_refs(), 2);
    }
}
