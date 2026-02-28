use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Result};

use crate::context::{discover, Context};
use crate::db::{ProjectDb, WorkspaceDb};
use crate::models::CategoryType;
use crate::reference::{parse_reference, resolve_references};
use crate::tools;
use crate::util::whoami;

struct ToolRef {
    project: Option<String>,
    name: String,
}

fn parse_tool_ref(input: &str) -> Result<ToolRef> {
    if !input.starts_with(':') {
        return Ok(ToolRef {
            project: None,
            name: input.to_string(),
        });
    }
    let body = &input[1..];
    if body.is_empty() {
        bail!("empty tool reference");
    }
    if let Some((project, name)) = body.split_once('.') {
        if project.is_empty() || name.is_empty() {
            bail!("invalid tool reference '{input}'");
        }
        Ok(ToolRef {
            project: Some(project.to_string()),
            name: name.to_string(),
        })
    } else {
        Ok(ToolRef {
            project: None,
            name: body.to_string(),
        })
    }
}

pub fn run(cwd: &Path, name: &str, args: &[String]) -> Result<()> {
    let tool_ref = parse_tool_ref(name)?;

    let ctx = discover(cwd)?;

    match &ctx {
        Context::Workspace { .. } if tool_ref.project.is_some() => {
            run_cross_project_tool(&ctx, &tool_ref, args)
        }
        Context::Workspace { .. } => run_workspace_tool(&ctx, &tool_ref, args),
        Context::Project { .. } => run_project_tool(&ctx, &tool_ref, args),
        Context::None => bail!("no project or workspace found"),
    }
}

fn run_project_tool(ctx: &Context, tool_ref: &ToolRef, args: &[String]) -> Result<()> {
    let (project_root, project_db, workspace_db) = ctx.require_project_with_workspace()?;
    let (file_paths, resolved_files) = resolve_file_refs(args, project_root, ctx)?;

    let (command_str, env_json, quiet) = if let Some(ref proj_name) = tool_ref.project {
        find_cross_project_tool(ctx, proj_name, &tool_ref.name, &resolved_files)?
    } else {
        find_local_tool(
            &tool_ref.name,
            &resolved_files,
            project_root,
            project_db,
            workspace_db,
        )?
    };

    let env_map = tools::build_tool_env(env_json.as_deref(), &command_str, quiet)?;
    let status = build_and_run_command(&command_str, &file_paths, &env_map, project_root, ctx)?;

    let user = whoami();
    let detail = serde_json::json!({
        "tool": tool_ref.name,
        "files": file_paths,
    });
    project_db.insert_audit("tool", None, Some(&user), Some(&detail.to_string()))?;

    if !status.success() {
        bail!("tool '{}' exited with {status}", tool_ref.name);
    }

    Ok(())
}

struct WsToolInvocation<'a> {
    tool_name: &'a str,
    command: String,
    env_json: Option<String>,
    quiet: bool,
    ws_root: &'a Path,
}

impl WsToolInvocation<'_> {
    fn run(&self, file_args: &[String]) -> Result<()> {
        let env_map = tools::build_tool_env(self.env_json.as_deref(), &self.command, self.quiet)?;
        let mut cmd = std::process::Command::new(&self.command);
        cmd.args(file_args);
        tools::apply_env(&mut cmd, &env_map);
        cmd.env("MKRK_WORKSPACE_ROOT", self.ws_root);
        let status = cmd.status()?;
        if !status.success() {
            bail!("tool '{}' exited with {status}", self.tool_name);
        }
        Ok(())
    }
}

fn run_cross_project_tool(ctx: &Context, tool_ref: &ToolRef, args: &[String]) -> Result<()> {
    let proj_name = tool_ref
        .project
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("cross-project tool requires project name"))?;
    let (command_str, env_json, quiet) =
        find_cross_project_tool(ctx, proj_name, &tool_ref.name, &[])?;
    let (ws_root, _) = require_workspace(ctx)?;
    let invocation = WsToolInvocation {
        tool_name: &tool_ref.name,
        command: command_str,
        env_json,
        quiet,
        ws_root,
    };
    invocation.run(args)
}

