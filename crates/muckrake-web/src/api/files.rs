use axum::{Json, Router, extract::Query, response::IntoResponse, routing::get};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/list", get(list_directory))
        .route("/home", get(get_home_dir))
}

#[derive(Debug, Deserialize)]
pub struct ListDirectoryQuery {
    pub path: String,
}

#[derive(Debug, Serialize)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub is_hidden: bool,
}

#[derive(Debug, Serialize)]
pub struct ListDirectoryResponse {
    pub path: String,
    pub parent: Option<String>,
    pub entries: Vec<FileEntry>,
}

async fn list_directory(Query(query): Query<ListDirectoryQuery>) -> impl IntoResponse {
    let path = PathBuf::from(&query.path);

    let parent = path.parent().map(|p| p.to_string_lossy().to_string());

    let mut entries = Vec::new();

    if let Ok(read_dir) = std::fs::read_dir(&path) {
        for entry in read_dir.flatten() {
            let file_name = entry.file_name().to_string_lossy().to_string();
            let file_path = entry.path();
            let is_dir = file_path.is_dir();
            let is_hidden = file_name.starts_with('.');

            entries.push(FileEntry {
                name: file_name,
                path: file_path.to_string_lossy().to_string(),
                is_dir,
                is_hidden,
            });
        }
    }

    // Sort: directories first, then by name (case-insensitive)
    entries.sort_by(|a, b| {
        match (a.is_dir, b.is_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        }
    });

    Json(ListDirectoryResponse {
        path: query.path,
        parent,
        entries,
    })
}

#[derive(Debug, Serialize)]
pub struct HomeDirResponse {
    pub path: String,
}

async fn get_home_dir() -> impl IntoResponse {
    let home = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/"))
        .to_string_lossy()
        .to_string();

    Json(HomeDirResponse { path: home })
}
