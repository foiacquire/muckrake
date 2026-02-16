use std::collections::HashSet;
use std::path::Path;

use anyhow::Result;
use chrono::Utc;

use crate::context::{discover, Context};
use crate::db::WorkspaceDb;
use crate::integrity;
use crate::models::{ProtectionLevel, TrackedFile, TriggerEvent};
use crate::rules::{evaluate_rules, RuleContext, RuleEvent};
use crate::util::whoami;

pub fn run(cwd: &Path, scope: Option<&str>) -> Result<()> {
    let ctx = discover(cwd)?;
    let (project_root, project_db) = ctx.require_project()?;
    let (workspace_root, workspace_db) = workspace_from_ctx(&ctx);

    let rule_ctx = RuleContext {
        project_root,
        project_db,
        workspace_root,
        workspace_db,
    };

    let patterns = resolve_patterns(project_db, scope)?;

    let mut count = 0;
    walk_dir(project_root, &rule_ctx, &patterns, &mut count)?;

    if count == 0 {
        eprintln!("No new files to ingest");
    } else {
        eprintln!("Ingested {count} file(s)");
    }

    Ok(())
}

pub fn workspace_from_ctx(ctx: &Context) -> (Option<&Path>, Option<&WorkspaceDb>) {
    if let Context::Project {
        workspace: Some(ws),
        ..
    } = ctx
    {
        (Some(ws.workspace_root.as_path()), Some(&ws.workspace_db))
    } else {
        (None, None)
    }
}

fn resolve_patterns(
    db: &crate::db::ProjectDb,
    scope: Option<&str>,
) -> Result<Vec<glob::Pattern>> {
    let Some(name) = scope else {
        return Ok(vec![glob::Pattern::new("**")?]);
    };

    let cat = db
        .get_category_by_name(name)?
        .ok_or_else(|| anyhow::anyhow!("no category named '{name}'"))?;

    let base = crate::models::Category::name_from_pattern(&cat.pattern);
    Ok(vec![
        glob::Pattern::new(&format!("{base}/*"))?,
        glob::Pattern::new(&format!("{base}/**/*"))?,
    ])
}

fn walk_dir(
    dir: &Path,
    rule_ctx: &RuleContext<'_>,
    patterns: &[glob::Pattern],
    count: &mut usize,
) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();

        if name.starts_with('.') {
            continue;
        }

        if path.is_dir() {
            walk_dir(&path, rule_ctx, patterns, count)?;
        } else if path.is_file() {
            let rel_path = path
                .strip_prefix(rule_ctx.project_root)?
                .to_string_lossy()
                .to_string();

            if !patterns.iter().any(|p| p.matches(&rel_path)) {
                continue;
            }

            if rule_ctx.project_db.get_file_by_path(&rel_path)?.is_some() {
                continue;
            }

            let file_id = track_file(rule_ctx.project_root, rule_ctx.project_db, &path, &rel_path)?;
            *count += 1;

            if let Ok(Some(file)) = rule_ctx.project_db.get_file_by_path(&rel_path) {
                let event = RuleEvent {
                    event: TriggerEvent::Ingest,
                    file: &file,
                    file_id,
                    rel_path: &rel_path,
                    tag_name: None,
                    target_category: None,
                };
                let mut fired = HashSet::new();
                let _ = evaluate_rules(&event, rule_ctx, &mut fired);
            }
        }
    }
    Ok(())
}

/// Track a file that already exists on disk inside the project.
pub fn track_file(
    project_root: &Path,
    db: &crate::db::ProjectDb,
    abs_path: &Path,
    rel_path: &str,
) -> Result<i64> {
    let hash = integrity::hash_file(abs_path)?;
    let meta = std::fs::metadata(abs_path)?;
    let size = meta.len();

    let file_name = abs_path.file_name().map_or_else(
        || "unnamed".to_string(),
        |n| n.to_string_lossy().to_string(),
    );
    let mime_type = guess_mime(&file_name);

    let protection = db.resolve_protection(rel_path)?;
    let is_immutable = try_set_immutable(abs_path, protection);

    let provenance = serde_json::json!({
        "method": "ingest",
        "timestamp": Utc::now().to_rfc3339(),
    });

    let file = TrackedFile {
        id: None,
        name: file_name,
        path: rel_path.to_string(),
        sha256: Some(hash),
        mime_type,
        size: Some(size as i64),
        ingested_at: Utc::now().to_rfc3339(),
        provenance: Some(provenance.to_string()),
        immutable: is_immutable,
    };

    let file_id = db.insert_file(&file)?;
    let user = whoami();
    db.insert_audit("ingest", Some(file_id), Some(&user), None)?;

    eprintln!(
        "  {} [{}]",
        rel_path,
        protection_label(project_root, abs_path, protection, is_immutable)
    );

    Ok(file_id)
}

const fn protection_label(
    _project_root: &Path,
    _abs_path: &Path,
    protection: ProtectionLevel,
    is_immutable: bool,
) -> &'static str {
    match (protection, is_immutable) {
        (ProtectionLevel::Immutable, true) => "immutable",
        (ProtectionLevel::Immutable, false) => "immutable (flag failed)",
        (ProtectionLevel::Protected, _) => "protected",
        (ProtectionLevel::Editable, _) => "editable",
    }
}

fn try_set_immutable(path: &Path, protection: ProtectionLevel) -> bool {
    if protection != ProtectionLevel::Immutable {
        return false;
    }
    match integrity::set_immutable(path) {
        Ok(()) => true,
        Err(e) => {
            eprintln!("  warning: could not set immutable flag: {e}");
            false
        }
    }
}

fn guess_mime(filename: &str) -> Option<String> {
    if !filename.contains('.') {
        return None;
    }
    let ext = filename.rsplit('.').next()?.to_lowercase();
    let mime = match ext.as_str() {
        "pdf" => "application/pdf",
        "doc" | "docx" => "application/msword",
        "xls" | "xlsx" => "application/vnd.ms-excel",
        "csv" => "text/csv",
        "txt" => "text/plain",
        "md" => "text/markdown",
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "tiff" | "tif" => "image/tiff",
        "wav" => "audio/wav",
        "mp3" => "audio/mpeg",
        "mp4" => "video/mp4",
        "mkv" => "video/x-matroska",
        "html" | "htm" => "text/html",
        "json" => "application/json",
        "xml" => "application/xml",
        "zip" => "application/zip",
        _ => return None,
    };
    Some(mime.to_string())
}
