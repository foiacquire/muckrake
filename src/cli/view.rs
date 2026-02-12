use std::path::Path;
use std::process::Command;

use anyhow::{bail, Result};

use crate::context::discover;
use crate::models::ProtectionLevel;
use crate::reference::{parse_reference, resolve_references};
use crate::tools;
use crate::util::whoami;

pub fn run_view(cwd: &Path, reference: &str) -> Result<()> {
    run_open(cwd, reference, "view")
}

pub fn run_edit(cwd: &Path, reference: &str) -> Result<()> {
    run_open(cwd, reference, "edit")
}

fn run_open(cwd: &Path, reference: &str, action: &str) -> Result<()> {
    let ctx = discover(cwd)?;
    let (project_root, project_db, workspace_db) = ctx.require_project_with_workspace()?;

    let parsed = parse_reference(reference)?;
    let collection = resolve_references(&[parsed], &ctx)?;
    let resolved = collection.expect_one(reference)?;
    let file = resolved.file;

    let file_path = project_root.join(&file.path);
    if !file_path.exists() {
        bail!("file missing from disk: {}", file.path);
    }

    let protection = project_db.resolve_protection(&file.path)?;

    if action == "edit" && protection == ProtectionLevel::Immutable {
        bail!("cannot edit immutable file '{}'", file.name);
    }

    if action == "edit" && protection == ProtectionLevel::Protected {
        eprintln!("Warning: editing protected file '{}'", file.name);
    }

    let (command_str, env_json) = resolve_tool_for_file(&file, action, project_db, workspace_db)?;
    let env_map = tools::build_tool_env(env_json.as_deref(), &command_str, false)?;

    let (temp_dir, target_path) = resolve_open_path(&file_path, action, protection)?;

    let mut cmd = Command::new(&command_str);
    cmd.arg(&target_path);
    tools::apply_env(&mut cmd, &env_map);

    let status = cmd.status()?;

    if let Some(dir) = temp_dir {
        if let Err(e) = std::fs::remove_dir_all(&dir) {
            eprintln!(
                "warning: failed to clean up temp dir {}: {e}",
                dir.display()
            );
        }
    }

    let user = whoami();
    project_db.insert_audit(action, file.id, Some(&user), None)?;

    if !status.success() {
        bail!("tool '{command_str}' exited with {status}");
    }

    Ok(())
}

fn resolve_tool_for_file(
    file: &crate::models::TrackedFile,
    action: &str,
    project_db: &crate::db::ProjectDb,
    workspace_db: Option<&crate::db::WorkspaceDb>,
) -> Result<(String, Option<String>)> {
    let file_ext = file.name.rsplit('.').next().unwrap_or("*");
    let tags = file
        .id
        .map(|id| project_db.get_tags(id))
        .transpose()?
        .unwrap_or_default();

    let scope_chain = tools::build_scope_chain(&file.path);
    let scope_refs: Vec<Option<&str>> = scope_chain.iter().map(|s| s.as_deref()).collect();

    let lookup = tools::ToolLookup {
        action,
        file_type: file_ext,
        scope_chain: &scope_refs,
        tags: &tags,
    };
    let tool = tools::resolve_tool(&lookup, project_db, workspace_db)?;

    let command_str = match &tool {
        Some(t) => t.command.clone(),
        None => tools::default_tool(action),
    };
    let env_json = tool.and_then(|t| t.env);

    Ok((command_str, env_json))
}

fn resolve_open_path(
    file_path: &Path,
    action: &str,
    protection: ProtectionLevel,
) -> Result<(Option<std::path::PathBuf>, std::path::PathBuf)> {
    match (action, protection) {
        ("view", ProtectionLevel::Immutable | ProtectionLevel::Protected) => {
            let unique_id = std::process::id();
            let dir_name = format!("mkrk-view-{unique_id}");
            let temp_dir = std::env::temp_dir().join(dir_name);
            std::fs::create_dir_all(&temp_dir)?;

            let temp_path = temp_dir.join(
                file_path
                    .file_name()
                    .unwrap_or_else(|| std::ffi::OsStr::new("file")),
            );

            if temp_path.symlink_metadata().is_ok() {
                std::fs::remove_file(&temp_path)?;
            }
            std::fs::copy(file_path, &temp_path)?;

            Ok((Some(temp_dir), temp_path))
        }
        _ => Ok((None, file_path.to_path_buf())),
    }
}
