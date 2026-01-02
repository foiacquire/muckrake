use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use muckrake_core::Storage;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::config::ServerConfig;
use crate::workspace::Workspace;

/// Unique identifier for a project
pub type ProjectId = Uuid;

/// Unique identifier for a session
pub type SessionId = Uuid;

/// Unique identifier for a workspace
pub type WorkspaceId = Uuid;

/// A project contains an investigation's data
pub struct Project {
    pub id: ProjectId,
    pub name: String,
    pub storage: Storage,
    pub file_path: Option<String>,
    pub created_at: DateTime<Utc>,
    pub modified_at: DateTime<Utc>,
}

impl Project {
    pub async fn new_in_memory(name: String) -> anyhow::Result<Self> {
        let now = Utc::now();
        Ok(Self {
            id: Uuid::new_v4(),
            name,
            storage: Storage::open_memory().await?,
            file_path: None,
            created_at: now,
            modified_at: now,
        })
    }

    pub async fn open(dir_path: &str, name: String) -> anyhow::Result<Self> {
        let dir = std::path::Path::new(dir_path);
        let db_path = dir.join(".mkprj");
        let db_path_str = db_path.to_string_lossy().to_string();
        let now = Utc::now();
        Ok(Self {
            id: Uuid::new_v4(),
            name,
            storage: Storage::open(&db_path_str).await?,
            file_path: Some(dir_path.to_string()),
            created_at: now,
            modified_at: now,
        })
    }

    pub fn touch(&mut self) {
        self.modified_at = Utc::now();
    }
}

/// Session tracks a user's connection and their current project/workspace
pub struct Session {
    pub id: SessionId,
    pub project_id: Option<ProjectId>,
    pub workspace_id: Option<WorkspaceId>,
    pub created_at: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
}

impl Session {
    pub fn new() -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            project_id: None,
            workspace_id: None,
            created_at: now,
            last_seen: now,
        }
    }

    pub fn touch(&mut self) {
        self.last_seen = Utc::now();
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

/// Manages all projects, workspaces, and sessions
///
/// Design for future collaboration:
/// - Multiple sessions can share the same project_id/workspace_id
/// - Projects and workspaces are identified by UUID, not tied to a single session
/// - Sessions can switch between projects/workspaces
pub struct ProjectManager {
    projects: HashMap<ProjectId, Project>,
    workspaces: HashMap<WorkspaceId, Workspace>,
    sessions: HashMap<SessionId, Session>,
}

impl ProjectManager {
    pub fn new() -> Self {
        Self {
            projects: HashMap::new(),
            workspaces: HashMap::new(),
            sessions: HashMap::new(),
        }
    }

    /// Get or create a session by ID
    pub fn get_or_create_session(&mut self, session_id: SessionId) -> &mut Session {
        self.sessions.entry(session_id).or_insert_with(|| {
            let mut session = Session::new();
            session.id = session_id;
            session
        })
    }

    /// Get a session by ID
    pub fn get_session(&self, session_id: SessionId) -> Option<&Session> {
        self.sessions.get(&session_id)
    }

    /// Get a session mutably
    pub fn get_session_mut(&mut self, session_id: SessionId) -> Option<&mut Session> {
        self.sessions.get_mut(&session_id)
    }

    /// Create a new in-memory project and associate it with a session
    pub async fn create_project(&mut self, session_id: SessionId, name: String) -> anyhow::Result<ProjectId> {
        let project = Project::new_in_memory(name).await?;
        let project_id = project.id;
        self.projects.insert(project_id, project);

        if let Some(session) = self.sessions.get_mut(&session_id) {
            session.project_id = Some(project_id);
        }

        Ok(project_id)
    }

    /// Open a project from disk and associate it with a session
    pub async fn open_project(&mut self, session_id: SessionId, path: &str, name: String) -> anyhow::Result<ProjectId> {
        let project = Project::open(path, name).await?;
        let project_id = project.id;
        self.projects.insert(project_id, project);

        if let Some(session) = self.sessions.get_mut(&session_id) {
            session.project_id = Some(project_id);
        }

        Ok(project_id)
    }

    /// Get the project for a session
    pub fn get_session_project(&self, session_id: SessionId) -> Option<&Project> {
        let session = self.sessions.get(&session_id)?;
        let project_id = session.project_id?;
        self.projects.get(&project_id)
    }

    /// Get the project for a session mutably
    pub fn get_session_project_mut(&mut self, session_id: SessionId) -> Option<&mut Project> {
        let project_id = self.sessions.get(&session_id)?.project_id?;
        self.projects.get_mut(&project_id)
    }

    /// Get a project by ID
    pub fn get_project(&self, project_id: ProjectId) -> Option<&Project> {
        self.projects.get(&project_id)
    }

    /// Get a project by ID mutably
    pub fn get_project_mut(&mut self, project_id: ProjectId) -> Option<&mut Project> {
        self.projects.get_mut(&project_id)
    }

    /// Remove a project (for cleanup)
    pub fn remove_project(&mut self, project_id: ProjectId) -> Option<Project> {
        // Remove project association from all sessions
        for session in self.sessions.values_mut() {
            if session.project_id == Some(project_id) {
                session.project_id = None;
            }
        }
        self.projects.remove(&project_id)
    }

    /// Get count of active sessions for a project (for future collaboration)
    pub fn project_session_count(&self, project_id: ProjectId) -> usize {
        self.sessions
            .values()
            .filter(|s| s.project_id == Some(project_id))
            .count()
    }

    // Workspace methods

    /// Create a new workspace and associate it with a session
    pub fn create_workspace(&mut self, session_id: SessionId, name: String) -> WorkspaceId {
        let workspace = Workspace::new(name);
        let workspace_id = Uuid::new_v4();

        self.workspaces.insert(workspace_id, workspace);

        if let Some(session) = self.sessions.get_mut(&session_id) {
            session.workspace_id = Some(workspace_id);
        }

        workspace_id
    }

    /// Get the workspace for a session
    pub fn get_session_workspace(&self, session_id: SessionId) -> Option<&Workspace> {
        let session = self.sessions.get(&session_id)?;
        let workspace_id = session.workspace_id?;
        self.workspaces.get(&workspace_id)
    }

    /// Get the workspace for a session mutably
    pub fn get_session_workspace_mut(&mut self, session_id: SessionId) -> Option<&mut Workspace> {
        let workspace_id = self.sessions.get(&session_id)?.workspace_id?;
        self.workspaces.get_mut(&workspace_id)
    }

    /// Get a workspace by ID
    pub fn get_workspace(&self, workspace_id: WorkspaceId) -> Option<&Workspace> {
        self.workspaces.get(&workspace_id)
    }

    /// Get a workspace by ID mutably
    pub fn get_workspace_mut(&mut self, workspace_id: WorkspaceId) -> Option<&mut Workspace> {
        self.workspaces.get_mut(&workspace_id)
    }
}

impl Default for ProjectManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Application state shared across all requests
#[derive(Clone)]
pub struct AppState {
    pub manager: Arc<RwLock<ProjectManager>>,
    pub config: ServerConfig,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            manager: Arc::new(RwLock::new(ProjectManager::new())),
            config: ServerConfig::from_env(),
        }
    }

    pub fn with_config(config: ServerConfig) -> Self {
        Self {
            manager: Arc::new(RwLock::new(ProjectManager::new())),
            config,
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
