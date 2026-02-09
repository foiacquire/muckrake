use std::path::{Path, PathBuf};

use anyhow::{bail, Result};

use crate::db::{ProjectDb, WorkspaceDb};

pub enum Context {
    Project {
        project_root: PathBuf,
        project_db: ProjectDb,
        workspace: Option<WorkspaceContext>,
    },
    Workspace {
        workspace_root: PathBuf,
        workspace_db: WorkspaceDb,
    },
    None,
}

pub struct WorkspaceContext {
    pub workspace_root: PathBuf,
    pub workspace_db: WorkspaceDb,
}

pub fn discover(cwd: &Path) -> Result<Context> {
    let mut project_root: Option<PathBuf> = None;
    let mut workspace_root: Option<PathBuf> = None;

    let mut dir = cwd.to_path_buf();
    loop {
        let mkrk = dir.join(".mkrk");
        let mksp = dir.join(".mksp");

        if project_root.is_none() && mkrk.exists() {
            project_root = Some(dir.clone());
        }
        if workspace_root.is_none() && mksp.exists() {
            workspace_root = Some(dir.clone());
        }

        if project_root.is_some() && workspace_root.is_some() {
            break;
        }

        if !dir.pop() {
            break;
        }
    }

    match (project_root, workspace_root) {
        (Some(proj), Some(ws)) => {
            let project_db = ProjectDb::open(&proj.join(".mkrk"))?;
            let workspace_db = WorkspaceDb::open(&ws.join(".mksp"))?;
            Ok(Context::Project {
                project_root: proj,
                project_db,
                workspace: Some(WorkspaceContext {
                    workspace_root: ws,
                    workspace_db,
                }),
            })
        }
        (Some(proj), None) => {
            let project_db = ProjectDb::open(&proj.join(".mkrk"))?;
            Ok(Context::Project {
                project_root: proj,
                project_db,
                workspace: None,
            })
        }
        (None, Some(ws)) => {
            let workspace_db = WorkspaceDb::open(&ws.join(".mksp"))?;
            Ok(Context::Workspace {
                workspace_root: ws,
                workspace_db,
            })
        }
        (None, None) => Ok(Context::None),
    }
}

pub fn find_workspace_root(cwd: &Path) -> Result<PathBuf> {
    let mut dir = cwd.to_path_buf();
    loop {
        if dir.join(".mksp").exists() {
            return Ok(dir);
        }
        if !dir.pop() {
            break;
        }
    }
    bail!("scope prefix requires a workspace (no .mksp found)")
}

pub fn resolve_scope(cwd: &Path, scope: &str) -> Result<PathBuf> {
    let ws_root = find_workspace_root(cwd)?;

    if scope.is_empty() {
        return Ok(ws_root);
    }

    let ws_db = WorkspaceDb::open(&ws_root.join(".mksp"))?;
    let project = ws_db
        .get_project_by_name(scope)?
        .ok_or_else(|| anyhow::anyhow!("project '{scope}' not found in workspace"))?;

    let project_root = ws_root.join(&project.path);
    let mkrk = project_root.join(".mkrk");
    if !mkrk.exists() {
        bail!(
            "project '{scope}' registered but has no .mkrk database"
        );
    }

    Ok(project_root)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn discover_none() {
        let dir = TempDir::new().unwrap();
        let ctx = discover(dir.path()).unwrap();
        assert!(matches!(ctx, Context::None));
    }

    #[test]
    fn discover_project_only() {
        let dir = TempDir::new().unwrap();
        ProjectDb::create(&dir.path().join(".mkrk")).unwrap();
        let ctx = discover(dir.path()).unwrap();
        assert!(matches!(
            ctx,
            Context::Project {
                workspace: None,
                ..
            }
        ));
    }

    #[test]
    fn discover_workspace_only() {
        let dir = TempDir::new().unwrap();
        WorkspaceDb::create(&dir.path().join(".mksp")).unwrap();
        let ctx = discover(dir.path()).unwrap();
        assert!(matches!(ctx, Context::Workspace { .. }));
    }

    #[test]
    fn discover_project_in_workspace() {
        let dir = TempDir::new().unwrap();
        WorkspaceDb::create(&dir.path().join(".mksp")).unwrap();
        let proj_dir = dir.path().join("projects").join("bailey");
        std::fs::create_dir_all(&proj_dir).unwrap();
        ProjectDb::create(&proj_dir.join(".mkrk")).unwrap();
        let ctx = discover(&proj_dir).unwrap();
        assert!(matches!(
            ctx,
            Context::Project {
                workspace: Some(_),
                ..
            }
        ));
    }

    #[test]
    fn discover_from_subdirectory() {
        let dir = TempDir::new().unwrap();
        ProjectDb::create(&dir.path().join(".mkrk")).unwrap();
        let sub = dir.path().join("evidence").join("financial");
        std::fs::create_dir_all(&sub).unwrap();
        let ctx = discover(&sub).unwrap();
        assert!(matches!(ctx, Context::Project { .. }));
    }

    #[test]
    fn find_workspace_root_found() {
        let dir = TempDir::new().unwrap();
        WorkspaceDb::create(&dir.path().join(".mksp")).unwrap();
        let sub = dir.path().join("projects").join("test");
        std::fs::create_dir_all(&sub).unwrap();
        let root = find_workspace_root(&sub).unwrap();
        assert_eq!(root, dir.path());
    }

    #[test]
    fn find_workspace_root_not_found() {
        let dir = TempDir::new().unwrap();
        let err = find_workspace_root(dir.path()).unwrap_err();
        assert!(err.to_string().contains("no .mksp found"));
    }

    #[test]
    fn resolve_scope_workspace() {
        let dir = TempDir::new().unwrap();
        WorkspaceDb::create(&dir.path().join(".mksp")).unwrap();
        let result = resolve_scope(dir.path(), "").unwrap();
        assert_eq!(result, dir.path());
    }

    #[test]
    fn resolve_scope_project() {
        let dir = TempDir::new().unwrap();
        let ws_db = WorkspaceDb::create(&dir.path().join(".mksp")).unwrap();

        let proj_dir = dir.path().join("projects").join("bailey");
        std::fs::create_dir_all(&proj_dir).unwrap();
        ProjectDb::create(&proj_dir.join(".mkrk")).unwrap();
        ws_db
            .register_project("bailey", "projects/bailey", None)
            .unwrap();

        let result = resolve_scope(dir.path(), "bailey").unwrap();
        assert_eq!(result, proj_dir);
    }

    #[test]
    fn resolve_scope_unknown_project() {
        let dir = TempDir::new().unwrap();
        WorkspaceDb::create(&dir.path().join(".mksp")).unwrap();
        let err = resolve_scope(dir.path(), "unknown").unwrap_err();
        assert!(err.to_string().contains("not found in workspace"));
    }

    #[test]
    fn resolve_scope_missing_mkrk() {
        let dir = TempDir::new().unwrap();
        let ws_db = WorkspaceDb::create(&dir.path().join(".mksp")).unwrap();

        let proj_dir = dir.path().join("projects").join("ghost");
        std::fs::create_dir_all(&proj_dir).unwrap();
        ws_db
            .register_project("ghost", "projects/ghost", None)
            .unwrap();

        let err = resolve_scope(dir.path(), "ghost").unwrap_err();
        assert!(err.to_string().contains("no .mkrk database"));
    }
}
