use std::path::Path;

use anyhow::{bail, Result};

#[cfg(unix)]
const CROSS_DEVICE_ERROR: i32 = 18; // EXDEV
#[cfg(windows)]
const CROSS_DEVICE_ERROR: i32 = 17; // ERROR_NOT_SAME_DEVICE

use crate::context::{discover, Context};
use crate::integrity;
use crate::models::ProtectionLevel;
use crate::reference::{parse_reference, resolve_references};
use crate::util::whoami;

pub fn run(cwd: &Path, reference: &str, category: &str) -> Result<()> {
    let ctx = discover(cwd)?;
    let Context::Project {
        project_root,
        project_db,
        ..
    } = &ctx
    else {
        bail!("must be inside a project to categorize files");
    };

    let parsed = parse_reference(reference)?;
    let collection = resolve_references(&[parsed], &ctx)?;
    let resolved = collection.expect_one(reference)?;
    let file = resolved.file;
    let file_id = file.id.ok_or_else(|| anyhow::anyhow!("file has no id"))?;

    let file_name = &file.name;
    let new_rel_path = format!("{category}/{file_name}");
    let old_path = project_root.join(&file.path);
    let new_path = project_root.join(&new_rel_path);

    validate_move(&old_path, &new_path, &file.path)?;

    if file.immutable {
        integrity::clear_immutable(&old_path)?;
    }

    rename_same_volume(&old_path, &new_path)?;
    project_db.update_file_path(file_id, &new_rel_path)?;

    let new_protection = apply_protection(file_id, &new_rel_path, &new_path, project_db)?;

    let user = whoami();
    let detail = serde_json::json!({
        "from": file.path,
        "to": new_rel_path,
    });
    project_db.insert_audit(
        "categorize",
        Some(file_id),
        Some(&user),
        Some(&detail.to_string()),
    )?;

    eprintln!("Moved: {} -> {}", file.path, new_rel_path);
    eprintln!("  Protection: {new_protection}");

    Ok(())
}

fn apply_protection(
    file_id: i64,
    rel_path: &str,
    abs_path: &Path,
    db: &crate::db::ProjectDb,
) -> Result<ProtectionLevel> {
    let protection = db.resolve_protection(rel_path)?;
    if protection == ProtectionLevel::Immutable {
        match integrity::set_immutable(abs_path) {
            Ok(()) => db.update_file_immutable(file_id, true)?,
            Err(e) => {
                eprintln!("  warning: could not set immutable flag: {e}");
                db.update_file_immutable(file_id, false)?;
            }
        }
    } else {
        db.update_file_immutable(file_id, false)?;
    }
    Ok(protection)
}

fn validate_move(old_path: &Path, new_path: &Path, rel_path: &str) -> Result<()> {
    if !old_path.exists() {
        bail!("file missing from disk: {rel_path}");
    }
    if new_path.exists() {
        bail!("destination already exists: {}", new_path.display());
    }

    let new_parent = new_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("invalid destination"))?;
    std::fs::create_dir_all(new_parent)?;

    // On Unix we can pre-check device IDs. On Windows, stable Rust lacks a safe
    // API for volume identity, so we rely on rename_same_volume catching the error.
    #[cfg(unix)]
    ensure_same_device(old_path, new_parent)?;

    Ok(())
}

#[cfg(unix)]
fn ensure_same_device(old_path: &Path, new_parent: &Path) -> Result<()> {
    use std::os::unix::fs::MetadataExt;

    let old_dev = old_path.metadata()?.dev();
    let new_dev = new_parent.metadata()?.dev();

    if old_dev != new_dev {
        bail!("cannot categorize across devices (source: dev {old_dev}, dest: dev {new_dev})");
    }
    Ok(())
}

fn rename_same_volume(from: &Path, to: &Path) -> Result<()> {
    std::fs::rename(from, to).map_err(|e| {
        if e.raw_os_error() == Some(CROSS_DEVICE_ERROR) {
            anyhow::anyhow!(
                "cannot categorize across volumes ({} -> {})",
                from.display(),
                to.display(),
            )
        } else {
            e.into()
        }
    })
}
