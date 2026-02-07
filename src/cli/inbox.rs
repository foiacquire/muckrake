use std::path::Path;

use anyhow::{bail, Result};
use console::style;

use crate::context::{discover, Context, WorkspaceContext};
use crate::db::WorkspaceDb;

pub fn run_list(cwd: &Path) -> Result<()> {
    let (workspace_root, workspace_db) = resolve_workspace(cwd)?;
    let inbox_dir = get_inbox_dir(&workspace_root, &workspace_db)?;

    if !inbox_dir.exists() {
        eprintln!("Inbox is empty");
        return Ok(());
    }

    let entries: Vec<_> = std::fs::read_dir(&inbox_dir)?
        .filter_map(Result::ok)
        .filter(|e| e.path().is_file())
        .collect();

    if entries.is_empty() {
        eprintln!("Inbox is empty");
        return Ok(());
    }

    eprintln!("Inbox ({} files):", entries.len());
    for entry in &entries {
        let name = entry.file_name().to_string_lossy().to_string();
        let size = entry
            .metadata()
            .map_or_else(|_| "?".to_string(), |m| format_size(m.len()));
        eprintln!("  {} {}", style(&name).bold(), style(size).dim());
    }

    Ok(())
}

pub fn run_assign(cwd: &Path, file: &str, project: &str, category: Option<&str>) -> Result<()> {
    let (workspace_root, workspace_db) = resolve_workspace(cwd)?;
    let inbox_dir = get_inbox_dir(&workspace_root, &workspace_db)?;

    let file_path = inbox_dir.join(file);
    if !file_path.exists() {
        bail!("file '{file}' not found in inbox");
    }

    let proj = workspace_db
        .get_project_by_name(project)?
        .ok_or_else(|| anyhow::anyhow!("project '{project}' not found in workspace"))?;

    let proj_root = workspace_root.join(&proj.path);
    let proj_mkrk = proj_root.join(".mkrk");
    if !proj_mkrk.exists() {
        bail!(
            "project '{}' has no .mkrk database at {}",
            project,
            proj_root.display()
        );
    }

    let proj_cwd = proj_root;
    crate::cli::ingest::run(
        &proj_cwd,
        &[file_path.to_string_lossy().to_string()],
        category,
    )?;

    std::fs::remove_file(&file_path)?;
    eprintln!("Removed {file} from inbox");

    Ok(())
}

fn resolve_workspace(cwd: &Path) -> Result<(std::path::PathBuf, WorkspaceDb)> {
    let ctx = discover(cwd)?;
    match ctx {
        Context::Workspace {
            workspace_root,
            workspace_db,
        }
        | Context::Project {
            workspace:
                Some(WorkspaceContext {
                    workspace_root,
                    workspace_db,
                }),
            ..
        } => Ok((workspace_root, workspace_db)),
        _ => bail!("must be inside a workspace to use inbox"),
    }
}

fn get_inbox_dir(workspace_root: &Path, ws_db: &WorkspaceDb) -> Result<std::path::PathBuf> {
    let inbox_rel = ws_db
        .get_config("inbox_dir")?
        .ok_or_else(|| anyhow::anyhow!("this workspace does not have an inbox configured"))?;
    Ok(workspace_root.join(inbox_rel))
}

fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

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
