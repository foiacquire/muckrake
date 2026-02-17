use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Result};
use console::style;

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

pub fn run(cwd: &Path, args: &[String]) -> Result<()> {
    let tool_ref = parse_tool_ref(&args[0])?;
    let raw_refs = &args[1..];

    let ctx = discover(cwd)?;
    let (project_root, project_db, workspace_db) = ctx.require_project_with_workspace()?;

    let (file_paths, resolved_files) = resolve_file_refs(raw_refs, project_root, &ctx)?;

    let (command_str, env_json, quiet) = if let Some(ref proj_name) = tool_ref.project {
        find_cross_project_tool(&ctx, proj_name, &tool_ref.name, &resolved_files)?
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

    let status = build_and_run_command(&command_str, &file_paths, &env_map, project_root, &ctx)?;

    let user = whoami();
    let detail = serde_json::json!({
        "tool": args[0],
        "files": file_paths,
    });
    project_db.insert_audit("tool", None, Some(&user), Some(&detail.to_string()))?;

    if !status.success() {
        bail!("tool '{}' exited with {status}", tool_ref.name);
    }

    Ok(())
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

pub fn run_list(cwd: &Path) -> Result<()> {
    let ctx = discover(cwd)?;
    let mut entries: Vec<(String, Option<String>)> = Vec::new();

    match &ctx {
        Context::Project {
            project_root,
            project_db,
            workspace,
        } => {
            if let Some(ws) = workspace {
                collect_workspace_entries(&ws.workspace_root, &ws.workspace_db, &mut entries)?;
            } else {
                collect_project_entries(project_root, project_db, None, &mut entries)?;
            }
        }
        Context::Workspace {
            workspace_root,
            workspace_db,
        } => collect_workspace_entries(workspace_root, workspace_db, &mut entries)?,
        Context::None => bail!("no project or workspace found"),
    }

    if entries.is_empty() {
        eprintln!("No tools found.");
        return Ok(());
    }

    entries.sort();
    entries.dedup();

    let mut name_counts: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for (name, _) in &entries {
        *name_counts.entry(name.as_str()).or_insert(0) += 1;
    }

    for (name, project) in &entries {
        if name_counts[name.as_str()] > 1 {
            if let Some(proj) = project {
                println!(":{proj}.{name}");
            } else {
                println!("{name}");
            }
        } else {
            println!("{name}");
        }
    }

    Ok(())
}

fn collect_project_entries(
    project_root: &Path,
    project_db: &ProjectDb,
    project_name: Option<&str>,
    entries: &mut Vec<(String, Option<String>)>,
) -> Result<()> {
    let proj = project_name.map(String::from);

    for config in project_db.list_tool_configs()? {
        entries.push((config.action.clone(), proj.clone()));
    }
    for config in project_db.list_tag_tool_configs()? {
        entries.push((config.action.clone(), proj.clone()));
    }
    for (name, _) in discover_all_tools(project_root, project_db) {
        entries.push((name, proj.clone()));
    }

    Ok(())
}

fn collect_workspace_entries(
    workspace_root: &Path,
    workspace_db: &WorkspaceDb,
    entries: &mut Vec<(String, Option<String>)>,
) -> Result<()> {
    for config in workspace_db.list_tool_configs()? {
        entries.push((config.action.clone(), None));
    }
    for config in workspace_db.list_tag_tool_configs()? {
        entries.push((config.action.clone(), None));
    }

    for proj in workspace_db.list_projects()? {
        let proj_root = workspace_root.join(&proj.path);
        let proj_mkrk = proj_root.join(".mkrk");
        let proj_db = ProjectDb::open(&proj_mkrk)?;
        collect_project_entries(&proj_root, &proj_db, Some(&proj.name), entries)?;
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

fn discover_all_tools(project_root: &Path, project_db: &ProjectDb) -> Vec<(String, PathBuf)> {
    let mut tools: Vec<(String, PathBuf)> = Vec::new();
    for entry in iter_tool_files(project_root, project_db) {
        if let Some(name) = entry.file_stem() {
            let name = name.to_string_lossy().to_string();
            if !tools.iter().any(|(n, _)| n == &name) {
                tools.push((name, entry));
            }
        }
    }
    tools.sort_by(|a, b| a.0.cmp(&b.0));
    tools
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
