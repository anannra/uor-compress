use crate::backend::traits::{BackendTag, CompressBackend, DecompressBackend};
use crate::error::{Error, Result};

/// Zstd compression backend.
pub struct ZstdBackend {
    level: i32,
}

impl ZstdBackend {
    pub fn new(level: i32) -> Self {
        Self { level }
    }

    /// Fast preset.
    pub fn fast() -> Self {
        Self::new(1)
    }

    /// Default balanced preset.
    pub fn default_level() -> Self {
        Self::new(3)
    }

    /// High compression preset.
    pub fn high() -> Self {
        Self::new(19)
    }
}

impl ZstdBackend {
    /// Compress using a shared dictionary for cross-chunk context.
    pub fn compress_with_dict(&self, data: &[u8], dict: &[u8]) -> Result<Vec<u8>> {
        let mut compressor =
            zstd::bulk::Compressor::with_dictionary(self.level, dict).map_err(Error::Io)?;
        compressor.compress(data).map_err(Error::Io)
    }

    /// Decompress using a shared dictionary.
    pub fn decompress_with_dict(
        &self,
        compressed: &[u8],
        original_size: usize,
        dict: &[u8],
    ) -> Result<Vec<u8>> {
        let mut decompressor =
            zstd::bulk::Decompressor::with_dictionary(dict).map_err(Error::Io)?;
        decompressor
            .decompress(compressed, original_size)
            .map_err(Error::Io)
    }
}

impl CompressBackend for ZstdBackend {
    fn compress(&self, data: &[u8]) -> Result<Vec<u8>> {
        zstd::encode_all(data, self.level).map_err(|e| Error::Io(e))
    }

    fn tag(&self) -> BackendTag {
        BackendTag::Zstd
    }
}

impl DecompressBackend for ZstdBackend {
    fn decompress(&self, compressed: &[u8], _original_size: usize) -> Result<Vec<u8>> {
        zstd::decode_all(compressed).map_err(|e| Error::Io(e))
    }

    fn tag(&self) -> BackendTag {
        BackendTag::Zstd
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let backend = ZstdBackend::default_level();
        let data = b"hello world hello world hello world";
        let compressed = backend.compress(data).unwrap();
        let decompressed = backend.decompress(&compressed, data.len()).unwrap();
        assert_eq!(&decompressed, data);
    }

    #[test]
    fn compresses_repetitive_data() {
        let backend = ZstdBackend::default_level();
        let data = vec![42u8; 10000];
        let compressed = backend.compress(&data).unwrap();
        assert!(compressed.len() < data.len() / 10);
    }
}
