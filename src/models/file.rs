use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackedFile {
    pub id: Option<i64>,
    pub name: Option<String>,
    pub path: Option<String>,
    pub sha256: String,
    pub fingerprint: String,
    pub mime_type: Option<String>,
    pub size: Option<i64>,
    pub ingested_at: String,
    pub provenance: Option<String>,
    pub immutable: bool,
}
