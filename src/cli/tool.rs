use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Result};
use console::style;

use crate::context::{discover, Context};
use crate::db::{ProjectDb, WorkspaceDb};
use crate::reference::{parse_reference, resolve_references};
use crate::tools;
use crate::util::whoami;

pub fn run(cwd: &Path, args: &[String]) -> Result<()> {
    if args.is_empty() {
        bail!("tool name required: mkrk tool <name> [references...]");
    }

    let tool_name = &args[0];
    let raw_refs = &args[1..];

    let ctx = discover(cwd)?;
    let (project_root, project_db, workspace_db) = ctx.require_project_with_workspace()?;

    let (file_paths, resolved_files) = resolve_file_refs(raw_refs, project_root, &ctx)?;

    let candidate = resolve_db_tool(tool_name, &resolved_files, project_db, workspace_db)?;

    let (command_str, env_json, quiet) = if let Some(c) = candidate {
        (c.command, c.env, c.quiet)
    } else {
        let tool_path = discover_tool(project_root, tool_name)?;
        (tool_path.to_string_lossy().to_string(), None, true)
    };

    let env_map = tools::build_tool_env(env_json.as_deref(), &command_str);

    if !quiet {
        eprintln!("{} {}", style(">").dim(), command_str);
    }

    let status = build_and_run_command(&command_str, &file_paths, &env_map, project_root, &ctx)?;

    let user = whoami();
    let detail = serde_json::json!({
        "tool": tool_name,
        "files": file_paths,
    });
    project_db.insert_audit("tool", None, Some(&user), Some(&detail.to_string()))?;

    if !status.success() {
        bail!("tool '{tool_name}' exited with {status}");
    }

    Ok(())
}

fn build_and_run_command(
    command_str: &str,
    file_paths: &[String],
    env_map: &std::collections::HashMap<String, Option<String>>,
    project_root: &Path,
    ctx: &Context,
) -> Result<std::process::ExitStatus> {
    let mut cmd = Command::new(command_str);
    cmd.args(file_paths);
    tools::apply_env(&mut cmd, env_map);
    cmd.env("MKRK_PROJECT_ROOT", project_root);
    cmd.env("MKRK_PROJECT_DB", project_root.join(".mkrk"));
    if let Context::Project {
        workspace: Some(ws),
        ..
    } = ctx
    {
        cmd.env("MKRK_WORKSPACE_ROOT", &ws.workspace_root);
    }
    cmd.status().map_err(Into::into)
}

fn resolve_db_tool(
    tool_name: &str,
    resolved_files: &[ResolvedFileInfo],
    project_db: &ProjectDb,
    workspace_db: Option<&WorkspaceDb>,
) -> Result<Option<tools::ToolCandidate>> {
    if let Some(first) = resolved_files.first() {
        let scope_chain = tools::build_scope_chain(&first.path);
        let scope_refs: Vec<Option<&str>> = scope_chain.iter().map(|s| s.as_deref()).collect();

        let lookup = tools::ToolLookup {
            action: tool_name,
            file_type: &first.file_ext,
            scope_chain: &scope_refs,
            tags: &first.tags,
        };
        return tools::resolve_tool(&lookup, project_db, workspace_db);
    }

    let scope_chain: Vec<Option<String>> = vec![None];
    let scope_refs: Vec<Option<&str>> = scope_chain.iter().map(|s| s.as_deref()).collect();
    let empty_tags: Vec<String> = Vec::new();
    let lookup = tools::ToolLookup {
        action: tool_name,
        file_type: "*",
        scope_chain: &scope_refs,
        tags: &empty_tags,
    };
    tools::resolve_tool(&lookup, project_db, workspace_db)
}

struct ResolvedFileInfo {
    path: String,
    file_ext: String,
    tags: Vec<String>,
}

fn resolve_file_refs(
    raw_refs: &[String],
    project_root: &Path,
    ctx: &Context,
) -> Result<(Vec<String>, Vec<ResolvedFileInfo>)> {
    if raw_refs.is_empty() {
        return Ok((vec![], vec![]));
    }

    let Context::Project { project_db, .. } = ctx else {
        bail!("must be inside a project");
    };

    let parsed: Vec<_> = raw_refs
        .iter()
        .map(|r| parse_reference(r))
        .collect::<Result<Vec<_>>>()?;
    let collection = resolve_references(&parsed, ctx)?;

    let mut file_paths = Vec::new();
    let mut infos = Vec::new();

    for rf in &collection.files {
        let abs_path = project_root
            .join(&rf.file.path)
            .to_string_lossy()
            .to_string();
        file_paths.push(abs_path);

        let ext = rf.file.name.rsplit('.').next().unwrap_or("*").to_string();
        let tags = rf
            .file
            .id
            .map(|id| project_db.get_tags(id))
            .transpose()?
            .unwrap_or_default();

        infos.push(ResolvedFileInfo {
            path: rf.file.path.clone(),
            file_ext: ext,
            tags,
        });
    }

    Ok((file_paths, infos))
}

pub struct AddToolParams<'a> {
    pub name: &'a str,
    pub command: &'a str,
    pub scope: Option<&'a str>,
    pub file_type: &'a str,
    pub tag: Option<&'a str>,
    pub env: Option<&'a str>,
    pub verbose: bool,
}

