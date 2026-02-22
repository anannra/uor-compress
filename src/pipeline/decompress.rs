use std::collections::HashMap;
use std::fs;
use std::io::{BufReader, Read, Seek};
use std::path::Path;

use byteorder::{LittleEndian, ReadBytesExt};

use crate::algebra::address::ChunkId;
use crate::archive::format::{flags, MAGIC, VERSION};
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

    // Peek at the header to check for single-stream mode.
    let file = fs::File::open(input)?;
    let mut reader = BufReader::new(file);

    let mut magic = [0u8; 8];
    reader.read_exact(&mut magic)?;
    if magic != MAGIC {
        return Err(Error::InvalidArchive("bad magic bytes".to_string()));
    }
    let version = reader.read_u16::<LittleEndian>()?;
    if version != VERSION {
        return Err(Error::InvalidArchive(format!("unsupported version: {version}")));
    }
    let header_flags = reader.read_u32::<LittleEndian>()?;

    if header_flags & flags::SINGLE_STREAM != 0 {
        // Single-stream fast path.
        return decompress_single_stream(reader, output, header_flags, compressed_size, verify);
    }

    // Multi-chunk path: re-open with ArchiveReader.
    drop(reader);
    let file = fs::File::open(input)?;
    let reader = BufReader::new(file);
    decompress_chunked(reader, output, compressed_size, verify)
}

/// Decompress a single-stream archive: header + one zstd/identity frame.
fn decompress_single_stream<R: Read + Seek>(
    mut reader: R,
    output: &Path,
    header_flags: u32,
    compressed_size: u64,
    verify: bool,
) -> Result<DecompressStats> {
    // Continue reading header fields (we already read magic + version + flags).
    let original_size = reader.read_u64::<LittleEndian>()?;
    let _chunk_count = reader.read_u32::<LittleEndian>()?;
    let backend_byte = reader.read_u32::<LittleEndian>()?; // repurposed as backend tag
    let _toc_offset = reader.read_u64::<LittleEndian>()?;
    let _file_map_offset = reader.read_u64::<LittleEndian>()?;
    let _manifest_offset = reader.read_u64::<LittleEndian>()?;
    let mut checksum = [0u8; 32];
    reader.read_exact(&mut checksum)?;
    let mut _reserved = [0u8; 2];
    reader.read_exact(&mut _reserved)?;

    let backend = BackendTag::from_u8(backend_byte as u8)?;
    let is_lossy = header_flags & flags::LOSSY != 0;

    // Read the rest of the file as compressed data.
    let mut compressed_data = Vec::new();
    reader.read_to_end(&mut compressed_data)?;

    let output_data = match backend {
        BackendTag::Identity => compressed_data,
        BackendTag::Zstd => {
            zstd::decode_all(compressed_data.as_slice()).map_err(Error::Io)?
        }
        other => {
            return Err(Error::InvalidArchive(format!(
                "unexpected backend in single-stream: {other:?}"
            )));
        }
    };

    if verify && !is_lossy {
        integrity::verify_file_checksum(&output_data, &checksum)?;
    }

    fs::write(output, &output_data)?;

    Ok(DecompressStats {
        original_size,
        compressed_size,
        chunks_decompressed: 1,
        is_lossy,
    })
}

/// Decompress a multi-chunk archive.
fn decompress_chunked<R: Read + Seek>(
    reader: R,
    output: &Path,
    compressed_size: u64,
    verify: bool,
) -> Result<DecompressStats> {
    let mut archive = ArchiveReader::open(reader)?;

    let original_size = archive.header.original_size;
    let is_lossy = archive.header.is_lossy();

    let mut decompressed_chunks: HashMap<ChunkId, Vec<u8>> = HashMap::new();

    // First pass: decompress non-delta chunks.
    for toc_entry in &archive.toc.clone() {
        if toc_entry.backend == BackendTag::Delta {
            continue;
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
                QuantizeDecompressor
                    .decompress(&compressed_data, toc_entry.original_size as usize)?
            }
            BackendTag::Reference => continue,
            BackendTag::Delta => unreachable!(),
        };

        if verify && !is_lossy {
            integrity::verify_chunk(&decompressed, &toc_entry.chunk_id)?;
        }

        decompressed_chunks.insert(toc_entry.chunk_id, decompressed);
    }

    // Second pass: decompress delta chunks.
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
            return Err(Error::InvalidArchive(
                "file map entry out of bounds".to_string(),
            ));
        }

        output_data[dst_start..dst_end].copy_from_slice(&chunk_data[..src_end]);
    }

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
