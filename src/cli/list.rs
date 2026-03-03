use std::path::Path;

use anyhow::Result;
use console::style;

use crate::cli::ingest::track_file;
use crate::context::{discover, Context};
use crate::db::ProjectDb;
use crate::integrity;
use crate::reference::{
    expand_reference_scope, format_ref, parse_reference, ExpandedScope, Reference, ScopeLevel,
    TagFilter,
};
use crate::walk;

pub fn run(cwd: &Path, raw_refs: &[String], _no_hash_check: bool) -> Result<()> {
    let ctx = discover(cwd)?;
    let refs = build_refs(raw_refs)?;

    let mut found = false;
    for r in &refs {
        found |= list_reference(r, &ctx)?;
    }

    if !found {
        eprintln!("(no files)");
    }
    Ok(())
}

fn build_refs(raw_refs: &[String]) -> Result<Vec<Reference>> {
    if raw_refs.is_empty() {
        return Ok(vec![Reference::Structured {
            scope: vec![],
            tags: vec![],
            glob: None,
        }]);
    }

    raw_refs
        .iter()
        .map(|r| {
            // Bare words without path separators are category/project names,
            // not file lookups. Promote to structured references so they
            // resolve correctly from both project and workspace context.
            if !r.starts_with(':') && !r.contains('/') {
                parse_reference(&format!(":{r}"))
            } else {
                parse_reference(r)
            }
        })
        .collect()
}

fn list_reference(reference: &Reference, ctx: &Context) -> Result<bool> {
    match reference {
        Reference::BarePath(path) => list_bare_path(path, ctx),
        Reference::Structured { scope, tags, glob } => {
            list_structured(scope, tags, glob.as_deref(), ctx)
        }
    }
}

fn list_bare_path(path: &str, ctx: &Context) -> Result<bool> {
    let (project_root, project_db) = ctx.require_project()?;
    let abs_path = project_root.join(path);
    if !abs_path.exists() {
        return Ok(false);
    }

    let hash = integrity::hash_file(&abs_path)?;
    let was_tracked = project_db.get_file_by_hash(&hash)?.is_some();
    if !was_tracked {
        if let Err(e) = track_file(project_db, &abs_path, path) {
            eprintln!("  warning: could not auto-ingest {path}: {e}");
        }
    }

    let file_name = abs_path.file_name().map_or_else(
        || "unnamed".to_string(),
        |n| n.to_string_lossy().to_string(),
    );
    print_file(&file_name, path, Some(&hash), was_tracked);
    Ok(true)
}

fn list_structured(
    scope: &[ScopeLevel],
    tags: &[TagFilter],
    glob: Option<&str>,
    ctx: &Context,
) -> Result<bool> {
    let targets = expand_reference_scope(scope, ctx)?;
    let glob_pattern = glob.map(glob::Pattern::new).transpose()?;

    let mut found = false;
    for target in &targets {
        found |= list_target(target, tags, glob_pattern.as_ref())?;
    }
    Ok(found)
}

fn list_target(
    target: &ExpandedScope,
    tags: &[TagFilter],
    glob_filter: Option<&glob::Pattern>,
) -> Result<bool> {
    let db = ProjectDb::open(&target.project_root.join(".mkrk"))?;
    let patterns = walk::category_patterns(&db, target.category_name.as_deref())?;
    let project_name = target.project_name.as_deref();

    let entries = walk::walk_and_collect(&target.project_root, &patterns)?;

    let mut found = false;
    let mut auto_ingested = 0usize;

    for rel_path in &entries {
        let file_name = Path::new(rel_path).file_name().map_or_else(
            || "unnamed".to_string(),
            |n| n.to_string_lossy().to_string(),
        );

        if let Some(pattern) = glob_filter {
            if !pattern.matches(&file_name) && !pattern.matches(rel_path.as_str()) {
                continue;
            }
        }

        let abs_path = target.project_root.join(rel_path);
        let hash = integrity::hash_file(&abs_path)?;
        let was_tracked = db.get_file_by_hash(&hash)?.is_some();
        if !was_tracked && track_file(&db, &abs_path, rel_path).is_ok() {
            auto_ingested += 1;
        }

        if !tags.is_empty() && !matches_tags(&db, &hash, tags)? {
            continue;
        }

        let ref_str = format_ref(rel_path, project_name, &db);
        print_file(&file_name, &ref_str, Some(&hash), was_tracked);
        found = true;
    }

    if auto_ingested > 0 {
        eprintln!("({auto_ingested} file(s) auto-ingested)");
    }

    Ok(found)
}

pub(crate) fn matches_tags(
    db: &ProjectDb,
    sha256: &str,
    tag_filters: &[TagFilter],
) -> Result<bool> {
    let Some(f) = db.get_file_by_hash(sha256)? else {
        return Ok(false);
    };
    let Some(file_id) = f.id else {
        return Ok(false);
    };
    let file_tags = db.get_tags(file_id)?;

    for filter in tag_filters {
        let group_matches = filter.tags.iter().any(|t| file_tags.contains(t));
        if !group_matches {
            return Ok(false);
        }
    }
    Ok(true)
}

fn print_file(name: &str, path: &str, sha256: Option<&str>, was_tracked: bool) {
    let hash_prefix = sha256.map_or("--------", |h| &h[..h.len().min(8)]);
    let status = if was_tracked { " " } else { "+" };
    println!(
        " {} {} {} {}",
        style(status).green(),
        style(name).bold(),
        style(path).dim(),
        style(hash_prefix).dim()
    );
}
