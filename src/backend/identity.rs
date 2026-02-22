use crate::backend::traits::{BackendTag, CompressBackend, DecompressBackend};
use crate::error::Result;

/// Pass-through backend: no compression.
/// Used for already-compressed or incompressible data.
pub struct IdentityBackend;

impl CompressBackend for IdentityBackend {
    fn compress(&self, data: &[u8]) -> Result<Vec<u8>> {
        Ok(data.to_vec())
    }

    fn tag(&self) -> BackendTag {
        BackendTag::Identity
    }
}

impl DecompressBackend for IdentityBackend {
    fn decompress(&self, compressed: &[u8], _original_size: usize) -> Result<Vec<u8>> {
        Ok(compressed.to_vec())
    }

    fn tag(&self) -> BackendTag {
        BackendTag::Identity
    }
}
