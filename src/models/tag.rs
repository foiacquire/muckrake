use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileTag {
    pub file_id: i64,
    pub tag: String,
    pub file_hash: Option<String>,
}
