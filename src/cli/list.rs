use std::path::Path;

use anyhow::{bail, Result};
use console::style;

use crate::context::{discover, Context, Scope};

pub fn run(cwd: &Path, scope: &Scope, tag: Option<&str>) -> Result<()> {
    let ctx = discover(cwd)?;

    match (&ctx, scope) {
        (Context::Project { project_db, .. }, Scope::Current) => {
            let files = if let Some(t) = tag {
                project_db.list_files_by_tag(t)?
            } else {
                project_db.list_files(None)?
            };
            print_files(&files);
        }
        (Context::Project { project_db, .. }, Scope::Project { path, .. }) => {
            let prefix = if path.is_empty() {
                None
            } else {
                Some(format!("{}/", path.join("/")))
            };
            let files = if let Some(t) = tag {
                let mut all = project_db.list_files_by_tag(t)?;
                if let Some(ref pfx) = prefix {
                    all.retain(|f| f.path.starts_with(pfx.as_str()));
                }
                all
            } else {
                project_db.list_files(prefix.as_deref())?
            };
            print_files(&files);
        }
        (
            Context::Project {
                workspace: Some(ws),
                ..
            },
            Scope::Workspace,
        ) => {
            let projects = ws.workspace_db.list_projects()?;
            for proj in &projects {
                let proj_root = ws.workspace_root.join(&proj.path);
                let mkrk = proj_root.join(".mkrk");
                if mkrk.exists() {
                    let proj_db = crate::db::ProjectDb::open(&mkrk)?;
                    let files = if let Some(t) = tag {
                        proj_db.list_files_by_tag(t)?
                    } else {
                        proj_db.list_files(None)?
                    };
                    if !files.is_empty() {
                        eprintln!("{}:", style(&proj.name).bold());
                        print_files(&files);
                    }
                }
            }
        }
        (
            Context::Workspace {
                workspace_db,
                workspace_root,
                ..
            },
            _,
        ) => {
            let projects = workspace_db.list_projects()?;
            for proj in &projects {
                let proj_root = workspace_root.join(&proj.path);
                let mkrk = proj_root.join(".mkrk");
                if mkrk.exists() {
                    let proj_db = crate::db::ProjectDb::open(&mkrk)?;
                    let files = if let Some(t) = tag {
                        proj_db.list_files_by_tag(t)?
                    } else {
                        proj_db.list_files(None)?
                    };
                    if !files.is_empty() {
                        eprintln!("{}:", style(&proj.name).bold());
                        print_files(&files);
                    }
                }
            }
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
