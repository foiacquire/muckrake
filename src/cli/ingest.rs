use std::path::Path;

use anyhow::Result;
use chrono::Utc;

use crate::context::{discover, Context};
use crate::db::WorkspaceDb;
use crate::integrity;
use crate::models::{ProtectionLevel, TrackedFile, TriggerEvent};
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

        if let Ok(Some(file)) = project_db.get_file_by_hash(&hash) {
            let protection = project_db
                .resolve_protection(rel_path)
                .unwrap_or(ProtectionLevel::Editable);
            let ref_str = format_ref(rel_path, ctx.project_name(), project_db);
            eprintln!(
                "  {ref_str} [{}]",
                protection_label(project_root, &abs_path, protection, file.immutable)
            );

            let event = RuleEvent {
                event: TriggerEvent::Ingest,
                file: Some(&file),
                file_id: Some(file_id),
                rel_path: Some(rel_path),
                tag_name: None,
                target_category: None,
                pipeline_name: None,
                sign_name: None,
                new_state: None,
            };
            crate::rules::fire_rules(&ctx, &event);
        }
    }

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

/// Track a file that already exists on disk inside the project.
pub fn track_file(db: &crate::db::ProjectDb, abs_path: &Path, rel_path: &str) -> Result<i64> {
    let (hash, fingerprint) = integrity::hash_and_fingerprint(abs_path)?;
    let meta = std::fs::metadata(abs_path)?;
    let size = meta.len();

    let file_name = abs_path.file_name().map_or_else(
        || "unnamed".to_string(),
        |n| n.to_string_lossy().to_string(),
    );
    let mime_type = guess_mime(&file_name).or_else(|| {
        if is_executable(&meta) {
            Some("application/x-executable".to_string())
        } else {
            None
        }
    });

    let protection = db.resolve_protection(rel_path)?;
    let is_immutable = try_set_immutable(abs_path, protection);

    let provenance = serde_json::json!({
        "method": "ingest",
        "timestamp": Utc::now().to_rfc3339(),
    });

    // name/path are written transitionally (schema still requires NOT NULL).
    // Phase 5 schema migration removes these columns.
    let file = TrackedFile {
        id: None,
        name: Some(file_name),
        path: Some(rel_path.to_string()),
        sha256: hash,
        fingerprint: fingerprint.to_json(),
        mime_type,
        size: Some(size as i64),
        ingested_at: Utc::now().to_rfc3339(),
        provenance: Some(provenance.to_string()),
        immutable: is_immutable,
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
