use std::path::Path;

use anyhow::{bail, Result};
use console::style;

use crate::context::discover;
use crate::db::ProjectDb;
use crate::models::{Category, CategoryType, ProtectionLevel};
use crate::reference::validate_name;

use super::create_category_dir;

/// Resolve a category by name first, then fall back to pattern match.
fn find_category(db: &ProjectDb, input: &str) -> Result<Option<Category>> {
    if let Some(cat) = db.get_category_by_name(input)? {
        return Ok(Some(cat));
    }
    db.get_category_by_pattern(input)
}

/// Find a category by name/pattern and extract its id, or bail with a clear error.
fn require_category(db: &ProjectDb, name: &str) -> Result<(Category, i64)> {
    let cat =
        find_category(db, name)?.ok_or_else(|| anyhow::anyhow!("no category matching '{name}'"))?;
    let cat_id = cat
        .id
        .ok_or_else(|| anyhow::anyhow!("category has no id"))?;
    Ok((cat, cat_id))
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
            .and_then(|id| project_db.get_policy_for_category(id).ok().flatten())
            .unwrap_or(ProtectionLevel::Editable);

        let type_label = if cat.category_type == CategoryType::Files {
            String::new()
        } else {
            format!(" [{}]", cat.category_type)
        };

        println!(
            "  {} {} {}{}",
            style(&cat.name).bold(),
            style(&cat.pattern).dim(),
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

    let cat = Category {
        id: None,
        name: name.to_string(),
        pattern: resolved_pattern,
        category_type: cat_type,
        description: params.description.map(String::from),
    };

    let cat_id = project_db.insert_category(&cat)?;
    project_db.insert_category_policy(cat_id, &level)?;
    create_category_dir(project_root, &cat.pattern);

    eprintln!("Added category '{}' ({level})", cat.name);
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
    let (cat, cat_id) = require_category(project_db, name)?;

    if new_pattern.is_none() && protection.is_none() {
        bail!("nothing to update — specify --pattern or --protection");
    }

    if let Some(p) = new_pattern {
        project_db.update_category_pattern(cat_id, p)?;
        eprintln!("Updated pattern: {} -> {p}", cat.pattern);
    }

    if let Some(level_str) = protection {
        let level: ProtectionLevel = level_str.parse()?;
        project_db.insert_category_policy(cat_id, &level)?;
        eprintln!("Updated protection: {level}");
    }

    Ok(())
}

pub fn run_remove(cwd: &Path, name: &str) -> Result<()> {
    let ctx = discover(cwd)?;
    let (_, project_db) = ctx.require_project()?;
    let (cat, cat_id) = require_category(project_db, name)?;

    project_db.remove_category(cat_id)?;
    eprintln!("Removed category '{}'", cat.name);

    Ok(())
}
