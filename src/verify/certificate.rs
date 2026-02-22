use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

/// A derivation certificate for a compression step.
/// Modeled after PRISM's Derivation dataclass and UOR-Framework's derivation namespace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionDerivation {
    pub derivation_id: String,
    pub original_chunk_id: String,
    pub compressed_chunk_id: String,
    pub backend: String,
    pub original_size: u64,
    pub compressed_size: u64,
    pub stratum_histogram: [u32; 9],
    pub classification: String,
    /// 1.0 for lossless, <1.0 for lossy.
    pub fidelity: f64,
}

impl CompressionDerivation {
    /// Create a new derivation certificate.
    /// The derivation_id is computed as SHA-256 of the certificate content.
    pub fn new(
        original_chunk_urn: &str,
        compressed_chunk_urn: &str,
        backend: &str,
        original_size: u64,
        compressed_size: u64,
        stratum_histogram: [u32; 9],
        classification: &str,
        fidelity: f64,
    ) -> Self {
        let content = format!(
            "{original_chunk_urn}|{backend}|{compressed_chunk_urn}|{original_size}|{compressed_size}"
        );
        let hash = Sha256::digest(content.as_bytes());
        let hex: String = hash.iter().map(|b| format!("{b:02x}")).collect();
        let derivation_id = format!("urn:uor:derivation:sha256:{}", &hex[..16]);

        Self {
            derivation_id,
            original_chunk_id: original_chunk_urn.to_string(),
            compressed_chunk_id: compressed_chunk_urn.to_string(),
            backend: backend.to_string(),
            original_size,
            compressed_size,
            stratum_histogram,
            classification: classification.to_string(),
            fidelity,
        }
    }

    /// Convert to JSON-LD representation.
    pub fn to_jsonld(&self) -> Value {
        json!({
            "@id": self.derivation_id,
            "@type": "derivation:Derivation",
            "derivation:source": self.original_chunk_id,
            "derivation:result": self.compressed_chunk_id,
            "derivation:backend": self.backend,
            "derivation:originalSize": self.original_size,
            "derivation:compressedSize": self.compressed_size,
            "observable:stratumHistogram": self.stratum_histogram,
            "derivation:classification": self.classification,
            "observable:fidelity": self.fidelity,
        })
    }
}
