use crate::algebra::address::ChunkId;

/// Parameters for content-defined chunking.
#[derive(Debug, Clone)]
pub struct ChunkParams {
    pub min_size: usize,
    pub target_size: usize,
    pub max_size: usize,
}

impl Default for ChunkParams {
    fn default() -> Self {
        Self {
            min_size: 4096,
            target_size: 16384,
            max_size: 65536,
        }
    }
}

/// A chunk produced by content-defined chunking.
#[derive(Debug)]
pub struct Chunk {
    pub id: ChunkId,
    pub offset: u64,
    pub data: Vec<u8>,
}

impl Chunk {
    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

/// Content-defined chunker using Gear rolling hash.
pub struct Chunker {
    params: ChunkParams,
    gear_table: [u64; 256],
}

impl Chunker {
    pub fn new(params: ChunkParams) -> Self {
        Self {
            params,
            gear_table: Self::build_gear_table(),
        }
    }

    /// Build a deterministic gear hash lookup table.
    fn build_gear_table() -> [u64; 256] {
        let mut table = [0u64; 256];
        // Use a simple deterministic PRNG seeded with a fixed value.
        let mut state: u64 = 0x5851_F42D_4C95_7F2D;
        for entry in &mut table {
            // xorshift64
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            *entry = state;
        }
        table
    }

    /// Split input data into content-defined chunks.
    pub fn chunk(&self, data: &[u8]) -> Vec<Chunk> {
        if data.is_empty() {
            return vec![];
        }

        let mut chunks = Vec::new();
        let mut start = 0;
        let mask = self.target_mask();

        while start < data.len() {
            let remaining = data.len() - start;
            let max_end = start + remaining.min(self.params.max_size);

            // Skip to minimum size before checking for boundaries.
            let scan_start = start + self.params.min_size.min(remaining);
            let mut end = max_end; // Default: cut at max_size.

            // Gear rolling hash to find boundary.
            let mut hash: u64 = 0;
            for pos in scan_start..max_end {
                hash = hash.wrapping_shl(1).wrapping_add(self.gear_table[data[pos] as usize]);
                if hash & mask == 0 {
                    end = pos + 1;
                    break;
                }
            }

            let chunk_data = data[start..end].to_vec();
            let id = ChunkId::from_data(&chunk_data);
            chunks.push(Chunk {
                id,
                offset: start as u64,
                data: chunk_data,
            });
            start = end;
        }

        chunks
    }

    /// Compute the mask for target chunk size.
    /// For a target of N bytes, we want roughly 1/N probability of boundary.
    fn target_mask(&self) -> u64 {
        let bits = (self.params.target_size as f64).log2().ceil() as u32;
        (1u64 << bits) - 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input() {
        let chunker = Chunker::new(ChunkParams::default());
        let chunks = chunker.chunk(&[]);
        assert!(chunks.is_empty());
    }

    #[test]
    fn small_input_single_chunk() {
        let chunker = Chunker::new(ChunkParams::default());
        let data = vec![42u8; 100];
        let chunks = chunker.chunk(&data);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].data, data);
        assert_eq!(chunks[0].offset, 0);
    }

    #[test]
    fn deterministic() {
        let chunker = Chunker::new(ChunkParams::default());
        let data: Vec<u8> = (0..100_000).map(|i| (i * 37 + 13) as u8).collect();
        let chunks1 = chunker.chunk(&data);
        let chunks2 = chunker.chunk(&data);
        assert_eq!(chunks1.len(), chunks2.len());
        for (a, b) in chunks1.iter().zip(chunks2.iter()) {
            assert_eq!(a.id, b.id);
            assert_eq!(a.offset, b.offset);
        }
    }

    #[test]
    fn respects_max_size() {
        let params = ChunkParams {
            min_size: 100,
            target_size: 500,
            max_size: 1000,
        };
        let chunker = Chunker::new(params);
        let data = vec![0u8; 5000];
        let chunks = chunker.chunk(&data);
        for chunk in &chunks {
            assert!(chunk.len() <= 1000);
        }
    }

    #[test]
    fn covers_all_data() {
        let chunker = Chunker::new(ChunkParams::default());
        let data: Vec<u8> = (0..100_000).map(|i| (i * 37 + 13) as u8).collect();
        let chunks = chunker.chunk(&data);
        let total: usize = chunks.iter().map(|c| c.len()).sum();
        assert_eq!(total, data.len());

        // Verify contiguous coverage.
        let mut offset = 0u64;
        for chunk in &chunks {
            assert_eq!(chunk.offset, offset);
            offset += chunk.len() as u64;
        }
    }
}