pub fn run_add(cwd: &Path, params: &AddToolParams<'_>) -> Result<()> {
    let AddToolParams {
        name,
        command,
        scope,
        file_type,
        tag,
        env,
        verbose,
    } = params;
    let ctx = discover(cwd)?;
    let (_, project_db) = ctx.require_project()?;

    if let Some(env_json) = *env {
        tools::confirm_privacy_removal(command, env_json)?;
    }

    eprintln!(
        "{} mkrk sets proxy environment variables by default, but cannot guarantee",
        style("⚠").yellow(),
    );
    eprintln!("  that \"{command}\" respects them. Verify this tool does not leak");
    eprintln!("  identifying information before use.");

    let quiet = !verbose;

    if let Some(tag_name) = *tag {
        let params = crate::db::TagToolConfigParams {
            tag: tag_name,
            action: name,
            file_type,
            command,
            env: *env,
            quiet,
        };
        project_db.insert_tag_tool_config(&params)?;
        eprintln!("Registered tool '{name}' for tag '{tag_name}' (file_type={file_type})");
    } else {
        let params = crate::db::ToolConfigParams {
            scope: *scope,
            action: name,
            file_type,
            command,
            env: *env,
            quiet,
        };
        project_db.insert_tool_config(&params)?;
        let scope_label = scope.unwrap_or("default");
        eprintln!("Registered tool '{name}' at scope '{scope_label}' (file_type={file_type})");
    }

    Ok(())
}

fn print_tool_configs(configs: &[crate::db::ToolConfigRow], label: &str) -> bool {
    if configs.is_empty() {
        return false;
    }
    eprintln!("{label} tool configs:");
    for c in configs {
        let scope = c.scope.as_deref().unwrap_or("(default)");
        let quiet_label = if c.quiet { "" } else { " [verbose]" };
        eprintln!(
            "  {:<12} scope={:<20} type={:<8} cmd={}{}",
            c.action, scope, c.file_type, c.command, quiet_label
        );
    }
    true
}

fn print_tag_tool_configs(configs: &[crate::db::TagToolConfigRow], label: &str) -> bool {
    if configs.is_empty() {
        return false;
    }
    eprintln!("{label} tag tool configs:");
    for c in configs {
        let quiet_label = if c.quiet { "" } else { " [verbose]" };
        eprintln!(
            "  {:<12} tag={:<20} type={:<8} cmd={}{}",
            c.action, c.tag, c.file_type, c.command, quiet_label
        );
    }
    true
}

pub fn run_list(cwd: &Path) -> Result<()> {
    let ctx = discover(cwd)?;
    let (_, project_db, workspace_db) = ctx.require_project_with_workspace()?;

    let mut found = false;
    found |= print_tool_configs(&project_db.list_tool_configs()?, "Project");
    found |= print_tag_tool_configs(&project_db.list_tag_tool_configs()?, "Project");
    if let Some(ws_db) = workspace_db {
        found |= print_tool_configs(&ws_db.list_tool_configs()?, "Workspace");
        found |= print_tag_tool_configs(&ws_db.list_tag_tool_configs()?, "Workspace");
    }
    if !found {
        eprintln!("No tool configurations registered.");
    }

    Ok(())
}

pub fn run_remove(
    cwd: &Path,
    name: &str,
    scope: Option<&str>,
    file_type: Option<&str>,
    tag: Option<&str>,
) -> Result<()> {
    let ctx = discover(cwd)?;
    let (_, project_db) = ctx.require_project()?;

    let count = if let Some(tag_name) = tag {
        project_db.remove_tag_tool_config(name, tag_name, file_type)?
    } else {
        project_db.remove_tool_config(name, scope, file_type)?
    };

    if count == 0 {
        eprintln!("No matching tool configuration found.");
    } else {
        eprintln!("Removed {count} tool configuration(s).");
    }

    Ok(())
}

fn discover_tool(project_root: &Path, name: &str) -> Result<PathBuf> {
    if name.contains('/') || name.contains("..") {
        bail!("tool name '{name}' contains invalid path characters");
    }

    let tools_dir = project_root.join("tools");
    let exact = tools_dir.join(name);
    if exact.is_file() {
        return Ok(exact);
    }

    let pattern = format!("{}/{}.*", tools_dir.display(), name);
    let mut matches = glob::glob(&pattern)?;
    if let Some(entry) = matches.next() {
        let path = entry?;
        if path.is_file() {
            return Ok(path);
        }
    }

    bail!(
        "tool '{}' not found in database or {}",
        name,
        tools_dir.display()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_tools_dir(dir: &Path) -> PathBuf {
        let tools_dir = dir.join("tools");
        std::fs::create_dir_all(&tools_dir).unwrap();
        tools_dir
    }

    #[test]
    fn discover_tool_exact() {
        let dir = TempDir::new().unwrap();
        let tools_dir = setup_tools_dir(dir.path());
        let tool = tools_dir.join("ner");
        std::fs::write(&tool, "#!/bin/sh\necho hi").unwrap();

        let found = discover_tool(dir.path(), "ner").unwrap();
        assert_eq!(found, tool);
    }

    #[test]
    fn discover_tool_with_extension() {
        let dir = TempDir::new().unwrap();
        let tools_dir = setup_tools_dir(dir.path());
        let tool = tools_dir.join("ner.py");
        std::fs::write(&tool, "print('hi')").unwrap();

        let found = discover_tool(dir.path(), "ner").unwrap();
        assert_eq!(found, tool);
    }

    #[test]
    fn discover_tool_not_found() {
        let dir = TempDir::new().unwrap();
        setup_tools_dir(dir.path());

        let result = discover_tool(dir.path(), "missing");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("not found"), "unexpected error: {err}");
    }

    #[test]
    fn discover_tool_rejects_path_traversal() {
        let dir = TempDir::new().unwrap();
        setup_tools_dir(dir.path());

        let result = discover_tool(dir.path(), "../evil");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("invalid path"), "unexpected error: {err}");

        let result = discover_tool(dir.path(), "sub/tool");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("invalid path"), "unexpected error: {err}");
    }
}
