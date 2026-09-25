//! Shared SHA-256 verification for downloaded models and binaries.
//!
//! Every engine that downloads a model file (Parakeet, Whisper, summary
//! GGUFs, diarization assets) uses this module so a partial or tampered
//! download can never be mistaken for a valid model. Hashing streams the
//! file through a fixed-size buffer so files up to a few GB (the Parakeet
//! FP32 encoder is ~2.4 GB) never need to be read into memory at once.
//!
//! Callers are responsible for deleting the file on a verification failure —
//! `verify_file` only reports the mismatch.

use anyhow::{anyhow, Result};
use sha2::{Digest, Sha256};
use std::path::Path;
use tokio::io::AsyncReadExt;

/// Read buffer size for streaming hashing: large enough to amortize
/// syscalls, small enough to never meaningfully affect resident memory.
const BUFFER_SIZE: usize = 1024 * 1024;

/// SHA-256 of a file on disk, lowercase hex, computed by streaming the file
/// through a 1 MiB buffer rather than reading it whole into memory.
pub async fn sha256_file(path: &Path) -> Result<String> {
    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(|e| anyhow!("Failed to open {} for verification: {}", path.display(), e))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; BUFFER_SIZE];
    loop {
        let read = file
            .read(&mut buffer)
            .await
            .map_err(|e| anyhow!("Failed to read {} for verification: {}", path.display(), e))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// SHA-256 of an in-memory byte slice, lowercase hex.
pub fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

/// A downloadable artifact's expected identity: exact size in bytes and
/// lowercase-hex SHA-256, both pinned from the upstream host's published
/// metadata (never guessed).
///
/// Generic over a borrow lifetime rather than requiring `'static` so callers
/// can build one from either a `&'static str` catalog constant (Parakeet,
/// Whisper) or fields borrowed from an owned `String` (the summary GGUF
/// catalog, whose `ModelDef` also derives `serde::Deserialize` and so can't
/// itself hold `&'static str` fields).
#[derive(Debug, Clone, Copy)]
pub struct ExpectedArtifact<'a> {
    pub name: &'a str,
    pub size: u64,
    pub sha256: &'a str,
}

/// Verify a downloaded file against its expected size and SHA-256.
///
/// Checks size first (cheap) before hashing (expensive) so a truncated
/// download fails fast without a multi-GB hash pass. On mismatch, the
/// caller must delete the file — this function only reports the failure so
/// callers can decide how (and whether) to clean up partial state.
pub async fn verify_file(path: &Path, expected: &ExpectedArtifact<'_>) -> Result<()> {
    let metadata = tokio::fs::metadata(path)
        .await
        .map_err(|e| anyhow!("Failed to read metadata for {}: {}", expected.name, e))?;
    let actual_size = metadata.len();
    if actual_size != expected.size {
        return Err(anyhow!(
            "{} has the wrong size: expected {} bytes, got {} bytes",
            expected.name,
            expected.size,
            actual_size
        ));
    }

    let actual_hash = sha256_file(path).await?;
    if actual_hash != expected.sha256 {
        return Err(anyhow!(
            "{} failed SHA-256 verification: expected {}, got {}",
            expected.name,
            expected.sha256,
            actual_hash
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn sha256_file_hashes_known_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("abc.txt");
        tokio::fs::write(&path, b"abc").await.unwrap();

        let hash = sha256_file(&path).await.unwrap();

        assert_eq!(
            hash,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn sha256_bytes_hashes_known_content() {
        assert_eq!(
            sha256_bytes(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[tokio::test]
    async fn verify_file_rejects_wrong_size_without_hashing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("model.bin");
        tokio::fs::write(&path, b"short").await.unwrap();
        let expected = ExpectedArtifact {
            name: "model.bin",
            size: 999,
            sha256: "irrelevant-because-size-check-fails-first",
        };

        let error = verify_file(&path, &expected).await.unwrap_err();

        assert!(error.to_string().contains("wrong size"));
    }

    #[tokio::test]
    async fn verify_file_rejects_wrong_hash() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("abc.txt");
        tokio::fs::write(&path, b"abc").await.unwrap();
        let expected = ExpectedArtifact {
            name: "abc.txt",
            size: 3,
            sha256: "0000000000000000000000000000000000000000000000000000000000000000",
        };

        let error = verify_file(&path, &expected).await.unwrap_err();

        assert!(error.to_string().contains("SHA-256"));
    }

    #[tokio::test]
    async fn verify_file_accepts_matching_size_and_hash() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("abc.txt");
        tokio::fs::write(&path, b"abc").await.unwrap();
        let expected = ExpectedArtifact {
            name: "abc.txt",
            size: 3,
            sha256: "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        };

        verify_file(&path, &expected).await.unwrap();
    }
}
