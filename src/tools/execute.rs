use std::path::Path;
use std::process::Command;

use anyhow::{bail, Result};

use crate::db::{ProjectDb, WorkspaceDb};

use super::resolve::{build_scope_chain, resolve_tool, ToolLookup};
use super::{apply_env, build_tool_env};

pub struct ExecuteToolParams<'a> {
    pub tool_name: &'a str,
    pub file_abs_path: Option<&'a Path>,
    pub file_rel_path: Option<&'a str>,
    pub file_ext: Option<&'a str>,
    pub tags: &'a [String],
    pub project_root: &'a Path,
    pub project_db: &'a ProjectDb,
    pub workspace_root: Option<&'a Path>,
    pub workspace_db: Option<&'a WorkspaceDb>,
}

struct ResolvedTool {
    command: String,
    env: Option<String>,
    quiet: bool,
}

fn resolve_execution_tool(params: &ExecuteToolParams<'_>) -> Result<ResolvedTool> {
    let scope_chain = params
        .file_rel_path
        .map(build_scope_chain)
        .unwrap_or_default();
    let scope_refs: Vec<Option<&str>> = scope_chain.iter().map(|s| s.as_deref()).collect();

    let lookup = ToolLookup {
        action: params.tool_name,
        file_type: params.file_ext.unwrap_or("*"),
        scope_chain: &scope_refs,
        tags: params.tags,
    };

    let candidate = resolve_tool(&lookup, params.project_db, params.workspace_db)?;

    if let Some(c) = candidate {
        return Ok(ResolvedTool {
            command: c.command,
            env: c.env,
            quiet: c.quiet,
        });
    }
    bail!(
        "no tool '{}' found{}",
        params.tool_name,
        params
            .file_rel_path
            .map_or(String::new(), |p| format!(" for file '{p}'"))
    );
}

pub fn execute_tool(params: &ExecuteToolParams<'_>) -> Result<()> {
    let resolved = resolve_execution_tool(params)?;
    let env_map = build_tool_env(resolved.env.as_deref(), &resolved.command, resolved.quiet)?;

    let mut cmd = Command::new(&resolved.command);
    if let Some(abs_path) = params.file_abs_path {
        cmd.arg(abs_path.to_string_lossy().as_ref());
    }
    apply_env(&mut cmd, &env_map);
    cmd.env("MKRK_PROJECT_ROOT", params.project_root);
    cmd.env("MKRK_PROJECT_DB", params.project_root.join(".mkrk"));
    if let Some(ws_root) = params.workspace_root {
        cmd.env("MKRK_WORKSPACE_ROOT", ws_root);
    }
    if let Some(rel_path) = params.file_rel_path {
        cmd.env("MKRK_FILE_REL_PATH", rel_path);
    }
    if let Some(abs_path) = params.file_abs_path {
        cmd.env("MKRK_FILE_ABS_PATH", abs_path);
    }
    if let Some(ext) = params.file_ext {
        cmd.env("MKRK_FILE_EXT", ext);
    }

    let status = cmd.status()?;
    if !status.success() {
        bail!("tool '{}' exited with {status}", params.tool_name,);
    }

    Ok(())
}
