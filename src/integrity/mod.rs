use std::fmt;
use std::io::Read;
use std::path::Path;
#[cfg(unix)]
use std::process::Command;

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};

const BUF_SIZE: usize = 64 * 1024;
const CHUNK_SIZE: usize = BUF_SIZE;
const CHUNK_HASH_BYTES: usize = 8;

pub fn hash_file(path: &Path) -> Result<String> {
    let file =
        std::fs::File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let mut reader = std::io::BufReader::with_capacity(BUF_SIZE, file);
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; BUF_SIZE];

    loop {
        let n = reader
            .read(&mut buf)
            .with_context(|| format!("failed to read {}", path.display()))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }

    Ok(hex::encode(hasher.finalize()))
}

#[cfg(unix)]
pub fn set_immutable(path: &Path) -> Result<()> {
    run_chattr("+i", path)
}

#[cfg(unix)]
pub fn clear_immutable(path: &Path) -> Result<()> {
    run_chattr("-i", path)
}

#[cfg(unix)]
fn run_chattr(flag: &str, path: &Path) -> Result<()> {
    let output = Command::new("chattr")
        .arg(flag)
        .arg(path)
        .output()
        .with_context(|| format!("failed to run chattr {flag} on {}", path.display()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "chattr {flag} failed on {}: {}",
            path.display(),
            stderr.trim()
        );
    }
    Ok(())
}

