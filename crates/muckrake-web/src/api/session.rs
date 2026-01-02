use axum::{Json, Router, extract::State, http::StatusCode, response::IntoResponse, routing::{get, post}};
use axum_extra::extract::CookieJar;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::session::{SessionToken, session_cookie};
use crate::state::{AppState, ProjectId};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(get_session))
        .route("/project", post(create_project))
        .route("/project/open", post(open_project))
        .route("/projects", get(list_projects))
}

#[derive(Debug, Serialize)]
pub struct ProjectInfo {
    pub id: ProjectId,
    pub name: String,
    pub saved: bool,
    pub created_at: String,
    pub modified_at: String,
}

#[derive(Debug, Serialize)]
pub struct SessionResponse {
    pub session_id: Uuid,
    pub project: Option<ProjectInfo>,
}

async fn get_session(
    State(state): State<AppState>,
    session: SessionToken,
    jar: CookieJar,
) -> impl IntoResponse {
    let manager = state.manager.read().await;

    let project = manager.get_session_project(session.0).map(|p| ProjectInfo {
        id: p.id,
        name: p.name.clone(),
        saved: p.file_path.is_some(),
        created_at: p.created_at.to_rfc3339(),
        modified_at: p.modified_at.to_rfc3339(),
    });

    let response = SessionResponse {
        session_id: session.0,
        project,
    };

    // Ensure session cookie is set
    let jar = jar.add(session_cookie(session.0));

    (jar, Json(response))
}

#[derive(Debug, Deserialize)]
pub struct CreateProjectRequest {
    pub name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CreateProjectResponse {
    pub project: ProjectInfo,
}

async fn create_project(
    State(state): State<AppState>,
    session: SessionToken,
    jar: CookieJar,
    Json(req): Json<CreateProjectRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let name = req.name.unwrap_or_else(|| "Untitled Investigation".to_string());

    let mut manager = state.manager.write().await;
    let project_id = manager
        .create_project(session.0, name)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let project = manager.get_project(project_id).ok_or_else(|| {
        (StatusCode::INTERNAL_SERVER_ERROR, "Project created but not found".to_string())
    })?;

    let response = CreateProjectResponse {
        project: ProjectInfo {
            id: project.id,
            name: project.name.clone(),
            saved: project.file_path.is_some(),
            created_at: project.created_at.to_rfc3339(),
            modified_at: project.modified_at.to_rfc3339(),
        },
    };

    let jar = jar.add(session_cookie(session.0));

    Ok((StatusCode::CREATED, jar, Json(response)))
}

#[derive(Debug, Deserialize)]
pub struct OpenProjectRequest {
    pub path: String,
}

#[derive(Debug, Serialize)]
pub struct OpenProjectResponse {
    pub path: String,
    pub project: ProjectInfo,
}

async fn open_project(
    State(state): State<AppState>,
    session: SessionToken,
    jar: CookieJar,
    Json(req): Json<OpenProjectRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let dir = std::path::Path::new(&req.path);

    // Create directory if it doesn't exist
    std::fs::create_dir_all(dir).map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to create directory: {}", e))
    })?;

    // Derive project name from directory name
    let name = dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "Untitled Investigation".to_string());

    let mut manager = state.manager.write().await;
    let project_id = manager
        .open_project(session.0, &req.path, name)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let project = manager.get_project(project_id).ok_or_else(|| {
        (StatusCode::INTERNAL_SERVER_ERROR, "Project opened but not found".to_string())
    })?;

    let response = OpenProjectResponse {
        path: req.path,
        project: ProjectInfo {
            id: project.id,
            name: project.name.clone(),
            saved: true,
            created_at: project.created_at.to_rfc3339(),
            modified_at: project.modified_at.to_rfc3339(),
        },
    };

    let jar = jar.add(session_cookie(session.0));

    Ok((jar, Json(response)))
}

// List projects endpoint

#[derive(Debug, Serialize)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
}

#[derive(Debug, Serialize)]
pub struct ProjectWithFiles {
    pub id: ProjectId,
    pub name: String,
    pub path: String,
    pub files: Vec<FileEntry>,
}

#[derive(Debug, Serialize)]
pub struct ListProjectsResponse {
    pub projects: Vec<ProjectWithFiles>,
}

fn list_directory_files(dir_path: &str) -> Vec<FileEntry> {
    let path = std::path::Path::new(dir_path);
    let mut files = Vec::new();

    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let file_name = entry.file_name().to_string_lossy().to_string();
            // Skip hidden files (like .mkprj database)
            if file_name.starts_with('.') {
                continue;
            }
            let file_path = entry.path();
            let is_dir = file_path.is_dir();
            files.push(FileEntry {
                name: file_name,
                path: file_path.to_string_lossy().to_string(),
                is_dir,
            });
        }
    }

    // Sort: directories first, then by name
    files.sort_by(|a, b| {
        match (a.is_dir, b.is_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        }
    });

    files
}

async fn list_projects(
    State(state): State<AppState>,
    session: SessionToken,
) -> impl IntoResponse {
    let manager = state.manager.read().await;

    let mut projects = Vec::new();

    // Get the project for this session
    if let Some(project) = manager.get_session_project(session.0) {
        if let Some(ref dir_path) = project.file_path {
            let files = list_directory_files(dir_path);
            projects.push(ProjectWithFiles {
                id: project.id,
                name: project.name.clone(),
                path: dir_path.clone(),
                files,
            });
        }
    }

    Json(ListProjectsResponse { projects })
}
