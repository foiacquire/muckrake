use std::io::Read;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};

const BUF_SIZE: usize = 64 * 1024;

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

pub fn set_immutable(path: &Path) -> Result<()> {
    let output = Command::new("chattr")
        .arg("+i")
        .arg(path)
        .output()
        .with_context(|| format!("failed to run chattr +i on {}", path.display()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("chattr +i failed on {}: {}", path.display(), stderr.trim());
    }
    Ok(())
}

pub fn clear_immutable(path: &Path) -> Result<()> {
    let output = Command::new("chattr")
        .arg("-i")
        .arg(path)
        .output()
        .with_context(|| format!("failed to run chattr -i on {}", path.display()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("chattr -i failed on {}: {}", path.display(), stderr.trim());
    }
    Ok(())
}

pub fn is_immutable(path: &Path) -> Result<bool> {
    let output = Command::new("lsattr")
        .arg("-d")
        .arg(path)
        .output()
        .with_context(|| format!("failed to run lsattr on {}", path.display()))?;

    if !output.status.success() {
        return Ok(false);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let attrs = stdout.split_whitespace().next().unwrap_or("");
    Ok(attrs.contains('i'))
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
}