fn run_workspace_tool(ctx: &Context, tool_ref: &ToolRef, args: &[String]) -> Result<()> {
    let (ws_root, ws_db) = require_workspace(ctx)?;

    if !args.is_empty() {
        bail!("file references not supported from workspace root (use :project.tool or cd into a project)");
    }

    let projects = ws_db.list_projects()?;
    let mut found: Vec<(String, String, Option<String>, bool)> = Vec::new();

    for proj in &projects {
        let proj_root = ws_root.join(&proj.path);
        let proj_mkrk = proj_root.join(".mkrk");
        if !proj_mkrk.exists() {
            continue;
        }
        let proj_db = ProjectDb::open(&proj_mkrk)?;

        if let Ok(result) = find_local_tool(&tool_ref.name, &[], &proj_root, &proj_db, Some(ws_db))
        {
            found.push((proj.name.clone(), result.0, result.1, result.2));
        }
    }

    if found.is_empty() {
        bail!("tool '{}' not found in any project", tool_ref.name);
    }
    if found.len() > 1 {
        let names: Vec<&str> = found.iter().map(|(n, ..)| n.as_str()).collect();
        bail!(
            "tool '{}' found in multiple projects: {}. Use :project.{} to disambiguate",
            tool_ref.name,
            names.join(", "),
            tool_ref.name,
        );
    }

    let (_, command_str, env_json, quiet) = found.into_iter().next().unwrap();
    let invocation = WsToolInvocation {
        tool_name: &tool_ref.name,
        command: command_str,
        env_json,
        quiet,
        ws_root,
    };
    invocation.run(&[])
}

fn require_workspace(ctx: &Context) -> Result<(&Path, &WorkspaceDb)> {
    match ctx {
        Context::Workspace {
            workspace_root,
            workspace_db,
        } => Ok((workspace_root, workspace_db)),
        Context::Project {
            workspace: Some(ws),
            ..
        } => Ok((&ws.workspace_root, &ws.workspace_db)),
        _ => bail!("workspace context required"),
    }
}

fn find_local_tool(
    tool_name: &str,
    resolved_files: &[ResolvedFileInfo],
    project_root: &Path,
    project_db: &ProjectDb,
    workspace_db: Option<&WorkspaceDb>,
) -> Result<(String, Option<String>, bool)> {
    let candidate = resolve_db_tool(tool_name, resolved_files, project_db, workspace_db)?;
    if let Some(c) = candidate {
        return Ok((c.command, c.env, c.quiet));
    }
    let tool_path = discover_tool(project_root, project_db, tool_name)?;
    Ok((tool_path.to_string_lossy().to_string(), None, true))
}

fn find_cross_project_tool(
    ctx: &Context,
    project_name: &str,
    tool_name: &str,
    resolved_files: &[ResolvedFileInfo],
) -> Result<(String, Option<String>, bool)> {
    let (ws_root, ws_db) = match ctx {
        Context::Project {
            workspace: Some(ws),
            ..
        } => (&ws.workspace_root, &ws.workspace_db),
        Context::Workspace {
            workspace_root,
            workspace_db,
        } => (workspace_root, workspace_db),
        _ => bail!("cross-project tool reference requires workspace context"),
    };

    let project = ws_db
        .get_project_by_name(project_name)?
        .ok_or_else(|| anyhow::anyhow!("project '{project_name}' not found in workspace"))?;
    let proj_root = ws_root.join(&project.path);
    let proj_db = ProjectDb::open(&proj_root.join(".mkrk"))?;

    find_local_tool(tool_name, resolved_files, &proj_root, &proj_db, Some(ws_db))
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

fn tool_patterns(project_root: &Path, project_db: &ProjectDb) -> Vec<String> {
    let Ok(categories) = project_db.list_categories() else {
        return vec![];
    };
    let mut patterns = Vec::new();
    for cat in categories
        .iter()
        .filter(|c| c.category_type == CategoryType::Tools)
    {
        let root = project_root.display();
        // Category patterns use `**` for recursive matching (e.g., `tools/**`).
        // The glob crate's `**` only matches subdirectory contents, not direct
        // children. Emit both `tools/*` and `tools/**/*` to cover all depths.
        if cat.pattern.ends_with("/**") {
            let base = &cat.pattern[..cat.pattern.len() - 3];
            patterns.push(format!("{root}/{base}/*"));
            patterns.push(format!("{root}/{base}/**/*"));
        } else {
            patterns.push(format!("{root}/{}", cat.pattern));
        }
    }
    patterns
}

fn iter_tool_files(project_root: &Path, project_db: &ProjectDb) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for pattern in tool_patterns(project_root, project_db) {
        let Ok(entries) = glob::glob(&pattern) else {
            continue;
        };
        for entry in entries.flatten() {
            if entry.is_file() {
                files.push(entry);
            }
        }
    }
    files
}

