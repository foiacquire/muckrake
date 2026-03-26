use std::path::Path;

use anyhow::{bail, Result};
use console::style;

use crate::context::discover;
use crate::db::ProjectDb;
use crate::models::scope::CategoryType;
use crate::models::{ProtectionLevel, Scope, ScopeType};
use crate::reference::validate_name;

use super::create_category_dir;

fn find_scope(db: &ProjectDb, input: &str) -> Result<Option<Scope>> {
    if let Some(scope) = db.get_category_by_name(input)? {
        return Ok(Some(scope));
    }
    db.get_category_by_pattern(input)
}

fn require_scope(db: &ProjectDb, name: &str) -> Result<(Scope, i64)> {
    let scope =
        find_scope(db, name)?.ok_or_else(|| anyhow::anyhow!("no category matching '{name}'"))?;
    let scope_id = scope
        .id
        .ok_or_else(|| anyhow::anyhow!("category has no id"))?;
    Ok((scope, scope_id))
}

pub fn run_list(cwd: &Path) -> Result<()> {
    let ctx = discover(cwd)?;
    let (_, project_db) = ctx.require_project()?;

    let categories = project_db.list_categories()?;
    if categories.is_empty() {
        eprintln!("No categories configured");
        return Ok(());
    }

    for cat in &categories {
        let protection = cat
            .id
            .and_then(|id| project_db.get_policy_for_scope(id).ok().flatten())
            .unwrap_or(ProtectionLevel::Editable);

        let cat_type = cat.category_type.unwrap_or_default();
        let type_label = if cat_type == CategoryType::Files {
            String::new()
        } else {
            format!(" [{cat_type}]")
        };

        let pattern_str = cat.pattern.as_deref().unwrap_or("");
        println!(
            "  {} {} {}{}",
            style(&cat.name).bold(),
            style(pattern_str).dim(),
            style(protection).dim(),
            type_label
        );

        if let Some(ref desc) = cat.description {
            println!("    {}", style(desc).dim());
        }
    }

    Ok(())
}

pub struct AddCategoryParams<'a> {
    pub name: &'a str,
    pub pattern: Option<&'a str>,
    pub category_type: &'a str,
    pub protection: &'a str,
    pub description: Option<&'a str>,
}

pub fn run_add(cwd: &Path, params: &AddCategoryParams<'_>) -> Result<()> {
    let ctx = discover(cwd)?;
    let (project_root, project_db) = ctx.require_project()?;

    let name = params.name;
    validate_name(name)?;

    if project_db.get_category_by_name(name)?.is_some() {
        bail!("category '{name}' already exists");
    }

    let resolved_pattern = params
        .pattern
        .map_or_else(|| format!("{name}/**"), String::from);

    let cat_type: CategoryType = params.category_type.parse()?;
    let level: ProtectionLevel = params.protection.parse()?;

    let scope = Scope {
        id: None,
        name: name.to_string(),
        scope_type: ScopeType::Category,
        pattern: Some(resolved_pattern.clone()),
        category_type: Some(cat_type),
        description: params.description.map(String::from),
        created_at: None,
    };

    let scope_id = project_db.insert_scope(&scope)?;
    project_db.insert_scope_policy(scope_id, &level)?;
    create_category_dir(project_root, &resolved_pattern);

    eprintln!("Added category '{name}' ({level})");
    Ok(())
}

pub fn run_update(
    cwd: &Path,
    name: &str,
    new_pattern: Option<&str>,
    protection: Option<&str>,
) -> Result<()> {
    let ctx = discover(cwd)?;
    let (_, project_db) = ctx.require_project()?;
    let (scope, scope_id) = require_scope(project_db, name)?;

    if new_pattern.is_none() && protection.is_none() {
        bail!("nothing to update — specify --pattern or --protection");
    }

    if let Some(p) = new_pattern {
        project_db.update_scope_pattern(scope_id, p)?;
        let old_pattern = scope.pattern.as_deref().unwrap_or("(none)");
        eprintln!("Updated pattern: {old_pattern} -> {p}");
    }

    if let Some(level_str) = protection {
        let level: ProtectionLevel = level_str.parse()?;
        project_db.insert_scope_policy(scope_id, &level)?;
        eprintln!("Updated protection: {level}");
    }

    Ok(())
}

pub fn run_remove(cwd: &Path, name: &str) -> Result<()> {
    let ctx = discover(cwd)?;
    let (_, project_db) = ctx.require_project()?;
    let (scope, scope_id) = require_scope(project_db, name)?;

    project_db.remove_scope(scope_id)?;
    eprintln!("Removed category '{}'", scope.name);

    Ok(())
}
