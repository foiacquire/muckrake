use axum::{Json, Router, extract::State, http::StatusCode, response::IntoResponse, routing::{get, post}};
use axum_extra::extract::CookieJar;
use serde::{Deserialize, Serialize};

use crate::session::{SessionToken, session_cookie};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_workspaces))
        .route("/dir", get(get_workspaces_dir_path))
        .route("/save", post(save_workspace))
        .route("/open", post(open_workspace))
}

#[derive(Debug, Serialize)]
pub struct WorkspacesDirResponse {
    pub path: String,
}

async fn get_workspaces_dir_path(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    check_remote_workspaces(&state)?;

    let dir = get_workspaces_dir()?;

    // Create the directory if it doesn't exist
    std::fs::create_dir_all(&dir).ok();

    Ok(Json(WorkspacesDirResponse {
        path: dir.to_string_lossy().to_string(),
    }))
}

fn get_workspaces_dir() -> Result<std::path::PathBuf, (StatusCode, String)> {
    let data_dir = dirs::data_dir()
        .ok_or_else(|| (StatusCode::INTERNAL_SERVER_ERROR, "Could not determine data directory".to_string()))?;
    Ok(data_dir.join("muckrake").join("workspaces"))
}

fn safe_filename(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}

// Check if remote workspaces are enabled
fn check_remote_workspaces(state: &AppState) -> Result<(), (StatusCode, String)> {
    if !state.config.allow_remote_workspaces {
        return Err((StatusCode::FORBIDDEN, "Remote workspaces are disabled on this server".to_string()));
    }
    Ok(())
}

// List saved workspaces

#[derive(Debug, Serialize)]
pub struct WorkspaceListItem {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Serialize)]
pub struct ListWorkspacesResponse {
    pub workspaces: Vec<WorkspaceListItem>,
}

async fn list_workspaces(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    check_remote_workspaces(&state)?;

    let workspaces_dir = get_workspaces_dir()?;

    let mut workspaces = Vec::new();

    if let Ok(entries) = std::fs::read_dir(&workspaces_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "mkspc") {
                let name = path
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();
                workspaces.push(WorkspaceListItem {
                    name,
                    path: path.to_string_lossy().to_string(),
                });
            }
        }
    }

    workspaces.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    Ok(Json(ListWorkspacesResponse { workspaces }))
}

// Save workspace

#[derive(Debug, Deserialize)]
pub struct SaveWorkspaceRequest {
    pub name: String,
    pub projects: Vec<ProjectRef>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProjectRef {
    pub path: String,
    #[serde(default = "default_mode")]
    pub mode: String,
}

fn default_mode() -> String {
    "readwrite".to_string()
}

#[derive(Debug, Deserialize, Serialize)]
pub struct WorkspaceFile {
    pub version: u32,
    pub name: String,
    pub projects: Vec<ProjectRef>,
}

#[derive(Debug, Serialize)]
pub struct SaveWorkspaceResponse {
    pub path: String,
    pub name: String,
}

async fn save_workspace(
    State(state): State<AppState>,
    Json(req): Json<SaveWorkspaceRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    check_remote_workspaces(&state)?;

    let workspaces_dir = get_workspaces_dir()?;
    std::fs::create_dir_all(&workspaces_dir).map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to create workspaces directory: {}", e))
    })?;

    let safe_name = safe_filename(&req.name);
    let save_path = workspaces_dir.join(format!("{}.mkspc", safe_name));

    let workspace = WorkspaceFile {
        version: 1,
        name: req.name.clone(),
        projects: req.projects,
    };

    let content = serde_json::to_string_pretty(&workspace).map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to serialize workspace: {}", e))
    })?;

    std::fs::write(&save_path, content).map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to save workspace: {}", e))
    })?;

    Ok(Json(SaveWorkspaceResponse {
        path: save_path.to_string_lossy().to_string(),
        name: req.name,
    }))
}

// Open workspace

#[derive(Debug, Deserialize)]
pub struct OpenWorkspaceRequest {
    pub path: String,
}

#[derive(Debug, Serialize)]
pub struct OpenWorkspaceResponse {
    pub name: String,
    pub projects: Vec<ProjectRef>,
}

async fn open_workspace(
    State(state): State<AppState>,
    session: SessionToken,
    jar: CookieJar,
    Json(req): Json<OpenWorkspaceRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    check_remote_workspaces(&state)?;

    let path = std::path::Path::new(&req.path);

    let content = std::fs::read_to_string(path).map_err(|e| {
        (StatusCode::NOT_FOUND, format!("Failed to read workspace: {}", e))
    })?;

    let workspace: WorkspaceFile = serde_json::from_str(&content).map_err(|e| {
        (StatusCode::BAD_REQUEST, format!("Invalid workspace file: {}", e))
    })?;

    // Open all projects in the workspace
    let mut manager = state.manager.write().await;
    for project_ref in &workspace.projects {
        let project_path = if std::path::Path::new(&project_ref.path).is_absolute() {
            project_ref.path.clone()
        } else {
            // Resolve relative to workspace file
            path.parent()
                .map(|p| p.join(&project_ref.path))
                .unwrap_or_else(|| std::path::PathBuf::from(&project_ref.path))
                .to_string_lossy()
                .to_string()
        };

        let name = std::path::Path::new(&project_path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "Untitled".to_string());

        if let Err(e) = manager.open_project(session.0, &project_path, name).await {
            eprintln!("Failed to open project {}: {}", project_path, e);
        }
    }

    let jar = jar.add(session_cookie(session.0));

    Ok((jar, Json(OpenWorkspaceResponse {
        name: workspace.name,
        projects: workspace.projects,
    })))
}
