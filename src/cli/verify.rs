use std::path::Path;

use anyhow::{bail, Result};
use console::style;

use crate::context::{discover, Context};
use crate::integrity::{self, VerifyResult};
use crate::models::TrackedFile;
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
        collection.files.into_iter().map(|rf| rf.file).collect()
    } else {
        project_db.list_files(None)?
    };

    let counts = verify_files(project_root, &files)?;

    eprintln!();
    eprintln!(
        "Verified: {} ok, {} modified, {} missing, {} skipped",
        counts.ok, counts.modified, counts.missing, counts.skipped
    );

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
}

fn verify_files(project_root: &Path, files: &[TrackedFile]) -> Result<VerifyCounts> {
    let mut counts = VerifyCounts {
        ok: 0,
        modified: 0,
        missing: 0,
        skipped: 0,
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

        check_immutable_flag(file, &file_path);
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

fn check_immutable_flag(file: &TrackedFile, file_path: &Path) {
    if !file.immutable {
        return;
    }
    let actually_immutable = integrity::is_immutable(file_path).unwrap_or(false);
    if !actually_immutable && file_path.exists() {
        eprintln!(
            "  {} {} immutable flag removed",
            style("!").yellow(),
            file.path
        );
    }
}
