use crate::algebra::address::ChunkId;
use crate::backend::traits::BackendTag;

/// Archive magic bytes: "UOR\xC0MP\x01\x00\x00"
pub const MAGIC: [u8; 8] = [b'U', b'O', b'R', 0xC0, b'M', b'P', 0x01, 0x00];

/// Archive format version.
pub const VERSION: u16 = 1;

/// Flag bits for the archive header.
pub mod flags {
    pub const LOSSY: u32 = 1 << 0;
    pub const HAS_MANIFEST: u32 = 1 << 1;
    pub const HAS_CERTIFICATES: u32 = 1 << 2;
    pub const VERIFIED: u32 = 1 << 3;
    /// Single-stream mode: entire file compressed as one zstd stream, no chunking.
    /// The archive is just: header (88 bytes) + compressed data.
    /// TOC/file map offsets are unused (set to 0). chunk_count = 0.
    pub const SINGLE_STREAM: u32 = 1 << 4;
    /// A shared zstd dictionary is stored after the header, before chunk data.
    /// Format: [dict_size: u32][dict_data: [u8; dict_size]]
    pub const HAS_DICTIONARY: u32 = 1 << 5;
}

/// Archive file header (88 bytes on disk).
#[derive(Debug, Clone)]
pub struct ArchiveHeader {
    pub version: u16,
    pub flags: u32,
    pub original_size: u64,
    pub chunk_count: u32,
    pub file_map_count: u32,
    pub toc_offset: u64,
    pub file_map_offset: u64,
    pub manifest_offset: u64,
    pub checksum: [u8; 32],
}

impl ArchiveHeader {
    pub fn is_lossy(&self) -> bool {
        self.flags & flags::LOSSY != 0
    }

    pub fn has_manifest(&self) -> bool {
        self.flags & flags::HAS_MANIFEST != 0
    }

    pub fn is_single_stream(&self) -> bool {
        self.flags & flags::SINGLE_STREAM != 0
    }
}

/// Table-of-contents entry for one unique chunk.
#[derive(Debug, Clone)]
pub struct TocEntry {
    pub chunk_id: ChunkId,
    pub backend: BackendTag,
    pub data_offset: u64,
    pub compressed_size: u32,
    pub original_size: u32,
    pub base_chunk_id: Option<ChunkId>,
    pub stratum_summary: [u8; 9],
}

/// File reconstruction entry: maps a file region to a chunk.
#[derive(Debug, Clone)]
pub struct FileMapEntry {
    pub file_offset: u64,
    pub chunk_id: ChunkId,
    pub length: u32,
}
