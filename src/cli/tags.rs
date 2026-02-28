use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{bail, Result};
use console::style;

use crate::context::{discover, Context};
use crate::integrity;
use crate::models::{TrackedFile, TriggerEvent};
use crate::reference::{parse_reference, resolve_references};
use crate::rules::RuleEvent;

fn fire_tag_event(
    ctx: &Context,
    file: &TrackedFile,
    file_id: i64,
    tag: &str,
    trigger: TriggerEvent,
) {
    let event = RuleEvent {
        event: trigger,
        file: Some(file),
        file_id: Some(file_id),
        rel_path: Some(&file.path),
        tag_name: Some(tag),
        target_category: None,
        pipeline_name: None,
        sign_name: None,
        new_state: None,
    };
    crate::rules::fire_rules(ctx, &event);
}

fn resolve_file_ref(reference: &str, ctx: &Context) -> Result<(TrackedFile, i64)> {
    let parsed = parse_reference(reference)?;
    let collection = resolve_references(&[parsed], ctx)?;
    let resolved = collection.expect_one(reference)?;
    let file = resolved.file;
    let file_id = file.id.ok_or_else(|| anyhow::anyhow!("file has no id"))?;
    Ok((file, file_id))
}

pub fn run_tag(cwd: &Path, reference: &str, tag: &str) -> Result<()> {
    let ctx = discover(cwd)?;
    let (project_root, project_db) = ctx.require_project()?;
    let (file, file_id) = resolve_file_ref(reference, &ctx)?;

    let abs_path = project_root.join(&file.path);
    let (hash, fingerprint) = integrity::hash_and_fingerprint(&abs_path)?;
    let fp_json = fingerprint.to_json();

    project_db.insert_tag(file_id, tag, &hash, &fp_json)?;

    let short_hash = &hash[..10];
    eprintln!(
        "Tagged '{}' with '{tag}' (sha256: {short_hash}...)",
        file.name
    );

    fire_tag_event(&ctx, &file, file_id, tag, TriggerEvent::Tag);

    Ok(())
}

pub fn run_untag(cwd: &Path, reference: &str, tag: &str) -> Result<()> {
    let ctx = discover(cwd)?;
    let (_project_root, project_db) = ctx.require_project()?;
    let (file, file_id) = resolve_file_ref(reference, &ctx)?;

    let removed = project_db.remove_tag(file_id, tag)?;
    if removed == 0 {
        bail!("tag '{tag}' not found on '{}'", file.name);
    }
    eprintln!("Removed tag '{tag}' from '{}'", file.name);

    fire_tag_event(&ctx, &file, file_id, tag, TriggerEvent::Untag);

    Ok(())
}

pub fn run_tags(cwd: &Path, reference: Option<&str>, no_hash_check: bool) -> Result<()> {
    let ctx = discover(cwd)?;
    let (project_root, project_db) = ctx.require_project()?;

    if no_hash_check {
        eprintln!(
            "{}",
            style("warning: hash verification skipped — stale tags will not be detected").yellow()
        );
    }

    if let Some(r) = reference {
        let (file, file_id) = resolve_file_ref(r, &ctx)?;
        let tags = project_db.get_tags(file_id)?;
        if tags.is_empty() {
            eprintln!("No tags on '{}'", file.name);
        } else {
            println!("Tags on '{}':", file.name);
            for tag in &tags {
                let status = if no_hash_check {
                    String::new()
                } else {
                    format_tag_status(project_db, project_root, file_id, tag, &file.path)
                };
                println!("  {}{status}", style(tag).cyan());
            }
        }
    } else {
        let all_tags = project_db.list_all_tags()?;
        if all_tags.is_empty() {
            eprintln!("No tags in project");
        } else {
            let mut by_tag: BTreeMap<String, Vec<i64>> = BTreeMap::new();
            for ft in &all_tags {
                by_tag.entry(ft.tag.clone()).or_default().push(ft.file_id);
            }
            for (tag, file_ids) in &by_tag {
                println!("  {} ({} files)", style(tag).cyan(), file_ids.len());
            }
        }
    }

    Ok(())
}

fn format_tag_status(
    project_db: &crate::db::ProjectDb,
    project_root: &Path,
    file_id: i64,
    tag: &str,
    file_path: &str,
) -> String {
    let stored_fp = match project_db.get_file_tag_fingerprint(file_id, tag) {
        Ok(Some(fp)) => fp,
        Ok(None) => return format_tag_status_sha256(project_db, project_root, file_id, tag, file_path),
        Err(_) => return format!(" {}", style("(lookup failed)").red()),
    };

    let Ok(expected) = integrity::Fingerprint::from_json(&stored_fp) else {
        return format_tag_status_sha256(project_db, project_root, file_id, tag, file_path);
    };

    let abs_path = project_root.join(file_path);
    match integrity::verify_fingerprint(&abs_path, &expected) {
        Ok(integrity::FingerprintResult::Ok) => format!(" {}", style("✓").green()),
        Ok(integrity::FingerprintResult::Modified { changed }) => {
            let n = changed.len();
            let detail = if n == 1 {
                format!("chunk {} changed", changed[0].index)
            } else {
                format!("{n} chunks changed")
            };
            format!(
                " {}",
                style(format!("⚠ file modified since tagging ({detail})")).yellow()
            )
        }
        Ok(integrity::FingerprintResult::Missing) => {
            format!(" {}", style("✗ file missing").red())
        }
        Err(_) => format!(" {}", style("(verify failed)").red()),
    }
}

fn format_tag_status_sha256(
    project_db: &crate::db::ProjectDb,
    project_root: &Path,
    file_id: i64,
    tag: &str,
    file_path: &str,
) -> String {
    let stored_hash = match project_db.get_file_tag_hash(file_id, tag) {
        Ok(Some(h)) => h,
        Ok(None) => return format!(" {}", style("(no hash)").dim()),
        Err(_) => return format!(" {}", style("(hash lookup failed)").red()),
    };

    let abs_path = project_root.join(file_path);
    match integrity::verify_file(&abs_path, &stored_hash) {
        Ok(integrity::VerifyResult::Ok) => format!(" {}", style("✓").green()),
        Ok(integrity::VerifyResult::Modified { .. }) => {
            format!(" {}", style("⚠ file modified since tagging").yellow())
        }
        Ok(integrity::VerifyResult::Missing) => {
            format!(" {}", style("✗ file missing").red())
        }
        Err(_) => format!(" {}", style("(verify failed)").red()),
    }
}
