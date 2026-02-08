use std::path::Path;

use anyhow::{bail, Result};
use console::style;

use crate::context::{discover, Context, Scope};
use crate::db::{ProjectDb, ProjectRow, WorkspaceDb};

pub fn run(cwd: &Path, scope: &Scope, tag: Option<&str>) -> Result<()> {
    let ctx = discover(cwd)?;

    match (&ctx, scope) {
        (Context::Project { project_db, .. }, Scope::Current) => {
            let files = query_files(project_db, None, tag)?;
            print_files(&files);
        }
        (Context::Project { project_db, .. }, Scope::Project { path, .. }) => {
            let prefix = if path.is_empty() {
                None
            } else {
                Some(format!("{}/", path.join("/")))
            };
            let files = query_files(project_db, prefix.as_deref(), tag)?;
            print_files(&files);
        }
        (
            Context::Project {
                workspace: Some(ws),
                ..
            },
            Scope::Workspace,
        ) => {
            list_workspace_files(&ws.workspace_db, &ws.workspace_root, tag)?;
        }
        (
            Context::Workspace {
                workspace_db,
                workspace_root,
                ..
            },
            _,
        ) => {
            list_workspace_files(workspace_db, workspace_root, tag)?;
        }
        (Context::None, _) => {
            bail!("not in a muckrake project or workspace");
        }
        _ => {
            bail!("scope not applicable in this context");
        }
    }

    Ok(())
}

fn query_files(
    db: &ProjectDb,
    prefix: Option<&str>,
    tag: Option<&str>,
) -> Result<Vec<crate::models::TrackedFile>> {
    if let Some(t) = tag {
        let mut all = db.list_files_by_tag(t)?;
        if let Some(pfx) = prefix {
            all.retain(|f| f.path.starts_with(pfx));
        }
        Ok(all)
    } else {
        db.list_files(prefix)
    }
}

fn list_workspace_files(
    workspace_db: &WorkspaceDb,
    workspace_root: &Path,
    tag: Option<&str>,
) -> Result<()> {
    let projects = workspace_db.list_projects()?;
    for proj in &projects {
        print_project_files(workspace_root, proj, tag)?;
    }
    Ok(())
}

fn print_project_files(workspace_root: &Path, proj: &ProjectRow, tag: Option<&str>) -> Result<()> {
    let mkrk = workspace_root.join(&proj.path).join(".mkrk");
    if !mkrk.exists() {
        return Ok(());
    }
    let proj_db = ProjectDb::open(&mkrk)?;
    let files = query_files(&proj_db, None, tag)?;
    if !files.is_empty() {
        eprintln!("{}:", style(&proj.name).bold());
        print_files(&files);
    }
    Ok(())
}

fn print_files(files: &[crate::models::TrackedFile]) {
    if files.is_empty() {
        eprintln!("  (no files)");
        return;
    }

    for f in files {
        let protection = if f.immutable { "immutable" } else { "editable" };
        let size = f.size.map_or_else(|| "?".to_string(), format_size);
        eprintln!(
            "  {} {} [{}] {}",
            style(&f.name).bold(),
            style(&f.path).dim(),
            protection,
            style(size).dim()
        );
    }
}

fn format_size(bytes: i64) -> String {
    const KB: i64 = 1024;
    const MB: i64 = KB * 1024;
    const GB: i64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1}G", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1}M", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1}K", bytes as f64 / KB as f64)
    } else {
        format!("{bytes}B")
    }
}
