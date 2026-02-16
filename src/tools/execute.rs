use std::path::Path;
use std::process::Command;

use anyhow::{bail, Result};

use crate::db::{ProjectDb, WorkspaceDb};

use super::resolve::{build_scope_chain, resolve_tool, ToolLookup};
use super::{apply_env, build_tool_env};

pub struct ExecuteToolParams<'a> {
    pub tool_name: &'a str,
    pub file_abs_path: &'a Path,
    pub file_rel_path: &'a str,
    pub file_ext: &'a str,
    pub tags: &'a [String],
    pub project_root: &'a Path,
    pub project_db: &'a ProjectDb,
    pub workspace_root: Option<&'a Path>,
    pub workspace_db: Option<&'a WorkspaceDb>,
}

pub fn execute_tool(params: &ExecuteToolParams<'_>) -> Result<()> {
    let scope_chain = build_scope_chain(params.file_rel_path);
    let scope_refs: Vec<Option<&str>> = scope_chain.iter().map(|s| s.as_deref()).collect();

    let lookup = ToolLookup {
        action: params.tool_name,
        file_type: params.file_ext,
        scope_chain: &scope_refs,
        tags: params.tags,
    };

    let candidate = resolve_tool(&lookup, params.project_db, params.workspace_db)?;
    let Some(candidate) = candidate else {
        bail!("no tool '{}' found for file '{}'", params.tool_name, params.file_rel_path);
    };

    let env_map = build_tool_env(candidate.env.as_deref(), &candidate.command, candidate.quiet)?;

    let file_path_str = params.file_abs_path.to_string_lossy();
    let mut cmd = Command::new(&candidate.command);
    cmd.arg(file_path_str.as_ref());
    apply_env(&mut cmd, &env_map);
    cmd.env("MKRK_PROJECT_ROOT", params.project_root);
    cmd.env("MKRK_PROJECT_DB", params.project_root.join(".mkrk"));
    if let Some(ws_root) = params.workspace_root {
        cmd.env("MKRK_WORKSPACE_ROOT", ws_root);
    }

    let status = cmd.status()?;
    if !status.success() {
        bail!(
            "tool '{}' exited with {status}",
            params.tool_name,
        );
    }

    Ok(())
}
