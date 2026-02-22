use crate::chunk::ChunkParams;

/// Compression mode.
#[derive(Debug, Clone, Copy)]
pub enum CompressionMode {
    Lossless,
    Lossy {
        /// Minimum fidelity to maintain (0.0 = max lossy, 1.0 = lossless).
        min_fidelity: f64,
        /// Bytes with stratum <= this are candidates for quantization.
        stratum_threshold: u8,
    },
}

/// Compression level affecting speed/ratio tradeoff.
#[derive(Debug, Clone, Copy)]
pub enum CompressionLevel {
    /// Prefer lz4, skip delta search.
    Fast,
    /// Balanced: zstd default, basic delta search.
    Default,
    /// zstd max, exhaustive delta search.
    Best,
}

/// Full pipeline configuration.
#[derive(Debug, Clone)]
pub struct CompressConfig {
    pub mode: CompressionMode,
    pub level: CompressionLevel,
    pub chunk_params: ChunkParams,
    pub emit_manifest: bool,
    pub emit_certificates: bool,
    pub verify_on_compress: bool,
}

impl Default for CompressConfig {
    fn default() -> Self {
        Self {
            mode: CompressionMode::Lossless,
            level: CompressionLevel::Default,
            chunk_params: ChunkParams::default(),
            emit_manifest: false,
            emit_certificates: false,
            verify_on_compress: false,
        }
    }
}
