#[derive(Debug, Clone)]
pub struct Sign {
    pub id: Option<i64>,
    pub pipeline_id: i64,
    pub file_id: i64,
    pub file_hash: String,
    pub sign_name: String,
    pub signer: String,
    pub signed_at: String,
    pub signature: Option<String>,
    pub revoked_at: Option<String>,
}

impl Sign {
    pub fn is_valid(&self, current_hash: &str) -> bool {
        self.revoked_at.is_none() && self.file_hash == current_hash
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_sign(hash: &str, revoked: bool) -> Sign {
        Sign {
            id: Some(1),
            pipeline_id: 1,
            file_id: 1,
            file_hash: hash.to_string(),
            sign_name: "review".to_string(),
            signer: "alice".to_string(),
            signed_at: "2025-01-01T00:00:00Z".to_string(),
            signature: None,
            revoked_at: if revoked {
                Some("2025-01-02T00:00:00Z".to_string())
            } else {
                None
            },
        }
    }

    #[test]
    fn valid_sign_matching_hash() {
        let sign = make_sign("abc123", false);
        assert!(sign.is_valid("abc123"));
    }

    #[test]
    fn invalid_sign_hash_mismatch() {
        let sign = make_sign("abc123", false);
        assert!(!sign.is_valid("different"));
    }

    #[test]
    fn invalid_sign_revoked() {
        let sign = make_sign("abc123", true);
        assert!(!sign.is_valid("abc123"));
    }

    #[test]
    fn invalid_sign_revoked_and_mismatch() {
        let sign = make_sign("abc123", true);
        assert!(!sign.is_valid("different"));
    }
}
