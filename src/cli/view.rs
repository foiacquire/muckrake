use std::path::Path;
use std::process::Command;

use anyhow::{bail, Result};

use crate::context::{discover, Context};
use crate::models::ProtectionLevel;
use crate::tools;

pub fn run_view(cwd: &Path, name: &str) -> Result<()> {
    run_open(cwd, name, "view")
}

pub fn run_edit(cwd: &Path, name: &str) -> Result<()> {
    run_open(cwd, name, "edit")
}

fn run_open(cwd: &Path, name: &str, action: &str) -> Result<()> {
    let ctx = discover(cwd)?;
    let (project_root, project_db, workspace_db) = match ctx {
        Context::Project {
            project_root,
            project_db,
            workspace,
        } => {
            let ws = workspace.map(|w| w.workspace_db);
            (project_root, project_db, ws)
        }
        _ => bail!("must be inside a project to {action} files"),
    };

    let file = project_db
        .get_file_by_name(name)?
        .ok_or_else(|| anyhow::anyhow!("file '{name}' not found in project"))?;

    let file_path = project_root.join(&file.path);
    if !file_path.exists() {
        bail!("file missing from disk: {}", file.path);
    }

    let protection = check_edit_permission(&project_db, &file.path, name, action)?;

    let command_str = resolve_open_tool(&file, action, &project_db, workspace_db.as_ref())?;
    let target_path = resolve_open_path(&file_path, action, &protection)?;

    let env_map = tools::build_tool_env(None, &command_str);
    let mut cmd = Command::new(&command_str);
    cmd.arg(&target_path);
    tools::apply_env(&mut cmd, &env_map);

    let status = cmd.status()?;

    if let Some(temp) = is_temp_path(&target_path, &file_path) {
        let _ = std::fs::remove_file(&temp);
    }

    let user = whoami();
    project_db.insert_audit(action, file.id, Some(&user), None)?;

    if !status.success() {
        bail!("tool '{command_str}' exited with {status}");
    }

    Ok(())
}

fn check_edit_permission(
    db: &crate::db::ProjectDb,
    rel_path: &str,
    name: &str,
    action: &str,
) -> Result<ProtectionLevel> {
    let matched_cat = db.match_category(rel_path)?;
    let protection = matched_cat
        .as_ref()
        .map(|c| &c.protection_level)
        .cloned()
        .unwrap_or(ProtectionLevel::Editable);

    if action == "edit" && protection == ProtectionLevel::Immutable {
        bail!("cannot edit immutable file '{name}'");
    }
    if action == "edit" && protection == ProtectionLevel::Protected {
        eprintln!("Warning: editing protected file '{name}'");
    }

    Ok(protection)
}

fn resolve_open_tool(
    file: &crate::models::TrackedFile,
    action: &str,
    project_db: &crate::db::ProjectDb,
    workspace_db: Option<&crate::db::WorkspaceDb>,
) -> Result<String> {
    let file_ext = file.name.rsplit('.').next().unwrap_or("*");
    let tags = file
        .id
        .map(|id| project_db.get_tags(id))
        .transpose()?
        .unwrap_or_default();

    let scope_chain = build_scope_chain(&file.path);
    let scope_refs: Vec<Option<&str>> = scope_chain.iter().map(|s| s.as_deref()).collect();

    let lookup = tools::ToolLookup {
        action,
        file_type: file_ext,
        scope_chain: &scope_refs,
        tags: &tags,
    };
    let tool = tools::resolve_tool(&lookup, project_db, workspace_db)?;

    Ok(match &tool {
        Some(t) => t.command.clone(),
        None => tools::default_tool(action),
    })
}

fn resolve_open_path(
    file_path: &Path,
    action: &str,
    protection: &ProtectionLevel,
) -> Result<std::path::PathBuf> {
    match (action, protection) {
        ("view", ProtectionLevel::Immutable | ProtectionLevel::Protected) => {
            let temp_dir = std::env::temp_dir().join("mkrk-view");
            std::fs::create_dir_all(&temp_dir)?;
            let file_name = file_path
                .file_name()
                .map_or_else(|| "file".to_string(), |n| n.to_string_lossy().to_string());
            let temp_path = temp_dir.join(file_name);
            std::fs::copy(file_path, &temp_path)?;
            Ok(temp_path)
        }
        _ => Ok(file_path.to_path_buf()),
    }
}

fn is_temp_path(target: &Path, original: &Path) -> Option<std::path::PathBuf> {
    if target == original {
        None
    } else {
        Some(target.to_path_buf())
    }
}

fn build_scope_chain(rel_path: &str) -> Vec<Option<String>> {
    let mut chain = Vec::new();
    let parts: Vec<&str> = rel_path.split('/').collect();

    for i in (1..parts.len()).rev() {
        let scope = parts[..i].join("/");
        chain.push(Some(scope));
    }
    chain.push(None);

    chain
}

fn whoami() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .unwrap_or_else(|_| "unknown".to_string())
}
