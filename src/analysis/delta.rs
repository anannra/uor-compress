use crate::algebra::address::ChunkId;
use crate::algebra::correlation;
use crate::analysis::classifier::ChunkClass;

/// Minimum fidelity threshold for considering a chunk as a near-duplicate.
const NEAR_DUPLICATE_THRESHOLD: f64 = 0.85;

/// Maximum number of recent chunks to compare against for delta detection.
const MAX_CANDIDATES: usize = 64;

/// A candidate base chunk for delta compression.
#[derive(Debug)]
struct Candidate {
    id: ChunkId,
    /// First N bytes for quick pre-screening.
    prefix: Vec<u8>,
}

/// Delta detector: finds similar chunks for delta compression.
pub struct DeltaDetector {
    candidates: Vec<Candidate>,
    /// Full chunk data for candidates (stored separately for cache efficiency).
    candidate_data: Vec<Vec<u8>>,
    threshold: f64,
}

impl DeltaDetector {
    pub fn new() -> Self {
        Self {
            candidates: Vec::new(),
            candidate_data: Vec::new(),
            threshold: NEAR_DUPLICATE_THRESHOLD,
        }
    }

    pub fn with_threshold(mut self, threshold: f64) -> Self {
        self.threshold = threshold;
        self
    }

    /// Register a chunk as a potential delta base.
    pub fn register(&mut self, id: ChunkId, data: &[u8]) {
        let prefix = data[..data.len().min(64)].to_vec();
        self.candidates.push(Candidate { id, prefix });
        self.candidate_data.push(data.to_vec());

        // Evict oldest candidates if we exceed the limit.
        if self.candidates.len() > MAX_CANDIDATES {
            self.candidates.remove(0);
            self.candidate_data.remove(0);
        }
    }

    /// Find the best delta base for a chunk, if any meets the threshold.
    /// Returns NearDuplicate classification with base ID and fidelity,
    /// or None if no suitable base was found.
    pub fn find_base(&self, data: &[u8]) -> Option<ChunkClass> {
        if self.candidates.is_empty() {
            return None;
        }

        let mut best_fidelity = 0.0f64;
        let mut best_id = None;

        for (i, candidate) in self.candidates.iter().enumerate() {
            let candidate_data = &self.candidate_data[i];

            // Only compare chunks of the same length for delta encoding.
            if candidate_data.len() != data.len() {
                continue;
            }

            // Quick pre-screen using prefix correlation.
            let prefix_len = candidate.prefix.len().min(data.len());
            let prefix_corr =
                correlation::correlate(&candidate.prefix[..prefix_len], &data[..prefix_len]);
            if prefix_corr.fidelity < self.threshold * 0.9 {
                continue;
            }

            // Full correlation check.
            let corr = correlation::correlate(candidate_data, data);
            if corr.fidelity > best_fidelity {
                best_fidelity = corr.fidelity;
                best_id = Some(candidate.id);
            }
        }

        if best_fidelity >= self.threshold {
            Some(ChunkClass::NearDuplicate {
                base: best_id.unwrap(),
                fidelity: best_fidelity,
            })
        } else {
            None
        }
    }
}

impl Default for DeltaDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_candidates_returns_none() {
        let detector = DeltaDetector::new();
        let data = vec![0u8; 100];
        assert!(detector.find_base(&data).is_none());
    }

    #[test]
    fn identical_chunk_detected() {
        let mut detector = DeltaDetector::new();
        let data = vec![42u8; 1000];
        let id = ChunkId::from_data(&data);
        detector.register(id, &data);

        let result = detector.find_base(&data);
        assert!(result.is_some());
        match result.unwrap() {
            ChunkClass::NearDuplicate { fidelity, .. } => {
                assert_eq!(fidelity, 1.0);
            }
            other => panic!("Expected NearDuplicate, got {other:?}"),
        }
    }

    #[test]
    fn slightly_different_chunk_detected() {
        let mut detector = DeltaDetector::new();
        let base = vec![42u8; 1000];
        let id = ChunkId::from_data(&base);
        detector.register(id, &base);

        // Change a few bytes (< 15% of bits should differ).
        let mut modified = base.clone();
        for i in 0..10 {
            modified[i] = modified[i].wrapping_add(1);
        }

        let result = detector.find_base(&modified);
        assert!(result.is_some());
    }

    #[test]
    fn very_different_chunk_not_detected() {
        let mut detector = DeltaDetector::new();
        let base = vec![0x00u8; 1000];
        let id = ChunkId::from_data(&base);
        detector.register(id, &base);

        let different = vec![0xFFu8; 1000];
        let result = detector.find_base(&different);
        assert!(result.is_none());
    }
}
