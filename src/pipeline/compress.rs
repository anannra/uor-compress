use std::fs;
use std::io::{BufWriter, Write};
use std::path::Path;

use byteorder::{LittleEndian, WriteBytesExt};

use crate::algebra::address::ChunkId;
use crate::analysis::classifier::ChunkClass;
use crate::analysis::delta::DeltaDetector;
use crate::analysis::stratum_profile::ChunkProfile;
use crate::archive::format::{flags, ArchiveHeader, FileMapEntry, TocEntry, MAGIC, VERSION};
use crate::archive::manifest;
use crate::archive::writer::ArchiveWriter;
use crate::backend::delta_backend::DeltaCompressor;
use crate::backend::identity::IdentityBackend;
use crate::backend::lz4_backend::Lz4Backend;
use crate::backend::quantize::QuantizeBackend;
use crate::backend::traits::{BackendTag, CompressBackend, CompressedChunk};
use crate::backend::zstd_backend::ZstdBackend;
use crate::chunk::chunk_store::ChunkStore;
use crate::chunk::cdc::Chunker;
use crate::error::Result;
use crate::pipeline::config::{CompressConfig, CompressionLevel, CompressionMode};
use crate::verify::certificate::CompressionDerivation;
use crate::verify::integrity;

/// Compression statistics returned after compression.
#[derive(Debug)]
pub struct CompressStats {
    pub original_size: u64,
    pub compressed_size: u64,
    pub chunk_count: u32,
    pub unique_chunks: u32,
    pub duplicate_chunks: u32,
    pub delta_chunks: u32,
}

impl CompressStats {
    pub fn ratio(&self) -> f64 {
        if self.compressed_size == 0 {
            return 0.0;
        }
        self.original_size as f64 / self.compressed_size as f64
    }
}

/// Compress a file using the UOR pipeline.
pub fn compress_file(input: &Path, output: &Path, config: &CompressConfig) -> Result<CompressStats> {
    let input_data = fs::read(input)?;
    if input_data.is_empty() {
        return compress_empty(output, config);
    }

    let checksum = integrity::file_checksum(&input_data);

    // Content-defined chunking.
    let chunker = Chunker::new(config.chunk_params.clone());
    let chunks = chunker.chunk(&input_data);

    // Check for deduplication opportunity: do any chunks repeat?
    let mut seen = std::collections::HashSet::new();
    let has_duplicates = chunks.iter().any(|c| !seen.insert(c.id));

    // Single-stream fast path: when there are no duplicate chunks, compress the
    // entire file as one zstd stream. This eliminates per-chunk overhead and gives
    // the compressor the full file as dictionary context — matching gzip behavior.
    if !has_duplicates && matches!(config.mode, CompressionMode::Lossless) {
        return compress_single_stream(&input_data, output, checksum, config);
    }

    // Multi-chunk path with dedup/delta support.
    compress_chunked(&input_data, &chunks, output, checksum, config)
}

/// Write a minimal archive for an empty file.
fn compress_empty(output: &Path, config: &CompressConfig) -> Result<CompressStats> {
    let checksum = integrity::file_checksum(&[]);
    let file = fs::File::create(output)?;
    let writer = BufWriter::new(file);
    let archive_flags = if config.emit_manifest { flags::HAS_MANIFEST } else { 0 };
    let archive = ArchiveWriter::new(writer, 0, checksum, archive_flags)?;
    archive.finalize(None)?;
    let compressed_size = fs::metadata(output)?.len();
    Ok(CompressStats {
        original_size: 0,
        compressed_size,
        chunk_count: 0,
        unique_chunks: 0,
        duplicate_chunks: 0,
        delta_chunks: 0,
    })
}

