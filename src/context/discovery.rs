use std::path::{Path, PathBuf};

use anyhow::Result;

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
}
