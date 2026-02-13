use std::path::Path;

use anyhow::Result;
use console::style;

use crate::context::discover;
use crate::models::TrackedFile;
use crate::reference::{parse_reference, resolve_references, Reference};
use crate::util::format_size;

pub fn run(cwd: &Path, raw_refs: &[String]) -> Result<()> {
    let ctx = discover(cwd)?;

    let refs = if raw_refs.is_empty() {
        vec![Reference::Structured {
            scope: vec![],
            tags: vec![],
            glob: None,
        }]
    } else {
        raw_refs
            .iter()
            .map(|r| parse_reference(r))
            .collect::<Result<Vec<_>>>()?
    };

    let collection = resolve_references(&refs, &ctx)?;

    if collection.files.is_empty() {
        eprintln!("(no files)");
        return Ok(());
    }

    let mut current_project: Option<&Option<String>> = None;
    let mut has_multiple_projects = false;

    for rf in &collection.files {
        if let Some(prev) = current_project {
            if prev != &rf.project_name {
                has_multiple_projects = true;
                break;
            }
        } else {
            current_project = Some(&rf.project_name);
        }
    }

    if has_multiple_projects {
        let mut last_project: Option<&Option<String>> = None;
        for rf in &collection.files {
            if last_project != Some(&rf.project_name) {
                if let Some(ref name) = rf.project_name {
                    println!("{}:", style(name).bold());
                }
                last_project = Some(&rf.project_name);
            }
            print_file(&rf.file);
        }
    } else {
        if let Some(Some(ref name)) = collection.files.first().map(|f| &f.project_name) {
            println!("{}:", style(name).bold());
        }
        for rf in &collection.files {
            print_file(&rf.file);
        }
    }

    Ok(())
}

fn print_file(f: &TrackedFile) {
    let protection = if f.immutable { "immutable" } else { "editable" };
    let size = f.size.map_or_else(|| "?".to_string(), format_size);
    println!(
        "  {} {} [{}] {}",
        style(&f.name).bold(),
        style(&f.path).dim(),
        protection,
        style(size).dim()
    );
}