/// Single-stream compression: header (88 bytes) + one zstd frame.
/// No TOC, no file map, no chunking overhead. Maximizes compression ratio.
fn compress_single_stream(
    data: &[u8],
    output: &Path,
    checksum: [u8; 32],
    config: &CompressConfig,
) -> Result<CompressStats> {
    let original_size = data.len() as u64;

    let zstd_level = match config.level {
        CompressionLevel::Fast => 1,
        CompressionLevel::Default => 3,
        CompressionLevel::Best => 19,
    };

    let compressed_data = zstd::encode_all(data, zstd_level)
        .map_err(crate::error::Error::Io)?;

    // If compression expanded the data, store raw.
    let (final_data, backend_tag) = if compressed_data.len() >= data.len() {
        (data.to_vec(), BackendTag::Identity)
    } else {
        (compressed_data, BackendTag::Zstd)
    };

    let archive_flags: u32 = flags::SINGLE_STREAM;
    // Encode backend in file_map_count field (repurposed in single-stream mode).
    let backend_byte = backend_tag as u32;

    let file = fs::File::create(output)?;
    let mut w = BufWriter::new(file);

    // Write header.
    w.write_all(&MAGIC)?;
    w.write_u16::<LittleEndian>(VERSION)?;
    w.write_u32::<LittleEndian>(archive_flags)?;
    w.write_u64::<LittleEndian>(original_size)?;
    w.write_u32::<LittleEndian>(1)?; // chunk_count = 1 (logical)
    w.write_u32::<LittleEndian>(backend_byte)?; // file_map_count repurposed as backend tag
    w.write_u64::<LittleEndian>(0)?; // toc_offset unused
    w.write_u64::<LittleEndian>(0)?; // file_map_offset unused
    w.write_u64::<LittleEndian>(0)?; // manifest_offset unused
    w.write_all(&checksum)?;
    w.write_all(&[0u8; 2])?; // reserved
    // Write compressed data immediately after header.
    w.write_all(&final_data)?;
    w.flush()?;
    drop(w);

    let compressed_size = fs::metadata(output)?.len();

    Ok(CompressStats {
        original_size,
        compressed_size,
        chunk_count: 1,
        unique_chunks: 1,
        duplicate_chunks: 0,
        delta_chunks: 0,
    })
}

/// Multi-chunk compression with dedup and delta support.
fn compress_chunked(
    input_data: &[u8],
    chunks: &[crate::chunk::cdc::Chunk],
    output: &Path,
    checksum: [u8; 32],
    config: &CompressConfig,
) -> Result<CompressStats> {
    let original_size = input_data.len() as u64;

    let mut store = ChunkStore::new();
    let mut delta_detector = DeltaDetector::new();
    let mut compressed_chunks: Vec<(ChunkId, CompressedChunk)> = Vec::new();
    let mut file_map_entries: Vec<FileMapEntry> = Vec::new();
    let mut derivations: Vec<CompressionDerivation> = Vec::new();
    let mut duplicate_count = 0u32;
    let mut delta_count = 0u32;

    let is_lossy = matches!(config.mode, CompressionMode::Lossy { .. });
    let mut archive_flags: u32 = 0;
    if is_lossy {
        archive_flags |= flags::LOSSY;
    }
    if config.emit_manifest {
        archive_flags |= flags::HAS_MANIFEST;
    }
    if config.emit_certificates {
        archive_flags |= flags::HAS_CERTIFICATES;
    }

    for chunk in chunks {
        file_map_entries.push(FileMapEntry {
            file_offset: chunk.offset,
            chunk_id: chunk.id,
            length: chunk.data.len() as u32,
        });

        let (_, is_new) = store.insert(chunk.id, chunk.data.clone());
        if !is_new {
            duplicate_count += 1;
            continue;
        }

        let profile = ChunkProfile::analyze(chunk.id, &chunk.data);

        let classification = match config.level {
            CompressionLevel::Fast => profile.classification.clone(),
            _ => {
                if let Some(near_dup) = delta_detector.find_base(&chunk.data) {
                    near_dup
                } else {
                    profile.classification.clone()
                }
            }
        };

        delta_detector.register(chunk.id, &chunk.data);

        let compressed = match &classification {
            ChunkClass::NearDuplicate { base, fidelity: _ } => {
                let base_data = store.get(base).map(|s| s.data.clone());
                if let Some(base_data) = base_data {
                    delta_count += 1;
                    let zstd_level = match config.level {
                        CompressionLevel::Fast => 1,
                        CompressionLevel::Default => 3,
                        CompressionLevel::Best => 19,
                    };
                    let compressor = DeltaCompressor::new(base_data, zstd_level);
                    let data = compressor.compress(&chunk.data)?;
                    CompressedChunk {
                        original_id: chunk.id,
                        backend: BackendTag::Delta,
                        original_size: chunk.data.len() as u32,
                        compressed_size: data.len() as u32,
                        compressed_data: data,
                        base_chunk_id: Some(*base),
                    }
                } else {
                    compress_with_standard_backend(&chunk.data, chunk.id, config)?
                }
            }
            _ => {
                if let CompressionMode::Lossy {
                    stratum_threshold,
                    min_fidelity: _,
                } = config.mode
                {
                    if matches!(classification, ChunkClass::Structured | ChunkClass::Dense) {
                        let backend = QuantizeBackend::new(stratum_threshold, 2, 3);
                        let data = backend.compress(&chunk.data)?;
                        CompressedChunk {
                            original_id: chunk.id,
                            backend: BackendTag::Quantized,
                            original_size: chunk.data.len() as u32,
                            compressed_size: data.len() as u32,
                            compressed_data: data,
                            base_chunk_id: None,
                        }
                    } else {
                        compress_with_standard_backend(&chunk.data, chunk.id, config)?
                    }
                } else {
                    compress_with_standard_backend(&chunk.data, chunk.id, config)?
                }
            }
        };

        if config.emit_certificates {
            let class_str = format!("{:?}", classification);
            let fidelity = if is_lossy { 0.95 } else { 1.0 };
            derivations.push(CompressionDerivation::new(
                &chunk.id.to_urn(),
                &format!(
                    "urn:uor:compressed:sha256:{}",
                    ChunkId::from_data(&compressed.compressed_data).to_hex()
                ),
                &format!("{:?}", compressed.backend),
                compressed.original_size as u64,
                compressed.compressed_size as u64,
                profile.histogram.bins,
                &class_str,
                fidelity,
            ));
        }

        compressed_chunks.push((chunk.id, compressed));
    }

    // Write archive.
    let file = fs::File::create(output)?;
    let writer = BufWriter::new(file);
    let mut archive = ArchiveWriter::new(writer, original_size, checksum, archive_flags)?;

    for (_, cc) in &compressed_chunks {
        let toc = TocEntry {
            chunk_id: cc.original_id,
            backend: cc.backend,
            data_offset: archive.current_data_offset(),
            compressed_size: cc.compressed_size,
            original_size: cc.original_size,
            base_chunk_id: cc.base_chunk_id,
            stratum_summary: [0u8; 9],
        };
        archive.write_chunk_data(toc, &cc.compressed_data)?;
    }

    for entry in &file_map_entries {
        archive.add_file_map_entry(entry.clone());
    }

    let manifest_bytes = if config.emit_manifest {
        let archive_hash = &ChunkId::from_data(input_data).to_hex()[..16];
        let manifest_value = manifest::generate_manifest(
            &ArchiveHeader {
                version: 1,
                flags: archive_flags,
                original_size,
                chunk_count: compressed_chunks.len() as u32,
                file_map_count: file_map_entries.len() as u32,
                toc_offset: 0,
                file_map_offset: 0,
                manifest_offset: 0,
                checksum,
            },
            &derivations,
            archive_hash,
        );
        let json = serde_json::to_string_pretty(&manifest_value)
            .map_err(|e| crate::error::Error::InvalidArchive(e.to_string()))?;
        Some(json.into_bytes())
    } else {
        None
    };

    archive.finalize(manifest_bytes.as_deref())?;

    let compressed_size = fs::metadata(output)?.len();

    Ok(CompressStats {
        original_size,
        compressed_size,
        chunk_count: file_map_entries.len() as u32,
        unique_chunks: compressed_chunks.len() as u32,
        duplicate_chunks: duplicate_count,
        delta_chunks: delta_count,
    })
}

