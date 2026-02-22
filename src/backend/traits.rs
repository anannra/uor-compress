use crate::algebra::address::ChunkId;
use crate::error::Result;

/// Identifies which backend compressed a chunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum BackendTag {
    Identity = 0,
    Zstd = 1,
    Lz4 = 2,
    Delta = 3,
    Quantized = 4,
    Reference = 5,
}

impl BackendTag {
    pub fn from_u8(v: u8) -> Result<Self> {
        match v {
            0 => Ok(Self::Identity),
            1 => Ok(Self::Zstd),
            2 => Ok(Self::Lz4),
            3 => Ok(Self::Delta),
            4 => Ok(Self::Quantized),
            5 => Ok(Self::Reference),
            _ => Err(crate::error::Error::UnsupportedBackend(v)),
        }
    }
}

/// Result of compressing a single chunk.
#[derive(Debug)]
pub struct CompressedChunk {
    pub original_id: ChunkId,
    pub backend: BackendTag,
    pub compressed_data: Vec<u8>,
    pub original_size: u32,
    pub compressed_size: u32,
    pub base_chunk_id: Option<ChunkId>,
}

pub trait CompressBackend: Send + Sync {
    fn compress(&self, data: &[u8]) -> Result<Vec<u8>>;
    fn tag(&self) -> BackendTag;
}

pub trait DecompressBackend: Send + Sync {
    fn decompress(&self, compressed: &[u8], original_size: usize) -> Result<Vec<u8>>;
    fn tag(&self) -> BackendTag;
}
