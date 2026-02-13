use std::path::{Path, PathBuf};

use anyhow::{bail, Result};
use chrono::Utc;

use crate::context::discover;
use crate::integrity;
use crate::models::{ProtectionLevel, TrackedFile};
use crate::util::whoami;

pub fn run(cwd: &Path, scope: Option<&str>) -> Result<()> {
    let ctx = discover(cwd)?;
    let (project_root, project_db) = ctx.require_project()?;

    let scan_dir = resolve_scan_dir(project_root, scope)?;
    if !scan_dir.exists() {
        bail!("directory not found: {}", scan_dir.display());
    }

    let mut count = 0;
    scan_recursive(&scan_dir, project_root, project_db, &mut count)?;

    if count == 0 {
        eprintln!("No new files to ingest");
    } else {
        eprintln!("Ingested {count} file(s)");
    }

    Ok(())
}

fn resolve_scan_dir(project_root: &Path, scope: Option<&str>) -> Result<PathBuf> {
    match scope {
        Some(s) => {
            if s.starts_with(':') {
                bail!(
                    "ingest scans the current project — cross-project references are not supported"
                );
            }
            Ok(project_root.join(s.replace('.', "/")))
        }
        None => Ok(project_root.to_path_buf()),
    }
}

fn scan_recursive(
    dir: &Path,
    project_root: &Path,
    db: &crate::db::ProjectDb,
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
            scan_recursive(&path, project_root, db, count)?;
        } else if path.is_file() {
            let rel_path = path
                .strip_prefix(project_root)?
                .to_string_lossy()
                .to_string();

            if db.get_file_by_path(&rel_path)?.is_some() {
                continue;
            }

            track_file(project_root, db, &path, &rel_path)?;
            *count += 1;
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
