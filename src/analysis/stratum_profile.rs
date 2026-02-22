use crate::algebra::address::ChunkId;
use crate::algebra::triad::StratumHistogram;
use crate::analysis::classifier::{self, ChunkClass};

/// Full analysis result for a chunk.
#[derive(Debug)]
pub struct ChunkProfile {
    pub id: ChunkId,
    pub histogram: StratumHistogram,
    pub classification: ChunkClass,
}

impl ChunkProfile {
    /// Analyze a chunk and produce its profile.
    pub fn analyze(id: ChunkId, data: &[u8]) -> Self {
        let histogram = StratumHistogram::from_bytes(data);
        let classification = classifier::classify(&histogram);
        Self {
            id,
            histogram,
            classification,
        }
    }
}
