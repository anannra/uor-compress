use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("invalid archive format: {0}")]
    InvalidArchive(String),

    #[error("chunk integrity failure: expected {expected}, got {actual}")]
    IntegrityFailure { expected: String, actual: String },

    #[error("decompression error: {0}")]
    DecompressError(String),

    #[error("coherence verification failed: {0}")]
    CoherenceError(String),

    #[error("unsupported backend tag: {0}")]
    UnsupportedBackend(u8),
}

pub type Result<T> = std::result::Result<T, Error>;
