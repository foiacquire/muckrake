use std::path::Path;

use anyhow::{bail, Result};
use console::style;

use crate::context::discover;
use crate::db::ProjectDb;
use crate::integrity::{self, VerifyResult};
use crate::models::{ProtectionLevel, TrackedFile};
use crate::reference::{format_ref, parse_reference, resolve_references};
use crate::util::whoami;

pub fn run(cwd: &Path, reference: Option<&str>) -> Result<()> {
    let ctx = discover(cwd)?;
    let (project_root, project_db) = ctx.require_project()?;

    let vctx = VerifyCtx {
        root: project_root,
        db: project_db,
        name: ctx.project_name(),
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

    let counts = verify_files(&vctx, &files)?;

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

struct VerifyCtx<'a> {
    root: &'a Path,
    db: &'a ProjectDb,
    name: Option<&'a str>,
}

impl VerifyCtx<'_> {
    fn format_ref(&self, path: &str) -> String {
        format_ref(path, self.name, self.db)
    }
}

struct VerifyCounts {
    ok: u32,
    modified: u32,
    missing: u32,
    skipped: u32,
    fixed: u32,
}

fn verify_files(vctx: &VerifyCtx<'_>, files: &[TrackedFile]) -> Result<VerifyCounts> {
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

        let abs_path = vctx.root.join(&file.path);
        let result = integrity::verify_file(&abs_path, expected_hash)?;

        print_verify_result(vctx, &result, &abs_path, file);
        match result {
            VerifyResult::Ok => {
                counts.ok += 1;
                counts.fixed += backfill_fingerprint(vctx, file, &abs_path)?;
            }
            VerifyResult::Modified { .. } => counts.modified += 1,
            VerifyResult::Missing => counts.missing += 1,
        }

        counts.fixed += check_immutable_flag(vctx, file, &abs_path)?;
    }

    Ok(counts)
}

fn print_verify_result(
    vctx: &VerifyCtx<'_>,
    result: &VerifyResult,
    abs_path: &Path,
    file: &TrackedFile,
) {
    let ref_str = vctx.format_ref(&file.path);
    match result {
        VerifyResult::Ok => {
            eprintln!("  {} {ref_str}", style("✓").green());
        }
        VerifyResult::Modified { expected, actual } => {
            eprintln!(
                "  {} {} MODIFIED",
                style("✗").red().bold(),
                style(&ref_str).red()
            );
            eprintln!("    expected: {}", style(expected).dim());
            eprintln!("    actual:   {}", style(actual).dim());
            print_chunk_diff(abs_path, file.fingerprint.as_deref());
        }
        VerifyResult::Missing => {
            eprintln!(
                "  {} {} MISSING",
                style("?").yellow(),
                style(&ref_str).yellow()
            );
        }
    }
}

fn print_chunk_diff(abs_path: &Path, fingerprint: Option<&str>) {
    let Some(fp_json) = fingerprint else {
        return;
    };
    let Ok(expected) = integrity::Fingerprint::from_json(fp_json) else {
        return;
    };
    if let Ok(integrity::FingerprintResult::Modified { changed }) =
        integrity::verify_fingerprint(abs_path, &expected)
    {
        let ranges: Vec<String> = changed
            .iter()
            .map(|c| format!("chunk {} (offset {})", c.index, c.offset))
            .collect();
        eprintln!("    changed: {}", style(ranges.join(", ")).dim());
    }
}

fn backfill_fingerprint(vctx: &VerifyCtx<'_>, file: &TrackedFile, abs_path: &Path) -> Result<u32> {
    if file.fingerprint.is_some() {
        return Ok(0);
    }
    let file_id = file.id.unwrap_or(0);
    if file_id == 0 {
        return Ok(0);
    }
    let fp = integrity::fingerprint_file(abs_path)?;
    vctx.db.update_file_fingerprint(file_id, &fp.to_json())?;
    let ref_str = vctx.format_ref(&file.path);
    eprintln!(
        "  {} {ref_str} stored fingerprint ({fp})",
        style("+").cyan()
    );
    Ok(1)
}

fn check_immutable_flag(vctx: &VerifyCtx<'_>, file: &TrackedFile, file_path: &Path) -> Result<u32> {
    let expected = vctx.db.resolve_protection(&file.path)?;
    let file_id = file.id.unwrap_or(0);

    if expected == ProtectionLevel::Immutable {
        ensure_immutable(vctx, file, file_path, file_id)
    } else if file.immutable {
        clear_unexpected_immutable(vctx, file, file_path, file_id, expected)
    } else {
        Ok(0)
    }
}

fn ensure_immutable(
    vctx: &VerifyCtx<'_>,
    file: &TrackedFile,
    file_path: &Path,
    file_id: i64,
) -> Result<u32> {
    if !file_path.exists() {
        return Ok(0);
    }

    let ref_str = vctx.format_ref(&file.path);
    if !integrity::is_immutable(file_path)? {
        match integrity::set_immutable(file_path) {
            Ok(()) => {
                eprintln!("  {} {ref_str} restored immutable flag", style("+").cyan());
                if !file.immutable {
                    vctx.db.update_file_immutable(file_id, true)?;
                }
                return Ok(1);
            }
            Err(e) => {
                eprintln!(
                    "  {} {ref_str} failed to restore immutable flag: {e}",
                    style("!").yellow()
                );
            }
        }
    } else if !file.immutable {
        vctx.db.update_file_immutable(file_id, true)?;
        eprintln!(
            "  {} {ref_str} synced immutable flag to db",
            style("+").cyan()
        );
        return Ok(1);
    }

    Ok(0)
}

fn clear_unexpected_immutable(
    vctx: &VerifyCtx<'_>,
    file: &TrackedFile,
    file_path: &Path,
    file_id: i64,
    expected: ProtectionLevel,
) -> Result<u32> {
    let ref_str = vctx.format_ref(&file.path);
    if file_path.exists() {
        if let Err(e) = integrity::clear_immutable(file_path) {
            eprintln!(
                "  {} {ref_str} failed to clear immutable flag: {e}",
                style("!").yellow()
            );
            return Ok(0);
        }
    }
    vctx.db.update_file_immutable(file_id, false)?;
    eprintln!(
        "  {} {ref_str} cleared immutable flag (policy: {expected})",
        style("+").cyan()
    );
    Ok(1)
}
