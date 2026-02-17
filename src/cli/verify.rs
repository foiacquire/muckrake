use std::path::Path;

use anyhow::{bail, Result};
use console::style;

use crate::context::{discover, Context};
use crate::db::ProjectDb;
use crate::integrity::{self, VerifyResult};
use crate::models::{ProtectionLevel, TrackedFile};
use crate::reference::{parse_reference, resolve_references};
use crate::util::whoami;

pub fn run(cwd: &Path, reference: Option<&str>) -> Result<()> {
    let ctx = discover(cwd)?;
    let Context::Project {
        project_root,
        project_db,
        ..
    } = &ctx
    else {
        bail!("must be inside a project to verify files");
    };

    let files = if let Some(r) = reference {
        let parsed = parse_reference(r)?;
        let collection = resolve_references(&[parsed], &ctx)?;
        if collection.files.is_empty() {
            bail!("reference '{r}' matched no files");
        }
        if collection.files.iter().any(|rf| rf.project_name.is_some()) {
            bail!("verify does not support cross-project references");
        }
        collection.files.into_iter().map(|rf| rf.file).collect()
    } else {
        project_db.list_files(None)?
    };

    let counts = verify_files(project_root, project_db, &files)?;

    eprintln!();
    if counts.fixed > 0 {
        eprintln!(
            "Verified: {} ok, {} modified, {} missing, {} skipped, {} fixed",
            counts.ok, counts.modified, counts.missing, counts.skipped, counts.fixed
        );
    } else {
        eprintln!(
            "Verified: {} ok, {} modified, {} missing, {} skipped",
            counts.ok, counts.modified, counts.missing, counts.skipped
        );
    }

    let user = whoami();
    project_db.insert_audit("verify", None, Some(&user), None)?;

    if counts.modified > 0 || counts.missing > 0 {
        bail!("integrity check failed");
    }

    Ok(())
}

struct VerifyCounts {
    ok: u32,
    modified: u32,
    missing: u32,
    skipped: u32,
    fixed: u32,
}

fn verify_files(
    project_root: &Path,
    project_db: &ProjectDb,
    files: &[TrackedFile],
) -> Result<VerifyCounts> {
    let mut counts = VerifyCounts {
        ok: 0,
        modified: 0,
        missing: 0,
        skipped: 0,
        fixed: 0,
    };

    for file in files {
        let Some(ref expected_hash) = file.sha256 else {
            counts.skipped += 1;
            continue;
        };

        let file_path = project_root.join(&file.path);
        let result = integrity::verify_file(&file_path, expected_hash)?;

        print_verify_result(&result, &file.path);
        match result {
            VerifyResult::Ok => counts.ok += 1,
            VerifyResult::Modified { .. } => counts.modified += 1,
            VerifyResult::Missing => counts.missing += 1,
        }

        counts.fixed += check_immutable_flag(file, &file_path, project_db)?;
    }

    Ok(counts)
}

fn print_verify_result(result: &VerifyResult, path: &str) {
    match result {
        VerifyResult::Ok => {
            eprintln!("  {} {path}", style("✓").green());
        }
        VerifyResult::Modified { expected, actual } => {
            eprintln!(
                "  {} {} MODIFIED",
                style("✗").red().bold(),
                style(path).red()
            );
            eprintln!("    expected: {}", style(expected).dim());
            eprintln!("    actual:   {}", style(actual).dim());
        }
        VerifyResult::Missing => {
            eprintln!("  {} {} MISSING", style("?").yellow(), style(path).yellow());
        }
    }
}

fn check_immutable_flag(
    file: &TrackedFile,
    file_path: &Path,
    project_db: &ProjectDb,
) -> Result<u32> {
    let expected = project_db.resolve_protection(&file.path)?;
    let file_exists = file_path.exists();
    let file_id = file.id.unwrap_or(0);
    let mut fixed = 0u32;

    if expected == ProtectionLevel::Immutable {
        if file_exists && !integrity::is_immutable(file_path)? {
            match integrity::set_immutable(file_path) {
                Ok(()) => {
                    eprintln!("  {} {} restored immutable flag", style("+").cyan(), file.path);
                    if !file.immutable {
                        project_db.update_file_immutable(file_id, true)?;
                    }
                    fixed += 1;
                }
                Err(e) => {
                    eprintln!(
                        "  {} {} failed to restore immutable flag: {e}",
                        style("!").yellow(),
                        file.path
                    );
                }
            }
        } else if file_exists && !file.immutable {
            project_db.update_file_immutable(file_id, true)?;
            eprintln!("  {} {} synced immutable flag to db", style("+").cyan(), file.path);
            fixed += 1;
        }
    } else if file.immutable {
        if file_exists {
            if let Err(e) = integrity::clear_immutable(file_path) {
                eprintln!(
                    "  {} {} failed to clear immutable flag: {e}",
                    style("!").yellow(),
                    file.path
                );
                return Ok(fixed);
            }
        }
        project_db.update_file_immutable(file_id, false)?;
        eprintln!(
            "  {} {} cleared immutable flag (policy: {expected})",
            style("+").cyan(),
            file.path
        );
        fixed += 1;
    }

    Ok(fixed)
}