fn discover_tool(project_root: &Path, project_db: &ProjectDb, name: &str) -> Result<PathBuf> {
    if name.contains('/') || name.contains("..") {
        bail!("tool name '{name}' contains invalid path characters");
    }
    iter_tool_files(project_root, project_db)
        .into_iter()
        .find(|entry| entry.file_stem().map(|s| s.to_string_lossy()).as_deref() == Some(name))
        .ok_or_else(|| anyhow::anyhow!("tool '{name}' not found"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Category;
    use tempfile::TempDir;

    fn setup_project_with_tools(dir: &Path) -> (PathBuf, ProjectDb) {
        let tools_dir = dir.join("tools");
        std::fs::create_dir_all(&tools_dir).unwrap();
        let db = ProjectDb::create(&dir.join(".mkrk")).unwrap();
        db.insert_category(&Category {
            id: None,
            name: "tools".to_string(),
            pattern: "tools/**".to_string(),
            category_type: CategoryType::Tools,
            description: None,
        })
        .unwrap();
        (tools_dir, db)
    }

    #[test]
    fn discover_tool_exact() {
        let dir = TempDir::new().unwrap();
        let (tools_dir, db) = setup_project_with_tools(dir.path());
        let tool = tools_dir.join("ner");
        std::fs::write(&tool, "#!/bin/sh\necho hi").unwrap();

        let found = discover_tool(dir.path(), &db, "ner").unwrap();
        assert_eq!(found, tool);
    }

    #[test]
    fn discover_tool_with_extension() {
        let dir = TempDir::new().unwrap();
        let (tools_dir, db) = setup_project_with_tools(dir.path());
        let tool = tools_dir.join("ner.py");
        std::fs::write(&tool, "print('hi')").unwrap();

        let found = discover_tool(dir.path(), &db, "ner").unwrap();
        assert_eq!(found, tool);
    }

    #[test]
    fn discover_tool_not_found() {
        let dir = TempDir::new().unwrap();
        let (_, db) = setup_project_with_tools(dir.path());

        let result = discover_tool(dir.path(), &db, "missing");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("not found"), "unexpected error: {err}");
    }

    #[test]
    fn discover_tool_rejects_path_traversal() {
        let dir = TempDir::new().unwrap();
        let (_, db) = setup_project_with_tools(dir.path());

        let result = discover_tool(dir.path(), &db, "../evil");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("invalid path"), "unexpected error: {err}");

        let result = discover_tool(dir.path(), &db, "sub/tool");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("invalid path"), "unexpected error: {err}");
    }

    #[test]
    fn discover_tool_custom_category() {
        let dir = TempDir::new().unwrap();
        let scripts_dir = dir.path().join("scripts");
        std::fs::create_dir_all(&scripts_dir).unwrap();
        let db = ProjectDb::create(&dir.path().join(".mkrk")).unwrap();
        db.insert_category(&Category {
            id: None,
            name: "scripts".to_string(),
            pattern: "scripts/**".to_string(),
            category_type: CategoryType::Tools,
            description: None,
        })
        .unwrap();
        let tool = scripts_dir.join("ocr.sh");
        std::fs::write(&tool, "#!/bin/sh").unwrap();

        let found = discover_tool(dir.path(), &db, "ocr").unwrap();
        assert_eq!(found, tool);
    }

    #[test]
    fn parse_tool_ref_plain_name() {
        let r = parse_tool_ref("ner").unwrap();
        assert!(r.project.is_none());
        assert_eq!(r.name, "ner");
    }

    #[test]
    fn parse_tool_ref_with_project() {
        let r = parse_tool_ref(":bailey.ner").unwrap();
        assert_eq!(r.project.as_deref(), Some("bailey"));
        assert_eq!(r.name, "ner");
    }

    #[test]
    fn parse_tool_ref_bare_colon_name() {
        let r = parse_tool_ref(":ner").unwrap();
        assert!(r.project.is_none());
        assert_eq!(r.name, "ner");
    }

    #[test]
    fn parse_tool_ref_empty() {
        assert!(parse_tool_ref(":").is_err());
    }

    #[test]
    fn parse_tool_ref_invalid_trailing_dot() {
        assert!(parse_tool_ref(":bailey.").is_err());
    }
}
