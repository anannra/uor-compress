use serde_json::{json, Value};

use crate::archive::format::ArchiveHeader;
use crate::verify::certificate::CompressionDerivation;

/// Generate a JSON-LD manifest for the archive.
/// References UOR Foundation ontology IRIs.
pub fn generate_manifest(
    header: &ArchiveHeader,
    derivations: &[CompressionDerivation],
    archive_hash: &str,
) -> Value {
    let mut graph: Vec<Value> = Vec::new();

    // Archive-level coherence proof.
    graph.push(json!({
        "@id": format!("urn:uor:archive:sha256:{}", archive_hash),
        "@type": "proof:CoherenceProof",
        "proof:verified": header.flags & crate::archive::format::flags::VERIFIED != 0,
        "proof:originalSize": header.original_size,
        "proof:chunkCount": header.chunk_count,
        "proof:fileMapCount": header.file_map_count,
        "proof:lossy": header.is_lossy(),
    }));

    // Per-chunk derivation certificates.
    for d in derivations {
        graph.push(d.to_jsonld());
    }

    json!({
        "@context": {
            "uor": "https://uor.foundation/",
            "proof": "https://uor.foundation/proof/",
            "derivation": "https://uor.foundation/derivation/",
            "observable": "https://uor.foundation/observable/",
            "xsd": "http://www.w3.org/2001/XMLSchema#"
        },
        "@graph": graph
    })
}
