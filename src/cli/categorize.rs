use std::os::unix::fs::MetadataExt;
use std::path::Path;

use anyhow::{bail, Result};

use crate::context::{discover, Context};
use crate::db::ProjectDb;
use crate::integrity;
use crate::models::ProtectionLevel;

pub fn run(cwd: &Path, name: &str, category: &str) -> Result<()> {
    let ctx = discover(cwd)?;
    let Context::Project {
        project_root,
        project_db,
        ..
    } = ctx
    else {
        bail!("must be inside a project to categorize files");
    };

    let file = project_db
        .get_file_by_name(name)?
        .ok_or_else(|| anyhow::anyhow!("file '{name}' not found"))?;
    let file_id = file.id.ok_or_else(|| anyhow::anyhow!("file has no id"))?;

    let file_name = &file.name;
    let new_rel_path = format!("{category}/{file_name}");
    let old_path = project_root.join(&file.path);
    let new_path = project_root.join(&new_rel_path);

    validate_move(&old_path, &new_path, &file.path)?;

    if file.immutable {
        integrity::clear_immutable(&old_path)?;
    }

    std::fs::rename(&old_path, &new_path)?;
    project_db.update_file_path(file_id, &new_rel_path)?;

    let new_protection = apply_protection(file_id, &new_rel_path, &new_path, &project_db)?;

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

fn validate_move(old_path: &Path, new_path: &Path, rel_path: &str) -> Result<()> {
    if !old_path.exists() {
        bail!("file missing from disk: {rel_path}");
    }
    if new_path.exists() {
        bail!("destination already exists: {}", new_path.display());
    }

    let old_dev = old_path.metadata()?.dev();
    let new_parent = new_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("invalid destination"))?;
    std::fs::create_dir_all(new_parent)?;
    let new_dev = new_parent.metadata()?.dev();

    if old_dev != new_dev {
        bail!("cannot categorize across devices (source: dev {old_dev}, dest: dev {new_dev})");
    }

    Ok(())
}

fn apply_protection(
    file_id: i64,
    rel_path: &str,
    abs_path: &Path,
    db: &ProjectDb,
) -> Result<ProtectionLevel> {
    let cat = db.match_category(rel_path)?;
    let protection = cat
        .as_ref()
        .map(|c| &c.protection_level)
        .cloned()
        .unwrap_or(ProtectionLevel::Editable);

    if protection == ProtectionLevel::Immutable {
        match integrity::set_immutable(abs_path) {
            Ok(()) => {
                db.update_file_immutable(file_id, true)?;
            }
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

fn whoami() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .unwrap_or_else(|_| "unknown".to_string())
}