/// Compress a chunk using the standard (non-delta, non-lossy) backend routing.
fn compress_with_standard_backend(
    data: &[u8],
    id: ChunkId,
    config: &CompressConfig,
) -> Result<CompressedChunk> {
    let profile = ChunkProfile::analyze(id, data);

    let (compressed_data, backend_tag) = match (&profile.classification, config.level) {
        (ChunkClass::Random, _) => {
            let lz4 = Lz4Backend;
            let compressed = lz4.compress(data)?;
            if compressed.len() >= data.len() {
                let identity = IdentityBackend;
                (identity.compress(data)?, BackendTag::Identity)
            } else {
                (compressed, BackendTag::Lz4)
            }
        }
        (ChunkClass::Sparse, _) => {
            let zstd = ZstdBackend::high();
            (zstd.compress(data)?, BackendTag::Zstd)
        }
        (_, CompressionLevel::Fast) => {
            let lz4 = Lz4Backend;
            (lz4.compress(data)?, BackendTag::Lz4)
        }
        (_, CompressionLevel::Best) => {
            let zstd = ZstdBackend::high();
            (zstd.compress(data)?, BackendTag::Zstd)
        }
        _ => {
            let zstd = ZstdBackend::default_level();
            (zstd.compress(data)?, BackendTag::Zstd)
        }
    };

    let (final_data, final_tag) = if compressed_data.len() >= data.len() {
        (data.to_vec(), BackendTag::Identity)
    } else {
        (compressed_data, backend_tag)
    };

    Ok(CompressedChunk {
        original_id: id,
        backend: final_tag,
        original_size: data.len() as u32,
        compressed_size: final_data.len() as u32,
        compressed_data: final_data,
        base_chunk_id: None,
    })
}