#[cfg(unix)]
pub fn is_immutable(path: &Path) -> Result<bool> {
    let output = Command::new("lsattr")
        .arg("-d")
        .arg(path)
        .output()
        .with_context(|| format!("failed to run lsattr on {}", path.display()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("lsattr failed on {}: {}", path.display(), stderr.trim());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let attrs = stdout
        .split_whitespace()
        .next()
        .with_context(|| format!("unexpected lsattr output for {}", path.display()))?;
    Ok(attrs.contains('i'))
}

#[cfg(windows)]
pub fn set_immutable(path: &Path) -> Result<()> {
    let mut perms = std::fs::metadata(path)
        .with_context(|| format!("failed to read metadata for {}", path.display()))?
        .permissions();
    perms.set_readonly(true);
    std::fs::set_permissions(path, perms)
        .with_context(|| format!("failed to set readonly on {}", path.display()))?;
    Ok(())
}

#[cfg(windows)]
#[allow(clippy::permissions_set_readonly_false)]
pub fn clear_immutable(path: &Path) -> Result<()> {
    let mut perms = std::fs::metadata(path)
        .with_context(|| format!("failed to read metadata for {}", path.display()))?
        .permissions();
    perms.set_readonly(false);
    std::fs::set_permissions(path, perms)
        .with_context(|| format!("failed to clear readonly on {}", path.display()))?;
    Ok(())
}

#[cfg(windows)]
pub fn is_immutable(path: &Path) -> Result<bool> {
    let perms = std::fs::metadata(path)
        .with_context(|| format!("failed to read metadata for {}", path.display()))?
        .permissions();
    Ok(perms.readonly())
}

#[derive(Debug, PartialEq, Eq)]
pub enum VerifyResult {
    Ok,
    Modified { expected: String, actual: String },
    Missing,
}

pub fn verify_file(path: &Path, expected_hash: &str) -> Result<VerifyResult> {
    if !path.exists() {
        return Ok(VerifyResult::Missing);
    }

    let actual = hash_file(path)?;
    if actual == expected_hash {
        Ok(VerifyResult::Ok)
    } else {
        Ok(VerifyResult::Modified {
            expected: expected_hash.to_string(),
            actual,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fingerprint(pub Vec<String>);

impl Fingerprint {
    pub fn to_json(&self) -> String {
        serde_json::to_string(&self.0).expect("fingerprint serialization cannot fail")
    }

    pub fn from_json(s: &str) -> Result<Self> {
        let chunks: Vec<String> =
            serde_json::from_str(s).with_context(|| "invalid fingerprint JSON")?;
        Ok(Self(chunks))
    }

    #[must_use]
    pub fn chunks(&self) -> &[String] {
        &self.0
    }
}

impl fmt::Display for Fingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{} chunks]", self.0.len())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkDiff {
    pub index: usize,
    pub offset: u64,
    pub size: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub enum FingerprintResult {
    Ok,
    Modified { changed: Vec<ChunkDiff> },
    Missing,
}

pub fn fingerprint_file(path: &Path) -> Result<Fingerprint> {
    let file =
        std::fs::File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let mut reader = std::io::BufReader::with_capacity(CHUNK_SIZE, file);
    let mut buf = vec![0u8; CHUNK_SIZE];
    let mut chunks = Vec::new();

    loop {
        let n = read_full_chunk(&mut reader, &mut buf)
            .with_context(|| format!("failed to read {}", path.display()))?;
        if n == 0 {
            break;
        }
        let hash = blake3::hash(&buf[..n]);
        chunks.push(hex::encode(&hash.as_bytes()[..CHUNK_HASH_BYTES]));
    }

    Ok(Fingerprint(chunks))
}

pub fn hash_and_fingerprint(path: &Path) -> Result<(String, Fingerprint)> {
    let file =
        std::fs::File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let mut reader = std::io::BufReader::with_capacity(CHUNK_SIZE, file);
    let mut sha_hasher = Sha256::new();
    let mut buf = vec![0u8; CHUNK_SIZE];
    let mut chunks = Vec::new();

    loop {
        let n = read_full_chunk(&mut reader, &mut buf)
            .with_context(|| format!("failed to read {}", path.display()))?;
        if n == 0 {
            break;
        }
        sha_hasher.update(&buf[..n]);
        let chunk_hash = blake3::hash(&buf[..n]);
        chunks.push(hex::encode(&chunk_hash.as_bytes()[..CHUNK_HASH_BYTES]));
    }

    let sha256 = hex::encode(sha_hasher.finalize());
    Ok((sha256, Fingerprint(chunks)))
}

pub fn verify_fingerprint(path: &Path, expected: &Fingerprint) -> Result<FingerprintResult> {
    if !path.exists() {
        return Ok(FingerprintResult::Missing);
    }

    let actual = fingerprint_file(path)?;
    let max_len = expected.0.len().max(actual.0.len());
    let mut changed = Vec::new();

    for i in 0..max_len {
        let expected_chunk = expected.0.get(i);
        let actual_chunk = actual.0.get(i);

        if expected_chunk != actual_chunk {
            let offset = (i as u64) * (CHUNK_SIZE as u64);
            let size = if i < actual.0.len() {
                CHUNK_SIZE as u64
            } else {
                0
            };
            changed.push(ChunkDiff {
                index: i,
                offset,
                size,
            });
        }
    }

    if changed.is_empty() {
        Ok(FingerprintResult::Ok)
    } else {
        Ok(FingerprintResult::Modified { changed })
    }
}

fn read_full_chunk<R: Read>(reader: &mut R, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut total = 0;
    while total < buf.len() {
        let n = reader.read(&mut buf[total..])?;
        if n == 0 {
            break;
        }
        total += n;
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn hash_known_content() {
        let mut tmp = NamedTempFile::new().unwrap();
        write!(tmp, "hello world").unwrap();
        tmp.flush().unwrap();

        let hash = hash_file(tmp.path()).unwrap();
        assert_eq!(
            hash,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }

    #[test]
    fn hash_empty_file() {
        let tmp = NamedTempFile::new().unwrap();
        let hash = hash_file(tmp.path()).unwrap();
        assert_eq!(
            hash,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn verify_ok() {
        let mut tmp = NamedTempFile::new().unwrap();
        write!(tmp, "hello world").unwrap();
        tmp.flush().unwrap();

        let result = verify_file(
            tmp.path(),
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9",
        )
        .unwrap();
        assert_eq!(result, VerifyResult::Ok);
    }

    #[test]
    fn verify_modified() {
        let mut tmp = NamedTempFile::new().unwrap();
        write!(tmp, "hello world").unwrap();
        tmp.flush().unwrap();

        let result = verify_file(tmp.path(), "0000000000000000").unwrap();
        assert!(matches!(result, VerifyResult::Modified { .. }));
    }

    #[test]
    fn verify_missing() {
        let result = verify_file(Path::new("/nonexistent/file"), "abc").unwrap();
        assert_eq!(result, VerifyResult::Missing);
    }

    #[test]
    fn set_immutable_marks_file_readonly() {
        let tmp = NamedTempFile::new().unwrap();
        if set_immutable(tmp.path()).is_err() {
            return; // chattr requires root on Linux
        }
        assert!(is_immutable(tmp.path()).unwrap());
        clear_immutable(tmp.path()).unwrap();
    }

    #[test]
    fn clear_immutable_removes_readonly() {
        let tmp = NamedTempFile::new().unwrap();
        if set_immutable(tmp.path()).is_err() {
            return; // chattr requires root on Linux
        }
        clear_immutable(tmp.path()).unwrap();
        assert!(!is_immutable(tmp.path()).unwrap());
    }

    #[test]
    fn is_immutable_false_by_default() {
        let tmp = NamedTempFile::new().unwrap();
        assert!(!is_immutable(tmp.path()).unwrap());
    }

    #[test]
    fn fingerprint_empty_file() {
        let tmp = NamedTempFile::new().unwrap();
        let fp = fingerprint_file(tmp.path()).unwrap();
        assert!(fp.0.is_empty());
        assert_eq!(fp.to_json(), "[]");
    }

    #[test]
    fn fingerprint_small_file() {
        let mut tmp = NamedTempFile::new().unwrap();
        write!(tmp, "hello world").unwrap();
        tmp.flush().unwrap();

        let fp = fingerprint_file(tmp.path()).unwrap();
        assert_eq!(fp.0.len(), 1);
        assert_eq!(fp.0[0].len(), CHUNK_HASH_BYTES * 2);
    }

    #[test]
    fn fingerprint_multi_chunk() {
        let mut tmp = NamedTempFile::new().unwrap();
        let data = vec![0xABu8; CHUNK_SIZE + 100];
        tmp.write_all(&data).unwrap();
        tmp.flush().unwrap();

        let fp = fingerprint_file(tmp.path()).unwrap();
        assert_eq!(fp.0.len(), 2);
        assert_ne!(fp.0[0], fp.0[1]);
    }

    #[test]
    fn hash_and_fingerprint_single_pass() {
        let mut tmp = NamedTempFile::new().unwrap();
        let data = vec![0x42u8; CHUNK_SIZE * 3 + 500];
        tmp.write_all(&data).unwrap();
        tmp.flush().unwrap();

        let (sha, fp) = hash_and_fingerprint(tmp.path()).unwrap();
        let sha_only = hash_file(tmp.path()).unwrap();
        let fp_only = fingerprint_file(tmp.path()).unwrap();

        assert_eq!(sha, sha_only);
        assert_eq!(fp, fp_only);
    }

    #[test]
    fn fingerprint_json_round_trip() {
        let mut tmp = NamedTempFile::new().unwrap();
        write!(tmp, "test data for fingerprint").unwrap();
        tmp.flush().unwrap();

        let fp = fingerprint_file(tmp.path()).unwrap();
        let json = fp.to_json();
        let restored = Fingerprint::from_json(&json).unwrap();
        assert_eq!(fp, restored);
    }

    #[test]
    fn verify_fingerprint_ok() {
        let mut tmp = NamedTempFile::new().unwrap();
        write!(tmp, "unchanged content").unwrap();
        tmp.flush().unwrap();

        let fp = fingerprint_file(tmp.path()).unwrap();
        let result = verify_fingerprint(tmp.path(), &fp).unwrap();
        assert_eq!(result, FingerprintResult::Ok);
    }

    #[test]
    fn verify_fingerprint_modified() {
        use std::io::{Seek, SeekFrom};

        let mut tmp = NamedTempFile::new().unwrap();
        let data = vec![0u8; CHUNK_SIZE * 3];
        tmp.write_all(&data).unwrap();
        tmp.flush().unwrap();

        let fp = fingerprint_file(tmp.path()).unwrap();
        assert_eq!(fp.0.len(), 3);

        // Modify the second chunk
        tmp.seek(SeekFrom::Start(CHUNK_SIZE as u64 + 10)).unwrap();
        tmp.write_all(b"MODIFIED").unwrap();
        tmp.flush().unwrap();

        let result = verify_fingerprint(tmp.path(), &fp).unwrap();
        match result {
            FingerprintResult::Modified { changed } => {
                assert_eq!(changed.len(), 1);
                assert_eq!(changed[0].index, 1);
                assert_eq!(changed[0].offset, CHUNK_SIZE as u64);
            }
            other => panic!("expected Modified, got {other:?}"),
        }
    }

    #[test]
    fn verify_fingerprint_missing() {
        let fp = Fingerprint(vec!["abc123".to_string()]);
        let result = verify_fingerprint(Path::new("/nonexistent/file"), &fp).unwrap();
        assert_eq!(result, FingerprintResult::Missing);
    }
}
