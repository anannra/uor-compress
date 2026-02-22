use crate::backend::traits::{BackendTag, CompressBackend, DecompressBackend};
use crate::error::{Error, Result};

/// Delta compression backend: XOR against a base chunk, then zstd the residual.
/// The residual is typically very sparse (mostly zeros), so zstd compresses it well.
pub struct DeltaCompressor {
    base_data: Vec<u8>,
    zstd_level: i32,
}

impl DeltaCompressor {
    pub fn new(base_data: Vec<u8>, zstd_level: i32) -> Self {
        Self {
            base_data,
            zstd_level,
        }
    }

    /// XOR two equal-length byte slices.
    fn xor_bytes(a: &[u8], b: &[u8]) -> Vec<u8> {
        a.iter().zip(b.iter()).map(|(&x, &y)| x ^ y).collect()
    }
}

impl CompressBackend for DeltaCompressor {
    fn compress(&self, data: &[u8]) -> Result<Vec<u8>> {
        if data.len() != self.base_data.len() {
            return Err(Error::DecompressError(
                "delta: chunk size mismatch".to_string(),
            ));
        }
        let residual = Self::xor_bytes(data, &self.base_data);
        zstd::encode_all(residual.as_slice(), self.zstd_level).map_err(Error::Io)
    }

    fn tag(&self) -> BackendTag {
        BackendTag::Delta
    }
}

/// Delta decompressor: decompress the residual then XOR with base to recover original.
pub struct DeltaDecompressor {
    base_data: Vec<u8>,
}

impl DeltaDecompressor {
    pub fn new(base_data: Vec<u8>) -> Self {
        Self { base_data }
    }
}

impl DecompressBackend for DeltaDecompressor {
    fn decompress(&self, compressed: &[u8], _original_size: usize) -> Result<Vec<u8>> {
        let residual = zstd::decode_all(compressed).map_err(Error::Io)?;
        if residual.len() != self.base_data.len() {
            return Err(Error::DecompressError(
                "delta: residual size mismatch".to_string(),
            ));
        }
        Ok(DeltaCompressor::xor_bytes(&residual, &self.base_data))
    }

    fn tag(&self) -> BackendTag {
        BackendTag::Delta
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let base = vec![0x55u8; 1000];
        let target = {
            let mut t = base.clone();
            t[0] = 0x56; // One byte different
            t[999] = 0x54;
            t
        };

        let compressor = DeltaCompressor::new(base.clone(), 3);
        let compressed = compressor.compress(&target).unwrap();

        let decompressor = DeltaDecompressor::new(base);
        let recovered = decompressor.decompress(&compressed, target.len()).unwrap();

        assert_eq!(recovered, target);
    }

    #[test]
    fn identical_chunks_compress_well() {
        let base = vec![42u8; 10000];
        let compressor = DeltaCompressor::new(base.clone(), 3);
        let compressed = compressor.compress(&base).unwrap();
        // XOR of identical data is all zeros — should compress extremely well.
        assert!(compressed.len() < 100);
    }
}
