use crate::backend::traits::{BackendTag, CompressBackend, DecompressBackend};
use crate::error::{Error, Result};

/// LZ4 compression backend — optimized for speed.
pub struct Lz4Backend;

impl CompressBackend for Lz4Backend {
    fn compress(&self, data: &[u8]) -> Result<Vec<u8>> {
        Ok(lz4_flex::compress_prepend_size(data))
    }

    fn tag(&self) -> BackendTag {
        BackendTag::Lz4
    }
}

impl DecompressBackend for Lz4Backend {
    fn decompress(&self, compressed: &[u8], _original_size: usize) -> Result<Vec<u8>> {
        lz4_flex::decompress_size_prepended(compressed)
            .map_err(|e| Error::DecompressError(e.to_string()))
    }

    fn tag(&self) -> BackendTag {
        BackendTag::Lz4
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let backend = Lz4Backend;
        let data = b"the quick brown fox jumps over the lazy dog repeatedly";
        let compressed = backend.compress(data).unwrap();
        let decompressed = backend.decompress(&compressed, data.len()).unwrap();
        assert_eq!(&decompressed, data);
    }
}
