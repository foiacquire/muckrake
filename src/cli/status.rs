use std::path::Path;

use anyhow::Result;
use console::style;

use crate::context::{discover, Context};
use crate::db::ProjectDb;

pub fn run(cwd: &Path) -> Result<()> {
    let ctx = discover(cwd)?;

    match ctx {
        Context::Project {
            project_root,
            project_db,
            workspace,
        } => {
            let name = project_root.file_name().map_or_else(
                || "unknown".to_string(),
                |n| n.to_string_lossy().to_string(),
            );
            eprintln!("{} Project: {}", style("●").green(), style(&name).bold());
            eprintln!("  Path: {}", project_root.display());
            print_project_stats(&project_db)?;

            if let Some(ws) = workspace {
                eprintln!("  Workspace: {}", ws.workspace_root.display());
            }
        }
        Context::Workspace {
            workspace_root,
            workspace_db,
        } => {
            eprintln!(
                "{} Workspace: {}",
                style("●").blue(),
                workspace_root.display()
            );
            let project_count = workspace_db.project_count()?;
            eprintln!("  Projects: {project_count}");

            if let Some(inbox_dir) = workspace_db.get_config("inbox_dir")? {
                let inbox_path = workspace_root.join(&inbox_dir);
                let inbox_count = count_inbox_files(&inbox_path)?;
                eprintln!("  Inbox: {inbox_count} files");
            }
        }
        Context::None => {
            eprintln!(
                "{} Not in a muckrake project or workspace",
                style("○").dim()
            );
            eprintln!("  Run 'mkrk init' to create a project");
            eprintln!("  Run 'mkrk init -w <dir>' to create a workspace");
        }
    }

    Ok(())
}

fn print_project_stats(db: &ProjectDb) -> Result<()> {
    let files = db.file_count()?;
    let categories = db.category_count()?;
    let tags = db.tag_count()?;
    let pipelines = db.pipeline_count()?;
    let signs = db.sign_count()?;

    eprintln!("  Files: {files}");
    eprintln!("  Categories: {categories}");
    eprintln!("  Tags: {tags}");

    if pipelines > 0 {
        eprintln!("  Pipelines: {pipelines}");
        eprintln!("  Active signs: {signs}");
    }

    if let Some(last_verify) = db.last_verify_time()? {
        eprintln!("  Last verified: {last_verify}");
    }

    Ok(())
}

fn count_inbox_files(inbox_path: &Path) -> Result<usize> {
    if !inbox_path.exists() {
        return Ok(0);
    }
    let mut count = 0;
    for entry in std::fs::read_dir(inbox_path)? {
        let entry = entry?;
        if entry.path().is_file() {
            count += 1;
        }
    }
    Ok(count)
}
