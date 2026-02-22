use std::collections::HashMap;
use std::fs;
use std::io::BufReader;
use std::path::Path;

use crate::algebra::address::ChunkId;
use crate::archive::reader::ArchiveReader;
use crate::backend::delta_backend::DeltaDecompressor;
use crate::backend::identity::IdentityBackend;
use crate::backend::lz4_backend::Lz4Backend;
use crate::backend::quantize::QuantizeDecompressor;
use crate::backend::traits::{BackendTag, DecompressBackend};
use crate::backend::zstd_backend::ZstdBackend;
use crate::error::{Error, Result};
use crate::verify::integrity;

/// Decompression statistics.
#[derive(Debug)]
pub struct DecompressStats {
    pub original_size: u64,
    pub compressed_size: u64,
    pub chunks_decompressed: u32,
    pub is_lossy: bool,
}

/// Decompress a .uorc archive back to the original file.
pub fn decompress_file(
    input: &Path,
    output: &Path,
    verify: bool,
) -> Result<DecompressStats> {
    let compressed_size = fs::metadata(input)?.len();
    let file = fs::File::open(input)?;
    let reader = BufReader::new(file);
    let mut archive = ArchiveReader::open(reader)?;

    let original_size = archive.header.original_size;
    let is_lossy = archive.header.is_lossy();

    // Decompress all unique chunks.
    let mut decompressed_chunks: HashMap<ChunkId, Vec<u8>> = HashMap::new();

    // First pass: decompress non-delta chunks.
    for toc_entry in &archive.toc.clone() {
        if toc_entry.backend == BackendTag::Delta {
            continue; // Handle in second pass.
        }

        let compressed_data = archive.read_chunk_data(toc_entry)?;
        let decompressed = match toc_entry.backend {
            BackendTag::Identity => {
                IdentityBackend.decompress(&compressed_data, toc_entry.original_size as usize)?
            }
            BackendTag::Zstd => {
                ZstdBackend::default_level()
                    .decompress(&compressed_data, toc_entry.original_size as usize)?
            }
            BackendTag::Lz4 => {
                Lz4Backend.decompress(&compressed_data, toc_entry.original_size as usize)?
            }
            BackendTag::Quantized => {
                QuantizeDecompressor.decompress(&compressed_data, toc_entry.original_size as usize)?
            }
            BackendTag::Reference => {
                // Should be resolved via file_map pointing to an existing chunk.
                continue;
            }
            BackendTag::Delta => unreachable!(),
        };

        if verify && !is_lossy {
            integrity::verify_chunk(&decompressed, &toc_entry.chunk_id)?;
        }

        decompressed_chunks.insert(toc_entry.chunk_id, decompressed);
    }

    // Second pass: decompress delta chunks (need their bases).
    for toc_entry in &archive.toc.clone() {
        if toc_entry.backend != BackendTag::Delta {
            continue;
        }

        let base_id = toc_entry
            .base_chunk_id
            .ok_or_else(|| Error::InvalidArchive("delta chunk without base ID".to_string()))?;

        let base_data = decompressed_chunks
            .get(&base_id)
            .ok_or_else(|| {
                Error::InvalidArchive(format!("delta base chunk not found: {base_id}"))
            })?
            .clone();

        let compressed_data = archive.read_chunk_data(toc_entry)?;
        let decompressor = DeltaDecompressor::new(base_data);
        let decompressed =
            decompressor.decompress(&compressed_data, toc_entry.original_size as usize)?;

        if verify && !is_lossy {
            integrity::verify_chunk(&decompressed, &toc_entry.chunk_id)?;
        }

        decompressed_chunks.insert(toc_entry.chunk_id, decompressed);
    }

    // Reassemble the file from the file map.
    let mut output_data = vec![0u8; original_size as usize];
    let chunks_decompressed = archive.file_map.len() as u32;

    for entry in &archive.file_map {
        let chunk_data = decompressed_chunks
            .get(&entry.chunk_id)
            .ok_or_else(|| {
                Error::InvalidArchive(format!("chunk not found in archive: {}", entry.chunk_id))
            })?;

        let dst_start = entry.file_offset as usize;
        let dst_end = dst_start + entry.length as usize;
        let src_end = entry.length as usize;

        if dst_end > output_data.len() || src_end > chunk_data.len() {
            return Err(Error::InvalidArchive("file map entry out of bounds".to_string()));
        }

        output_data[dst_start..dst_end].copy_from_slice(&chunk_data[..src_end]);
    }

    // Verify whole-file checksum (lossless only).
    if verify && !is_lossy {
        integrity::verify_file_checksum(&output_data, &archive.header.checksum)?;
    }

    fs::write(output, &output_data)?;

    Ok(DecompressStats {
        original_size,
        compressed_size,
        chunks_decompressed,
        is_lossy,
    })
}
