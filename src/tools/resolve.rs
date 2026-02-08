use anyhow::Result;
use dialoguer::Select;

use crate::db::{ProjectDb, TagToolConfigRow, ToolConfigRow, WorkspaceDb};

#[derive(Debug, Clone)]
pub struct ToolCandidate {
    pub label: String,
    pub command: String,
    pub env: Option<String>,
}

pub struct ToolLookup<'a> {
    pub action: &'a str,
    pub file_type: &'a str,
    pub scope_chain: &'a [Option<&'a str>],
    pub tags: &'a [String],
}

pub fn resolve_tool(
    lookup: &ToolLookup,
    project_db: &ProjectDb,
    workspace_db: Option<&WorkspaceDb>,
) -> Result<Option<ToolCandidate>> {
    let mut candidates = Vec::new();

    let category_candidate = resolve_category_candidate(
        lookup.action,
        lookup.file_type,
        lookup.scope_chain,
        project_db,
        workspace_db,
    )?;
    if let Some(c) = category_candidate {
        candidates.push(c);
    }

    let tag_candidates = resolve_tag_candidates(
        lookup.action,
        lookup.file_type,
        lookup.tags,
        project_db,
        workspace_db,
    )?;
    candidates.extend(tag_candidates);

    match candidates.len() {
        0 => Ok(None),
        1 => Ok(candidates.into_iter().next()),
        _ => prompt_selection(&candidates),
    }
}

fn resolve_category_candidate(
    action: &str,
    file_type: &str,
    scope_chain: &[Option<&str>],
    project_db: &ProjectDb,
    workspace_db: Option<&WorkspaceDb>,
) -> Result<Option<ToolCandidate>> {
    for scope in scope_chain {
        if let Some(row) = project_db.get_tool_config(*scope, action, file_type)? {
            return Ok(Some(tool_config_to_candidate(&row, "Category")));
        }
    }

    if let Some(ws_db) = workspace_db {
        for scope in scope_chain {
            if let Some(row) = ws_db.get_tool_config(*scope, action, file_type)? {
                return Ok(Some(tool_config_to_candidate(&row, "Category(workspace)")));
            }
        }
    }

    Ok(None)
}

fn resolve_tag_candidates(
    action: &str,
    file_type: &str,
    tags: &[String],
    project_db: &ProjectDb,
    workspace_db: Option<&WorkspaceDb>,
) -> Result<Vec<ToolCandidate>> {
    let mut result = Vec::new();
    let mut seen_tags: Vec<String> = Vec::new();

    let project_configs = project_db.get_tag_tool_configs(tags, action, file_type)?;
    for row in &project_configs {
        seen_tags.push(row.tag.clone());
        result.push(tag_config_to_candidate(row));
    }

    if let Some(ws_db) = workspace_db {
        let remaining_tags: Vec<String> = tags
            .iter()
            .filter(|t| !seen_tags.contains(t))
            .cloned()
            .collect();
        if !remaining_tags.is_empty() {
            let ws_configs = ws_db.get_tag_tool_configs(&remaining_tags, action, file_type)?;
            for row in &ws_configs {
                result.push(tag_config_to_candidate(row));
            }
        }
    }

    Ok(result)
}

fn tool_config_to_candidate(row: &ToolConfigRow, label_prefix: &str) -> ToolCandidate {
    let label = match &row.scope {
        Some(s) => format!("{label_prefix}:{s}"),
        None => format!("{label_prefix}:default"),
    };
    ToolCandidate {
        label,
        command: row.command.clone(),
        env: row.env.clone(),
    }
}

fn tag_config_to_candidate(row: &TagToolConfigRow) -> ToolCandidate {
    ToolCandidate {
        label: row.tag.clone(),
        command: row.command.clone(),
        env: row.env.clone(),
    }
}

fn prompt_selection(candidates: &[ToolCandidate]) -> Result<Option<ToolCandidate>> {
    let labels: Vec<&str> = candidates.iter().map(|c| c.label.as_str()).collect();
    let selection = Select::new()
        .with_prompt("Multiple tools match. Which one?")
        .items(&labels)
        .default(0)
        .interact_opt()?;

    match selection {
        Some(idx) => Ok(Some(candidates[idx].clone())),
        None => Ok(None),
    }
}

pub fn default_tool(action: &str) -> String {
    match action {
        "view" => std::env::var("PAGER").unwrap_or_else(|_| "less".to_string()),
        "edit" => std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string()),
        _ => std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string()),
    }
}
