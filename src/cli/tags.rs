use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{bail, Result};
use console::style;

use crate::context::{discover, Context};

pub fn run_tag(cwd: &Path, name: &str, tag: &str) -> Result<()> {
    let ctx = discover(cwd)?;
    let Context::Project { project_db, .. } = ctx else {
        bail!("must be inside a project to tag files");
    };

    let file = project_db
        .get_file_by_name(name)?
        .ok_or_else(|| anyhow::anyhow!("file '{name}' not found"))?;
    let file_id = file.id.ok_or_else(|| anyhow::anyhow!("file has no id"))?;

    project_db.insert_tag(file_id, tag)?;
    eprintln!("Tagged '{name}' with '{tag}'");

    Ok(())
}

pub fn run_untag(cwd: &Path, name: &str, tag: &str) -> Result<()> {
    let ctx = discover(cwd)?;
    let Context::Project { project_db, .. } = ctx else {
        bail!("must be inside a project to untag files");
    };

    let file = project_db
        .get_file_by_name(name)?
        .ok_or_else(|| anyhow::anyhow!("file '{name}' not found"))?;
    let file_id = file.id.ok_or_else(|| anyhow::anyhow!("file has no id"))?;

    project_db.remove_tag(file_id, tag)?;
    eprintln!("Removed tag '{tag}' from '{name}'");

    Ok(())
}

pub fn run_tags(cwd: &Path, name: Option<&str>) -> Result<()> {
    let ctx = discover(cwd)?;
    let Context::Project { project_db, .. } = ctx else {
        bail!("must be inside a project to list tags");
    };

    if let Some(n) = name {
        let file = project_db
            .get_file_by_name(n)?
            .ok_or_else(|| anyhow::anyhow!("file '{n}' not found"))?;
        let file_id = file.id.ok_or_else(|| anyhow::anyhow!("file has no id"))?;
        let tags = project_db.get_tags(file_id)?;
        if tags.is_empty() {
            eprintln!("No tags on '{n}'");
        } else {
            eprintln!("Tags on '{n}':");
            for tag in &tags {
                eprintln!("  {}", style(tag).cyan());
            }
        }
    } else {
        let all_tags = project_db.list_all_tags()?;
        if all_tags.is_empty() {
            eprintln!("No tags in project");
        } else {
            let mut by_tag: BTreeMap<String, Vec<i64>> = BTreeMap::new();
            for ft in &all_tags {
                by_tag.entry(ft.tag.clone()).or_default().push(ft.file_id);
            }
            for (tag, file_ids) in &by_tag {
                eprintln!("  {} ({} files)", style(tag).cyan(), file_ids.len());
            }
        }
    }

    Ok(())
}
