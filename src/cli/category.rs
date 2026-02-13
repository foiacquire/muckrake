use std::path::Path;

use anyhow::{bail, Result};
use console::style;

use crate::context::discover;
use crate::models::{Category, CategoryType, ProtectionLevel};

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
            "  {} {}{}",
            style(&cat.pattern).bold(),
            style(protection).dim(),
            type_label
        );

        if let Some(ref desc) = cat.description {
            println!("    {}", style(desc).dim());
        }
    }

    Ok(())
}

pub fn run_add(
    cwd: &Path,
    pattern: &str,
    category_type: &str,
    protection: &str,
    description: Option<&str>,
) -> Result<()> {
    let ctx = discover(cwd)?;
    let (_, project_db) = ctx.require_project()?;

    if project_db.get_category_by_pattern(pattern)?.is_some() {
        bail!("category '{pattern}' already exists");
    }

    let cat_type: CategoryType = category_type.parse()?;
    let level: ProtectionLevel = protection.parse()?;

    let cat = Category {
        id: None,
        pattern: pattern.to_string(),
        category_type: cat_type,
        description: description.map(String::from),
    };

    let cat_id = project_db.insert_category(&cat)?;
    project_db.insert_category_policy(cat_id, &level)?;

    eprintln!("Added category '{pattern}' ({level})");
    Ok(())
}

pub fn run_update(
    cwd: &Path,
    pattern: &str,
    new_pattern: Option<&str>,
    protection: Option<&str>,
) -> Result<()> {
    let ctx = discover(cwd)?;
    let (_, project_db) = ctx.require_project()?;

    let cat = project_db
        .get_category_by_pattern(pattern)?
        .ok_or_else(|| anyhow::anyhow!("no category with pattern '{pattern}'"))?;
    let cat_id = cat
        .id
        .ok_or_else(|| anyhow::anyhow!("category has no id"))?;

    if new_pattern.is_none() && protection.is_none() {
        bail!("nothing to update — specify --pattern or --protection");
    }

    if let Some(p) = new_pattern {
        if project_db.get_category_by_pattern(p)?.is_some() {
            bail!("category '{p}' already exists");
        }
        project_db.update_category_pattern(cat_id, p)?;
        eprintln!("Updated pattern: {pattern} -> {p}");
    }

    if let Some(level_str) = protection {
        let level: ProtectionLevel = level_str.parse()?;
        project_db.insert_category_policy(cat_id, &level)?;
        eprintln!("Updated protection: {level}");
    }

    Ok(())
}

pub fn run_remove(cwd: &Path, pattern: &str) -> Result<()> {
    let ctx = discover(cwd)?;
    let (_, project_db) = ctx.require_project()?;

    let cat = project_db
        .get_category_by_pattern(pattern)?
        .ok_or_else(|| anyhow::anyhow!("no category with pattern '{pattern}'"))?;
    let cat_id = cat
        .id
        .ok_or_else(|| anyhow::anyhow!("category has no id"))?;

    project_db.remove_category(cat_id)?;
    eprintln!("Removed category '{pattern}'");

    Ok(())
}
