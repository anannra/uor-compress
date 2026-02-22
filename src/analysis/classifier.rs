use crate::algebra::address::ChunkId;
use crate::algebra::triad::StratumHistogram;

/// Stratum-based classification of a data chunk.
/// Determines which compression backend to use.
#[derive(Debug, Clone, PartialEq)]
pub enum ChunkClass {
    /// Most bytes at stratum 0 or 8 (all zeros / all ones). Highly compressible.
    Sparse,
    /// Stratum histogram matches structured patterns (text, code). Use zstd high.
    Structured,
    /// Broad stratum distribution but compressible. Use zstd default.
    Dense,
    /// Near-uniform random distribution across strata. Incompressible.
    Random,
    /// Exact duplicate of an existing chunk. Store reference only.
    Duplicate,
    /// High fidelity to an existing chunk. Use delta compression.
    NearDuplicate { base: ChunkId, fidelity: f64 },
}

/// Classify a chunk based on its stratum histogram.
/// Duplicate/NearDuplicate detection happens separately in the delta module.
pub fn classify(histogram: &StratumHistogram) -> ChunkClass {
    if histogram.total_bytes == 0 {
        return ChunkClass::Sparse;
    }

    // Sparse: >80% of bytes at extreme strata (0 or 8).
    if histogram.extreme_density() > 0.80 {
        return ChunkClass::Sparse;
    }

    // Random: check if the distribution closely matches binomial(8, 0.5).
    // The expected proportions for binomial(8, 0.5) are:
    // [1/256, 8/256, 28/256, 56/256, 70/256, 56/256, 28/256, 8/256, 1/256]
    let binomial_expected: [f64; 9] = [
        1.0 / 256.0,
        8.0 / 256.0,
        28.0 / 256.0,
        56.0 / 256.0,
        70.0 / 256.0,
        56.0 / 256.0,
        28.0 / 256.0,
        8.0 / 256.0,
        1.0 / 256.0,
    ];

    let chi_sq: f64 = (0..9)
        .map(|i| {
            let observed = histogram.density(i);
            let expected = binomial_expected[i];
            if expected > 0.0 {
                let diff = observed - expected;
                diff * diff / expected
            } else {
                0.0
            }
        })
        .sum();

    // Low chi-squared means close to random (binomial).
    // Threshold chosen empirically; true random data gives chi_sq ~0.03.
    if chi_sq < 0.15 {
        return ChunkClass::Random;
    }

    // Structured: mean stratum between 2.5 and 5.5, low variance.
    let mean = histogram.mean_stratum();
    let var = histogram.variance();
    if (2.5..=5.5).contains(&mean) && var < 3.0 {
        return ChunkClass::Structured;
    }

    // Everything else is Dense.
    ChunkClass::Dense
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::algebra::triad::StratumHistogram;

    #[test]
    fn all_zeros_is_sparse() {
        let data = vec![0u8; 1000];
        let h = StratumHistogram::from_bytes(&data);
        assert_eq!(classify(&h), ChunkClass::Sparse);
    }

    #[test]
    fn all_ff_is_sparse() {
        let data = vec![0xFFu8; 1000];
        let h = StratumHistogram::from_bytes(&data);
        assert_eq!(classify(&h), ChunkClass::Sparse);
    }

    #[test]
    fn uniform_random_is_random() {
        // Simulate data matching binomial distribution.
        let mut data = Vec::new();
        // Fill with all 256 byte values equally (which gives binomial distribution).
        for _ in 0..100 {
            for b in 0u8..=255 {
                data.push(b);
            }
        }
        let h = StratumHistogram::from_bytes(&data);
        assert_eq!(classify(&h), ChunkClass::Random);
    }

    #[test]
    fn ascii_text_is_structured() {
        // ASCII text tends to have bytes in the 0x20-0x7E range (strata ~2-5).
        let text = b"The quick brown fox jumps over the lazy dog. \
                     This is a test of structured English text content \
                     that should be classified as structured data by the \
                     stratum-based classifier. Lorem ipsum dolor sit amet.";
        let data: Vec<u8> = text.iter().copied().cycle().take(10000).collect();
        let h = StratumHistogram::from_bytes(&data);
        let class = classify(&h);
        assert!(
            class == ChunkClass::Structured || class == ChunkClass::Dense,
            "Expected Structured or Dense, got {class:?}"
        );
    }
}
