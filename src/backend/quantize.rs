use crate::backend::traits::{BackendTag, CompressBackend, DecompressBackend};
use crate::error::{Error, Result};

/// Lossy compression backend: stratum-aware quantization.
///
/// For bytes with low stratum (few bits set), zeroes out the lowest-significance bits.
/// The quantized data then compresses better through zstd.
pub struct QuantizeBackend {
    /// Bytes with stratum <= this threshold are candidates for quantization.
    stratum_threshold: u8,
    /// Number of least-significant bits to clear in quantized bytes.
    bits_to_clear: u8,
    zstd_level: i32,
}

impl QuantizeBackend {
    pub fn new(stratum_threshold: u8, bits_to_clear: u8, zstd_level: i32) -> Self {
        Self {
            stratum_threshold,
            bits_to_clear,
            zstd_level,
        }
    }

    /// Quantize a byte slice: for low-stratum bytes, clear LSBs.
    pub fn quantize(&self, data: &[u8]) -> Vec<u8> {
        let mask = !((1u8 << self.bits_to_clear) - 1);
        data.iter()
            .map(|&byte| {
                if byte.count_ones() as u8 <= self.stratum_threshold {
                    byte & mask
                } else {
                    byte
                }
            })
            .collect()
    }
}

impl CompressBackend for QuantizeBackend {
    fn compress(&self, data: &[u8]) -> Result<Vec<u8>> {
        let quantized = self.quantize(data);
        zstd::encode_all(quantized.as_slice(), self.zstd_level).map_err(Error::Io)
    }

    fn tag(&self) -> BackendTag {
        BackendTag::Quantized
    }
}

/// Decompressor for quantized data. Note: the original precision is lost.
pub struct QuantizeDecompressor;

impl DecompressBackend for QuantizeDecompressor {
    fn decompress(&self, compressed: &[u8], _original_size: usize) -> Result<Vec<u8>> {
        zstd::decode_all(compressed).map_err(Error::Io)
    }

    fn tag(&self) -> BackendTag {
        BackendTag::Quantized
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantization_clears_low_bits() {
        let backend = QuantizeBackend::new(2, 2, 3);
        // 0x03 = 0b00000011, stratum 2 -> quantize: clear 2 LSBs -> 0x00
        // 0xFF = 0b11111111, stratum 8 -> no change
        let data = vec![0x03, 0xFF, 0x01, 0x80];
        let quantized = backend.quantize(&data);
        assert_eq!(quantized[0], 0x00); // stratum 2 <= threshold, cleared
        assert_eq!(quantized[1], 0xFF); // stratum 8 > threshold, unchanged
        assert_eq!(quantized[2], 0x00); // stratum 1 <= threshold, cleared
        assert_eq!(quantized[3], 0x80); // stratum 1 <= threshold, 0x80 & 0xFC = 0x80
    }

    #[test]
    fn round_trip_decompresses() {
        let backend = QuantizeBackend::new(2, 2, 3);
        let data = vec![0x42u8; 1000];
        let compressed = backend.compress(&data).unwrap();
        let decompressor = QuantizeDecompressor;
        let decompressed = decompressor.decompress(&compressed, data.len()).unwrap();
        // Lossy: decompressed != original (possibly), but is valid.
        assert_eq!(decompressed.len(), data.len());
    }
}
