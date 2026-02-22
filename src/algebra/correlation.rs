/// Correlation result between two byte slices.
/// Maps PRISM's correlate() method.
#[derive(Debug, Clone)]
pub struct Correlation {
    /// Number of differing bits (Hamming distance).
    pub hamming_distance: u32,
    /// Total bits compared.
    pub max_distance: u32,
    /// 1.0 - (hamming / max), range [0.0, 1.0].
    pub fidelity: f64,
}

/// Compute Hamming correlation between two byte slices.
/// Slices must be the same length.
///
/// Fidelity of 1.0 means identical, 0.0 means maximally different.
pub fn correlate(a: &[u8], b: &[u8]) -> Correlation {
    assert_eq!(a.len(), b.len(), "correlate requires equal-length slices");

    let hamming_distance: u32 = a
        .iter()
        .zip(b.iter())
        .map(|(&x, &y)| (x ^ y).count_ones())
        .sum();

    let max_distance = (a.len() as u32) * 8;
    let fidelity = if max_distance == 0 {
        1.0
    } else {
        1.0 - (hamming_distance as f64 / max_distance as f64)
    };

    Correlation {
        hamming_distance,
        max_distance,
        fidelity,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_data() {
        let data = [0x42, 0x55, 0xAA];
        let c = correlate(&data, &data);
        assert_eq!(c.hamming_distance, 0);
        assert_eq!(c.fidelity, 1.0);
    }

    #[test]
    fn complementary_data() {
        let a = [0x55u8];
        let b = [0xAAu8];
        let c = correlate(&a, &b);
        assert_eq!(c.hamming_distance, 8);
        assert_eq!(c.max_distance, 8);
        assert!((c.fidelity - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn one_bit_difference() {
        let a = [0x00u8];
        let b = [0x01u8];
        let c = correlate(&a, &b);
        assert_eq!(c.hamming_distance, 1);
        assert!((c.fidelity - 0.875).abs() < f64::EPSILON);
    }

    #[test]
    fn multi_byte() {
        let a = [0x00u8, 0x00];
        let b = [0xFF, 0xFF];
        let c = correlate(&a, &b);
        assert_eq!(c.hamming_distance, 16);
        assert_eq!(c.max_distance, 16);
        assert!((c.fidelity - 0.0).abs() < f64::EPSILON);
    }
}
