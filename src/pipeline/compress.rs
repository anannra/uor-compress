use std::fs;
use std::io::BufWriter;
use std::path::Path;

use crate::algebra::address::ChunkId;
use crate::analysis::classifier::ChunkClass;
use crate::analysis::delta::DeltaDetector;
use crate::analysis::stratum_profile::ChunkProfile;
use crate::archive::format::{flags, FileMapEntry, TocEntry};
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
    let original_size = input_data.len() as u64;

    if input_data.is_empty() {
        // Handle empty file: write minimal archive.
        let checksum = integrity::file_checksum(&input_data);
        let file = fs::File::create(output)?;
        let writer = BufWriter::new(file);
        let archive_flags = if config.emit_manifest { flags::HAS_MANIFEST } else { 0 };
        let archive = ArchiveWriter::new(writer, 0, checksum, archive_flags)?;
        archive.finalize(None)?;
        let compressed_size = fs::metadata(output)?.len();
        return Ok(CompressStats {
            original_size: 0,
            compressed_size,
            chunk_count: 0,
            unique_chunks: 0,
            duplicate_chunks: 0,
            delta_chunks: 0,
        });
    }

    let checksum = integrity::file_checksum(&input_data);

    // Step 1: Content-defined chunking.
    let chunker = Chunker::new(config.chunk_params.clone());
    let chunks = chunker.chunk(&input_data);

    // Step 2-7: Process chunks.
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

    for chunk in &chunks {
        // File map entry for reconstruction.
        file_map_entries.push(FileMapEntry {
            file_offset: chunk.offset,
            chunk_id: chunk.id,
            length: chunk.data.len() as u32,
        });

        // Deduplication check.
        let (_, is_new) = store.insert(chunk.id, chunk.data.clone());
        if !is_new {
            duplicate_count += 1;
            continue; // Already stored.
        }

        // Triadic analysis.
        let profile = ChunkProfile::analyze(chunk.id, &chunk.data);

        // Delta detection (for Default/Best levels).
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

        // Register chunk for future delta detection.
        delta_detector.register(chunk.id, &chunk.data);

        // Route to backend.
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
                // Lossy quantization path.
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

        // Generate derivation certificate if requested.
        if config.emit_certificates {
            let class_str = format!("{:?}", classification);
            let fidelity = if is_lossy { 0.95 } else { 1.0 };
            derivations.push(CompressionDerivation::new(
                &chunk.id.to_urn(),
                &format!("urn:uor:compressed:sha256:{}", ChunkId::from_data(&compressed.compressed_data).to_hex()),
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

    // Step 8-9: Write archive.
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
            stratum_summary: [0u8; 9], // TODO: fill from profile
        };
        archive.write_chunk_data(toc, &cc.compressed_data)?;
    }

    for entry in &file_map_entries {
        archive.add_file_map_entry(entry.clone());
    }

    // Generate and write manifest.
    let manifest_bytes = if config.emit_manifest {
        let archive_hash = &ChunkId::from_data(&input_data).to_hex()[..16];
        let manifest_value = manifest::generate_manifest(
            &crate::archive::format::ArchiveHeader {
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

    // Fall back to identity if compression expanded the data.
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
