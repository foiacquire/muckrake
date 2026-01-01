use std::sync::Arc;

use muckrake_core::Storage;
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct AppState {
    pub storage: Arc<RwLock<Storage>>,
}

impl AppState {
    pub async fn new(db_path: &str) -> anyhow::Result<Self> {
        let storage = Storage::open(db_path).await?;
        Ok(Self {
            storage: Arc::new(RwLock::new(storage)),
        })
    }
}
