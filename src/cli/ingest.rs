use std::path::Path;

use anyhow::{bail, Result};
use chrono::Utc;

use crate::context::{discover, Context};
use crate::db::ProjectDb;
use crate::integrity;
use crate::models::{ProtectionLevel, TrackedFile};
use crate::util::whoami;

pub fn run(cwd: &Path, paths: &[String], category: Option<&str>) -> Result<()> {
    let ctx = discover(cwd)?;
    let Context::Project {
        project_root,
        project_db,
        ..
    } = ctx
    else {
        bail!("must be inside a project to ingest files");
    };

    for path_str in paths {
        ingest_one(&project_root, &project_db, Path::new(path_str), category)?;
    }

    Ok(())
}

fn ingest_one(
    project_root: &Path,
    db: &ProjectDb,
    source: &Path,
    category: Option<&str>,
) -> Result<()> {
    if !source.exists() {
        bail!("file not found: {}", source.display());
    }
    if !source.is_file() {
        bail!("not a regular file: {}", source.display());
    }

    let (rel_path, dest, file_name) = prepare_destination(project_root, db, source, category)?;

    let hash = integrity::hash_file(&dest)?;
    let size = std::fs::metadata(&dest)?.len();
    let mime_type = guess_mime(&file_name);

    let protection = db.resolve_protection(&rel_path)?;

    let is_immutable = try_set_immutable(&dest, protection);

    let provenance = serde_json::json!({
        "source": source.display().to_string(),
        "method": "ingest",
        "timestamp": Utc::now().to_rfc3339(),
    });

    let file = TrackedFile {
        id: None,
        name: file_name,
        path: rel_path.clone(),
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

    eprintln!("Ingested: {} -> {}", source.display(), rel_path);
    eprintln!("  Protection: {protection}");

    Ok(())
}

fn prepare_destination(
    project_root: &Path,
    db: &ProjectDb,
    source: &Path,
    category: Option<&str>,
) -> Result<(String, std::path::PathBuf, String)> {
    let file_name = source.file_name().map_or_else(
        || "unnamed".to_string(),
        |n| n.to_string_lossy().to_string(),
    );

    let rel_path = match category {
        Some(cat) => format!("{cat}/{file_name}"),
        None => file_name.clone(),
    };

    let dest = project_root.join(&rel_path);

    if dest.exists() {
        bail!("destination already exists: {}", dest.display());
    }
    if db.get_file_by_path(&rel_path)?.is_some() {
        bail!("file already registered at '{rel_path}'");
    }

    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::copy(source, &dest)?;

    Ok((rel_path, dest, file_name))
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
