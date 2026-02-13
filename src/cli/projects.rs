use std::path::Path;

use anyhow::{bail, Result};
use console::style;

use crate::context::{discover, Context, WorkspaceContext};

pub fn run(cwd: &Path) -> Result<()> {
    let ctx = discover(cwd)?;

    let (Context::Workspace { workspace_db, .. }
    | Context::Project {
        workspace: Some(WorkspaceContext { workspace_db, .. }),
        ..
    }) = ctx
    else {
        bail!("must be inside a workspace to list projects");
    };

    let projects = workspace_db.list_projects()?;

    if projects.is_empty() {
        eprintln!("No projects in workspace");
        return Ok(());
    }

    for proj in &projects {
        let desc = proj.description.as_deref().unwrap_or("");
        println!(
            "  {} {} {}",
            style(&proj.name).bold(),
            style(&proj.path).dim(),
            if desc.is_empty() {
                String::new()
            } else {
                format!("- {desc}")
            }
        );
    }

    Ok(())
}
