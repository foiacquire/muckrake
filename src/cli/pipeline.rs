use std::path::Path;

use anyhow::{bail, Result};
use console::style;

use crate::context::discover;
use crate::models::{AttachmentScope, Pipeline};

pub fn run_add(
    cwd: &Path,
    name: &str,
    states_str: &str,
    transitions_json: Option<&str>,
) -> Result<()> {
    let ctx = discover(cwd)?;
    let (_, project_db) = ctx.require_project()?;

    if project_db.get_pipeline_by_name(name)?.is_some() {
        bail!("pipeline '{name}' already exists");
    }

    let states: Vec<String> = states_str
        .split(',')
        .map(|s| s.trim().to_string())
        .collect();
    let transitions = match transitions_json {
        Some(json) => serde_json::from_str(json)
            .map_err(|e| anyhow::anyhow!("invalid transitions JSON: {e}"))?,
        None => Pipeline::default_transitions(&states),
    };

    let pipeline = Pipeline {
        id: None,
        name: name.to_string(),
        states,
        transitions,
    };
    pipeline.validate()?;

    project_db.insert_pipeline(&pipeline)?;
    eprintln!(
        "Added pipeline '{}' ({})",
        name,
        pipeline.states.join(" -> ")
    );

    Ok(())
}

pub fn run_list(cwd: &Path) -> Result<()> {
    let ctx = discover(cwd)?;
    let (_, project_db) = ctx.require_project()?;

    let pipelines = project_db.list_pipelines()?;
    if pipelines.is_empty() {
        eprintln!("No pipelines configured");
        return Ok(());
    }

    for pipeline in &pipelines {
        let states_display = pipeline.states.join(" -> ");
        println!(
            "  {} {}",
            style(&pipeline.name).bold(),
            style(states_display).dim()
        );

        let pid = pipeline.id.unwrap();
        let attachments = project_db.list_attachments_for_pipeline(pid)?;
        for att in &attachments {
            println!(
                "    {} {}",
                style(att.scope_type.to_string()).cyan(),
                att.scope_value
            );
        }
    }

    Ok(())
}

pub fn run_remove(cwd: &Path, name: &str) -> Result<()> {
    let ctx = discover(cwd)?;
    let (_, project_db) = ctx.require_project()?;

    let count = project_db.remove_pipeline(name)?;
    if count == 0 {
        bail!("no pipeline named '{name}'");
    }
    eprintln!("Removed pipeline '{name}'");

    Ok(())
}

pub fn run_attach(
    cwd: &Path,
    pipeline_name: &str,
    category: Option<&str>,
    tag: Option<&str>,
) -> Result<()> {
    let ctx = discover(cwd)?;
    let (_, project_db) = ctx.require_project()?;

    let (scope_type, scope_value) = resolve_scope_args(category, tag)?;

    let pipeline = project_db
        .get_pipeline_by_name(pipeline_name)?
        .ok_or_else(|| anyhow::anyhow!("pipeline '{pipeline_name}' not found"))?;

    project_db.attach_pipeline(pipeline.id.unwrap(), scope_type, scope_value)?;
    eprintln!("Attached pipeline '{pipeline_name}' to {scope_type} '{scope_value}'");

    Ok(())
}

pub fn run_detach(
    cwd: &Path,
    pipeline_name: &str,
    category: Option<&str>,
    tag: Option<&str>,
) -> Result<()> {
    let ctx = discover(cwd)?;
    let (_, project_db) = ctx.require_project()?;

    let (scope_type, scope_value) = resolve_scope_args(category, tag)?;

    let pipeline = project_db
        .get_pipeline_by_name(pipeline_name)?
        .ok_or_else(|| anyhow::anyhow!("pipeline '{pipeline_name}' not found"))?;

    let count = project_db.detach_pipeline(pipeline.id.unwrap(), scope_type, scope_value)?;
    if count == 0 {
        bail!("pipeline '{pipeline_name}' is not attached to {scope_type} '{scope_value}'");
    }
    eprintln!("Detached pipeline '{pipeline_name}' from {scope_type} '{scope_value}'");

    Ok(())
}

fn resolve_scope_args<'a>(
    category: Option<&'a str>,
    tag: Option<&'a str>,
) -> Result<(AttachmentScope, &'a str)> {
    match (category, tag) {
        (Some(cat), None) => Ok((AttachmentScope::Category, cat)),
        (None, Some(t)) => Ok((AttachmentScope::Tag, t)),
        (Some(_), Some(_)) => bail!("specify either --category or --tag, not both"),
        (None, None) => bail!("specify --category or --tag"),
    }
}
