/// Triadic coordinates for a single byte.
/// Maps PRISM's Triad dataclass: datum, stratum, spectrum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ByteTriad {
    /// The byte value itself (identity).
    pub datum: u8,
    /// Popcount: number of set bits (magnitude).
    pub stratum: u8,
    /// Bit positions that are set (structure).
    pub spectrum: [bool; 8],
}

impl ByteTriad {
    pub fn new(byte: u8) -> Self {
        let mut spectrum = [false; 8];
        for i in 0..8 {
            spectrum[i] = (byte >> i) & 1 == 1;
        }
        Self {
            datum: byte,
            stratum: byte.count_ones() as u8,
            spectrum,
        }
    }

    /// Return the bit positions that are set, as a Vec of indices.
    pub fn set_positions(&self) -> Vec<u8> {
        self.spectrum
            .iter()
            .enumerate()
            .filter(|(_, &set)| set)
            .map(|(i, _)| i as u8)
            .collect()
    }
}

/// Stratum histogram: counts of bytes at each popcount level (0..=8).
/// 9 bins, used as a fast entropy proxy for chunk classification.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StratumHistogram {
    pub bins: [u32; 9],
    pub total_bytes: u32,
}

impl StratumHistogram {
    /// Build a stratum histogram from a byte slice.
    pub fn from_bytes(data: &[u8]) -> Self {
        let mut bins = [0u32; 9];
        for &byte in data {
            bins[byte.count_ones() as usize] += 1;
        }
        Self {
            bins,
            total_bytes: data.len() as u32,
        }
    }

    /// Fraction of bytes at a given stratum.
    pub fn density(&self, stratum: usize) -> f64 {
        if self.total_bytes == 0 {
            return 0.0;
        }
        self.bins[stratum] as f64 / self.total_bytes as f64
    }

    /// Mean stratum across all bytes.
    pub fn mean_stratum(&self) -> f64 {
        if self.total_bytes == 0 {
            return 0.0;
        }
        let weighted_sum: u64 = self
            .bins
            .iter()
            .enumerate()
            .map(|(i, &count)| i as u64 * count as u64)
            .sum();
        weighted_sum as f64 / self.total_bytes as f64
    }

    /// Variance of the stratum distribution.
    pub fn variance(&self) -> f64 {
        if self.total_bytes == 0 {
            return 0.0;
        }
        let mean = self.mean_stratum();
        let weighted_sq_sum: f64 = self
            .bins
            .iter()
            .enumerate()
            .map(|(i, &count)| {
                let diff = i as f64 - mean;
                diff * diff * count as f64
            })
            .sum();
        weighted_sq_sum / self.total_bytes as f64
    }

    /// Fraction of bytes at extreme strata (0 or 8).
    pub fn extreme_density(&self) -> f64 {
        if self.total_bytes == 0 {
            return 0.0;
        }
        (self.bins[0] + self.bins[8]) as f64 / self.total_bytes as f64
    }

    /// Compact 9-byte summary (each bin quantized to u8, saturating).
    pub fn to_summary(&self) -> [u8; 9] {
        let mut summary = [0u8; 9];
        if self.total_bytes == 0 {
            return summary;
        }
        for (i, &count) in self.bins.iter().enumerate() {
            let frac = (count as f64 / self.total_bytes as f64 * 255.0) as u32;
            summary[i] = frac.min(255) as u8;
        }
        summary
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn triad_zero() {
        let t = ByteTriad::new(0x00);
        assert_eq!(t.stratum, 0);
        assert_eq!(t.set_positions(), Vec::<u8>::new());
    }

    #[test]
    fn triad_ff() {
        let t = ByteTriad::new(0xFF);
        assert_eq!(t.stratum, 8);
        assert_eq!(t.set_positions(), vec![0, 1, 2, 3, 4, 5, 6, 7]);
    }

    #[test]
    fn triad_55() {
        let t = ByteTriad::new(0x55);
        assert_eq!(t.stratum, 4);
        assert_eq!(t.set_positions(), vec![0, 2, 4, 6]);
    }

    #[test]
    fn histogram_all_zeros() {
        let data = vec![0u8; 100];
        let h = StratumHistogram::from_bytes(&data);
        assert_eq!(h.bins[0], 100);
        assert_eq!(h.extreme_density(), 1.0);
        assert_eq!(h.mean_stratum(), 0.0);
    }

    #[test]
    fn histogram_all_ff() {
        let data = vec![0xFFu8; 50];
        let h = StratumHistogram::from_bytes(&data);
        assert_eq!(h.bins[8], 50);
        assert_eq!(h.extreme_density(), 1.0);
        assert_eq!(h.mean_stratum(), 8.0);
    }

    #[test]
    fn histogram_mixed() {
        let data = vec![0x00, 0xFF, 0x55, 0xAA];
        let h = StratumHistogram::from_bytes(&data);
        assert_eq!(h.bins[0], 1); // 0x00
        assert_eq!(h.bins[4], 2); // 0x55, 0xAA
        assert_eq!(h.bins[8], 1); // 0xFF
        assert_eq!(h.total_bytes, 4);
    }

    #[test]
    fn binomial_distribution_check() {
        // Count how many bytes have each stratum value.
        // Should match C(8, k) for each k.
        let expected = [1u32, 8, 28, 56, 70, 56, 28, 8, 1];
        let all_bytes: Vec<u8> = (0u16..=255).map(|x| x as u8).collect();
        let h = StratumHistogram::from_bytes(&all_bytes);
        assert_eq!(h.bins, expected);
    }
}
