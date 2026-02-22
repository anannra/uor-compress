use sha2::{Digest, Sha256};

use crate::algebra::address::ChunkId;
use crate::error::{Error, Result};

/// Verify a decompressed chunk matches its expected ChunkId.
pub fn verify_chunk(data: &[u8], expected_id: &ChunkId) -> Result<()> {
    let actual_id = ChunkId::from_data(data);
    if &actual_id != expected_id {
        return Err(Error::IntegrityFailure {
            expected: expected_id.to_hex(),
            actual: actual_id.to_hex(),
        });
    }
    Ok(())
}

/// Compute SHA-256 checksum of a file's contents.
pub fn file_checksum(data: &[u8]) -> [u8; 32] {
    let hash = Sha256::digest(data);
    let mut out = [0u8; 32];
    out.copy_from_slice(&hash);
    out
}

/// Verify a file's checksum matches expected.
pub fn verify_file_checksum(data: &[u8], expected: &[u8; 32]) -> Result<()> {
    let actual = file_checksum(data);
    if &actual != expected {
        let exp_hex: String = expected.iter().map(|b| format!("{b:02x}")).collect();
        let act_hex: String = actual.iter().map(|b| format!("{b:02x}")).collect();
        return Err(Error::IntegrityFailure {
            expected: exp_hex,
            actual: act_hex,
        });
    }
    Ok(())
}
