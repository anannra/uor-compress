use sha2::{Digest, Sha256};
use std::fmt;

/// Content-addressed chunk identifier (SHA-256).
/// Maps PRISM's derivation_id pattern.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChunkId([u8; 32]);

impl ChunkId {
    /// Compute a ChunkId from raw data via SHA-256.
    pub fn from_data(data: &[u8]) -> Self {
        let hash = Sha256::digest(data);
        let mut id = [0u8; 32];
        id.copy_from_slice(&hash);
        Self(id)
    }

    /// Create from a raw 32-byte hash.
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Return the raw 32-byte hash.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Hex-encoded string.
    pub fn to_hex(&self) -> String {
        self.0.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// URN format: "urn:uor:chunk:sha256:{hex}"
    pub fn to_urn(&self) -> String {
        format!("urn:uor:chunk:sha256:{}", &self.to_hex()[..16])
    }

    /// Braille IRI per PRISM's _iri() — maps each byte to Unicode Braille U+2800..U+28FF.
    pub fn to_braille_iri(&self) -> String {
        let base = "https://uor.foundation/u/";
        let glyphs: String = self.0.iter().map(|&b| char::from_u32(0x2800 + b as u32).unwrap()).collect();
        format!("{base}{glyphs}")
    }
}

impl fmt::Debug for ChunkId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ChunkId({})", &self.to_hex()[..16])
    }
}

impl fmt::Display for ChunkId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", &self.to_hex()[..16])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_data_same_id() {
        let data = b"hello world";
        let a = ChunkId::from_data(data);
        let b = ChunkId::from_data(data);
        assert_eq!(a, b);
    }

    #[test]
    fn different_data_different_id() {
        let a = ChunkId::from_data(b"hello");
        let b = ChunkId::from_data(b"world");
        assert_ne!(a, b);
    }

    #[test]
    fn urn_format() {
        let id = ChunkId::from_data(b"test");
        let urn = id.to_urn();
        assert!(urn.starts_with("urn:uor:chunk:sha256:"));
        assert_eq!(urn.len(), "urn:uor:chunk:sha256:".len() + 16);
    }

    #[test]
    fn braille_iri_format() {
        let id = ChunkId::from_data(b"test");
        let iri = id.to_braille_iri();
        assert!(iri.starts_with("https://uor.foundation/u/"));
    }

    #[test]
    fn hex_is_64_chars() {
        let id = ChunkId::from_data(b"test");
        assert_eq!(id.to_hex().len(), 64);
    }
}
