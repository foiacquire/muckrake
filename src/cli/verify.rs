use std::path::Path;

use anyhow::{bail, Result};
use console::style;

use crate::context::{discover, Context};
use crate::integrity::{self, VerifyResult};

pub fn run(cwd: &Path, name: Option<&str>) -> Result<()> {
    let ctx = discover(cwd)?;
    let Context::Project {
        project_root,
        project_db,
        ..
    } = ctx
    else {
        bail!("must be inside a project to verify files");
    };

    let files = if let Some(n) = name {
        let f = project_db
            .get_file_by_name(n)?
            .ok_or_else(|| anyhow::anyhow!("file '{n}' not found"))?;
        vec![f]
    } else {
        project_db.list_files(None)?
    };

    let mut ok_count = 0u32;
    let mut modified_count = 0u32;
    let mut missing_count = 0u32;
    let mut skipped_count = 0u32;

    for file in &files {
        let Some(ref expected_hash) = file.sha256 else {
            skipped_count += 1;
            continue;
        };

        let file_path = project_root.join(&file.path);
        let result = integrity::verify_file(&file_path, expected_hash)?;

        match result {
            VerifyResult::Ok => {
                ok_count += 1;
                eprintln!("  {} {}", style("✓").green(), file.path);
            }
            VerifyResult::Modified { expected, actual } => {
                modified_count += 1;
                eprintln!(
                    "  {} {} MODIFIED",
                    style("✗").red().bold(),
                    style(&file.path).red()
                );
                eprintln!("    expected: {}", style(&expected).dim());
                eprintln!("    actual:   {}", style(&actual).dim());
            }
            VerifyResult::Missing => {
                missing_count += 1;
                eprintln!(
                    "  {} {} MISSING",
                    style("?").yellow(),
                    style(&file.path).yellow()
                );
            }
        }

        if file.immutable {
            let actually_immutable = integrity::is_immutable(&file_path).unwrap_or(false);
            if !actually_immutable && file_path.exists() {
                eprintln!(
                    "  {} {} immutable flag removed",
                    style("!").yellow(),
                    file.path
                );
            }
        }
    }

    eprintln!();
    eprintln!(
        "Verified: {ok_count} ok, {modified_count} modified, {missing_count} missing, {skipped_count} skipped"
    );

    let user = whoami();
    project_db.insert_audit("verify", None, Some(&user), None)?;

    if modified_count > 0 || missing_count > 0 {
        bail!("integrity check failed");
    }

    Ok(())
}

fn whoami() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .unwrap_or_else(|_| "unknown".to_string())
}
