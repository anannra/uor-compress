/// Ring operations on bytes in Z/256Z.
/// Port of PRISM's UOR class primitive operations (Quantum 0).
pub struct ByteRing;

impl ByteRing {
    /// Additive inverse: (-x) mod 256
    #[inline]
    pub fn neg(byte: u8) -> u8 {
        byte.wrapping_neg()
    }

    /// Bitwise complement: 255 XOR x
    #[inline]
    pub fn bnot(byte: u8) -> u8 {
        !byte
    }

    /// Bitwise exclusive or
    #[inline]
    pub fn xor(a: u8, b: u8) -> u8 {
        a ^ b
    }

    /// Bitwise and
    #[inline]
    pub fn band(a: u8, b: u8) -> u8 {
        a & b
    }

    /// Bitwise or
    #[inline]
    pub fn bor(a: u8, b: u8) -> u8 {
        a | b
    }

    /// Successor: neg(bnot(x)) = x + 1 mod 256. The critical identity.
    #[inline]
    pub fn succ(byte: u8) -> u8 {
        Self::neg(Self::bnot(byte))
    }

    /// Predecessor: bnot(neg(x)) = x - 1 mod 256.
    #[inline]
    pub fn pred(byte: u8) -> u8 {
        Self::bnot(Self::neg(byte))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exhaustive_neg_involution() {
        for x in 0u8..=255 {
            assert_eq!(ByteRing::neg(ByteRing::neg(x)), x);
        }
    }

    #[test]
    fn exhaustive_bnot_involution() {
        for x in 0u8..=255 {
            assert_eq!(ByteRing::bnot(ByteRing::bnot(x)), x);
        }
    }

    #[test]
    fn exhaustive_critical_identity_succ() {
        for x in 0u8..=255 {
            assert_eq!(ByteRing::neg(ByteRing::bnot(x)), x.wrapping_add(1));
            assert_eq!(ByteRing::succ(x), x.wrapping_add(1));
        }
    }

    #[test]
    fn exhaustive_critical_identity_pred() {
        for x in 0u8..=255 {
            assert_eq!(ByteRing::bnot(ByteRing::neg(x)), x.wrapping_sub(1));
            assert_eq!(ByteRing::pred(x), x.wrapping_sub(1));
        }
    }

    #[test]
    fn xor_self_annihilation() {
        for x in 0u8..=255 {
            assert_eq!(ByteRing::xor(x, x), 0);
        }
    }

    #[test]
    fn stratum_symmetry() {
        for x in 0u8..=255 {
            assert_eq!(
                x.count_ones() + ByteRing::bnot(x).count_ones(),
                8
            );
        }
    }
}
