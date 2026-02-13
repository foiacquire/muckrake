use std::path::Path;

use anyhow::{bail, Result};
use console::style;

use crate::context::{discover, Context, WorkspaceContext};
use crate::db::{ProjectDb, WorkspaceDb};
use crate::util::format_size;

pub fn run_list(cwd: &Path) -> Result<()> {
    let (workspace_root, workspace_db) = resolve_workspace(cwd)?;
    let inbox_dir = get_inbox_dir(&workspace_root, &workspace_db)?;

    if !inbox_dir.exists() {
        eprintln!("Inbox is empty");
        return Ok(());
    }

    let mut entries = Vec::new();
    for entry in std::fs::read_dir(&inbox_dir)? {
        let entry = entry?;
        if entry.path().is_file() {
            entries.push(entry);
        }
    }

    if entries.is_empty() {
        eprintln!("Inbox is empty");
        return Ok(());
    }

    println!("Inbox ({} files):", entries.len());
    for entry in &entries {
        let name = entry.file_name().to_string_lossy().to_string();
        let size = format_size(entry.metadata()?.len() as i64);
        println!("  {} {}", style(&name).bold(), style(size).dim());
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

    let dest_rel = match category {
        Some(cat) => format!("{cat}/{file}"),
        None => file.to_string(),
    };
    let dest_path = proj_root.join(&dest_rel);

    if dest_path.exists() {
        bail!("destination already exists: {}", dest_path.display());
    }
    if let Some(parent) = dest_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::copy(&file_path, &dest_path)?;

    let proj_db = ProjectDb::open(&proj_mkrk)?;
    crate::cli::ingest::track_file(&proj_root, &proj_db, &dest_path, &dest_rel)?;

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
