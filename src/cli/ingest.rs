use std::path::Path;

use anyhow::Result;
use chrono::Utc;

use crate::context::{discover, Context};
use crate::db::materialize::{self, FileContext};
use crate::db::{ProjectDb, WorkspaceDb};
use crate::integrity;
use crate::models::{ProtectionLevel, Scope, TrackedFile, TriggerEvent};
use crate::reference::format_ref;
use crate::rules::RuleEvent;
use crate::util::whoami;
use crate::walk;

pub fn run(cwd: &Path, scope: Option<&str>) -> Result<()> {
    let ctx = discover(cwd)?;
    let (project_root, project_db) = ctx.require_project()?;

    if let Some(name) = scope {
        if project_db.get_category_by_name(name)?.is_none() {
            anyhow::bail!("no category named '{name}'");
        }
    }

    let categories = project_db.list_categories()?;
    let patterns = walk::category_patterns(project_db, scope)?;
    let entries = walk::walk_and_collect(project_root, &patterns)?;

    let mut count = 0usize;
    for rel_path in &entries {
        let abs_path = project_root.join(rel_path);
        let hash = integrity::hash_file(&abs_path)?;
        if project_db.get_file_by_hash(&hash)?.is_some() {
            continue;
        }

        let file_id = track_file(project_db, &abs_path, rel_path)?;
        count += 1;

        materialize_for_new_file(project_db, rel_path, &hash, &categories);
        let evt = IngestResult {
            project_root,
            abs_path: &abs_path,
            rel_path,
            hash: &hash,
            file_id,
        };
        report_and_fire_rules(&ctx, project_db, &evt);
    }

    if count == 0 {
        eprintln!("No new files to ingest");
    } else {
        eprintln!("Ingested {count} file(s)");
    }

    Ok(())
}

fn materialize_for_new_file(db: &ProjectDb, rel_path: &str, sha256: &str, categories: &[Scope]) {
    let matching_cats: Vec<_> = categories
        .iter()
        .filter(|cat| cat.matches(rel_path).unwrap_or(false))
        .cloned()
        .collect();
    let file_ctx = FileContext {
        rel_path,
        sha256,
        matching_categories: &matching_cats,
        tags: &[],
    };
    let _ = materialize::materialize_all_for_file(db, &file_ctx);
}

struct IngestResult<'a> {
    project_root: &'a Path,
    abs_path: &'a Path,
    rel_path: &'a str,
    hash: &'a str,
    file_id: i64,
}

fn report_and_fire_rules(ctx: &Context, project_db: &ProjectDb, evt: &IngestResult<'_>) {
    let Ok(Some(file)) = project_db.get_file_by_hash(evt.hash) else {
        return;
    };

    let protection = project_db
        .resolve_protection_for_file(evt.hash, evt.rel_path)
        .unwrap_or(ProtectionLevel::Editable);
    let is_immutable = integrity::is_immutable(evt.abs_path).unwrap_or(false);
    let ref_str = format_ref(evt.rel_path, ctx.project_name(), project_db);
    eprintln!(
        "  {ref_str} [{}]",
        protection_label(evt.project_root, evt.abs_path, protection, is_immutable)
    );

    let event = RuleEvent {
        event: TriggerEvent::Ingest,
        file: Some(&file),
        file_id: Some(evt.file_id),
        rel_path: Some(evt.rel_path),
        tag_name: None,
        target_category: None,
        pipeline_name: None,
        sign_name: None,
        new_state: None,
    };
    crate::rules::fire_rules(ctx, &event);
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

/// Track a file that already exists on disk inside the project.
pub fn track_file(db: &ProjectDb, abs_path: &Path, rel_path: &str) -> Result<i64> {
    let (hash, fingerprint) = integrity::hash_and_fingerprint(abs_path)?;
    let meta = std::fs::metadata(abs_path)?;
    let size = meta.len();

    let filename = abs_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let mime_type = guess_mime(&filename).or_else(|| {
        if is_executable(&meta) {
            Some("application/x-executable".to_string())
        } else {
            None
        }
    });

    let protection = db.resolve_protection_for_file(&hash, rel_path)?;
    try_set_immutable(abs_path, protection);

    let provenance = serde_json::json!({
        "method": "ingest",
        "timestamp": Utc::now().to_rfc3339(),
    });

    let file = TrackedFile {
        id: None,
        name: None,
        path: None,
        sha256: hash,
        fingerprint: fingerprint.to_json(),
        mime_type,
        size: Some(size as i64),
        ingested_at: Utc::now().to_rfc3339(),
        provenance: Some(provenance.to_string()),
        immutable: false,
    };

    let file_id = db.insert_file(&file)?;
    let user = whoami();
    db.insert_audit("ingest", Some(file_id), Some(&user), None)?;

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

#[cfg(unix)]
fn is_executable(meta: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    meta.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn is_executable(_meta: &std::fs::Metadata) -> bool {
    false
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
