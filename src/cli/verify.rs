use std::collections::HashSet;
use std::path::Path;

use anyhow::{bail, Result};
use console::style;

use crate::context::discover;
use crate::db::ProjectDb;
use crate::integrity::{self, VerifyResult};
use crate::models::{ProtectionLevel, TrackedFile};
use crate::reference::{format_ref, parse_reference, resolve_references};
use crate::util::whoami;
use crate::walk;

pub fn run(cwd: &Path, references: &[String]) -> Result<()> {
    let ctx = discover(cwd)?;
    let (project_root, project_db) = ctx.require_project()?;

    let vctx = VerifyCtx {
        root: project_root,
        db: project_db,
        name: ctx.project_name(),
    };

    let pending_migration = project_db.needs_content_addressed_migration();

    let items: Vec<VerifyItem> = if pending_migration && references.is_empty() {
        collect_legacy(&vctx)?
    } else if references.is_empty() {
        collect_all(&vctx)?
    } else {
        collect_from_references(references, &ctx, &vctx)?
    };

    let counts = verify_items(&vctx, &items)?;

    let user = whoami();
    project_db.insert_audit("verify", None, Some(&user), None)?;

    if pending_migration
        && references.is_empty()
        && counts.modified == 0
        && counts.missing.is_empty()
    {
        eprintln!();
        eprintln!("All files verified. Migrating database to content-addressed schema...");
        project_db.finalize_content_addressed_migration()?;
        eprintln!("Migration complete. File paths removed from database.");
    }

    print_summary(&counts)
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

fn print_summary(counts: &VerifyCounts) -> Result<()> {
    if !counts.missing.is_empty() {
        eprintln!();
        eprintln!("{}:", style("Missing files").yellow().bold());
        for ref_str in &counts.missing {
            eprintln!("  {} {}", style("?").yellow(), style(ref_str).yellow());
        }
    }

    let missing_count = counts.missing.len();
    eprintln!();
    if counts.fixed > 0 {
        eprintln!(
            "Verified: {} ok, {} modified, {} missing, {} skipped, {} fixed",
            counts.ok, counts.modified, missing_count, counts.skipped, counts.fixed
        );
    } else {
        eprintln!(
            "Verified: {} ok, {} modified, {} missing, {} skipped",
            counts.ok, counts.modified, missing_count, counts.skipped
        );
    }

    if counts.modified > 0 || missing_count > 0 {
        bail!("integrity check failed");
    }

    Ok(())
}

struct VerifyItem {
    rel_path: String,
    file: TrackedFile,
    /// True when the item was already verified during collection (fingerprint
    /// match, partial match + hash verify, or hash fallback). Skips re-hashing
    /// in `verify_items`.
    pre_verified: bool,
    /// If set, the file's fingerprint in DB should be updated to this value.
    /// Set when a partial match or hash fallback identified a stale/missing
    /// fingerprint that needs correcting.
    updated_fingerprint: Option<String>,
}

struct VerifyCounts {
    ok: u32,
    modified: u32,
    missing: Vec<String>,
    skipped: u32,
    fixed: u32,
}

/// Pre-migration verify: use legacy stored paths to bridge DB records to disk.
/// This is the last time stored paths are used before they're dropped.
fn collect_legacy(vctx: &VerifyCtx<'_>) -> Result<Vec<VerifyItem>> {
    let legacy_files = vctx.db.list_files_with_legacy_paths()?;
    let mut items = Vec::new();

    for (id, rel_path, sha256, fingerprint) in legacy_files {
        items.push(VerifyItem {
            rel_path,
            pre_verified: false,
            updated_fingerprint: None,
            file: TrackedFile {
                id: Some(id),
                name: None,
                path: None,
                sha256,
                fingerprint,
                mime_type: None,
                size: None,
                ingested_at: String::new(),
                provenance: None,
                immutable: false,
            },
        });
    }

    Ok(items)
}

/// Walk the filesystem, fingerprint-first matching against DB.
/// Falls back to partial fingerprint match + hash verify, then hash-only lookup.
/// DB records with no disk match are reported as MISSING.
fn collect_all(vctx: &VerifyCtx<'_>) -> Result<Vec<VerifyItem>> {
    let patterns = walk::category_patterns(vctx.db, None)?;
    let disk_files = walk::walk_and_collect(vctx.root, &patterns)?;

    let all_db_files = vctx.db.list_all_files()?;

    // Pre-parse DB fingerprints for partial matching
    let db_fps: Vec<Option<integrity::Fingerprint>> = all_db_files
        .iter()
        .map(|f| {
            integrity::Fingerprint::from_json(&f.fingerprint)
                .ok()
                .filter(|fp| !fp.is_empty())
        })
        .collect();

    let db_ids_seen = HashSet::new();
    let mut items = Vec::new();

    let mut match_ctx = MatchCtx {
        all_db_files: &all_db_files,
        db_fps: &db_fps,
        db_ids_seen,
    };

    for rel_path in &disk_files {
        let abs_path = vctx.root.join(rel_path);
        let (hash, disk_fp) = integrity::hash_and_fingerprint(&abs_path)?;
        let fp_json = disk_fp.to_json();

        if let Some(item) = match_ctx.match_disk_file(vctx, rel_path, &hash, &disk_fp, &fp_json)? {
            items.push(item);
        }
    }

    collect_missing(&all_db_files, &match_ctx.db_ids_seen, &mut items);
    Ok(items)
}

/// Pre-parsed DB state for matching disk files against tracked records.
struct MatchCtx<'a> {
    all_db_files: &'a [TrackedFile],
    db_fps: &'a [Option<integrity::Fingerprint>],
    db_ids_seen: HashSet<i64>,
}

impl MatchCtx<'_> {
    /// Try to match a disk file against DB records.
    /// Returns `Some(VerifyItem)` if matched, `None` if untracked.
    #[allow(clippy::too_many_arguments)]
    fn match_disk_file(
        &mut self,
        vctx: &VerifyCtx<'_>,
        rel_path: &str,
        hash: &str,
        disk_fp: &integrity::Fingerprint,
        fp_json: &str,
    ) -> Result<Option<VerifyItem>> {
        // 1. Exact fingerprint match — file is unchanged
        if let Some(file) = vctx.db.get_file_by_fingerprint(fp_json)? {
            if let Some(id) = file.id {
                if !self.db_ids_seen.insert(id) {
                    return Ok(None);
                }
            }
            return Ok(Some(VerifyItem {
                rel_path: rel_path.to_string(),
                pre_verified: true,
                updated_fingerprint: None,
                file,
            }));
        }

        // 2. Partial fingerprint match — check if hash confirms identity
        if !disk_fp.is_empty() {
            if let Some(db_idx) =
                best_partial_match(disk_fp, self.all_db_files, self.db_fps, &self.db_ids_seen)
            {
                let candidate = &self.all_db_files[db_idx];
                if candidate.sha256 == hash {
                    if let Some(id) = candidate.id {
                        self.db_ids_seen.insert(id);
                    }
                    return Ok(Some(VerifyItem {
                        rel_path: rel_path.to_string(),
                        pre_verified: true,
                        updated_fingerprint: Some(fp_json.to_string()),
                        file: candidate.clone(),
                    }));
                }
            }
        }

        // 3. Hash fallback — catches records with empty/missing fingerprints
        if let Some(file) = vctx.db.get_file_by_hash(hash)? {
            if let Some(id) = file.id {
                if !self.db_ids_seen.insert(id) {
                    return Ok(None);
                }
            }
            let needs_update = file.fingerprint != fp_json;
            return Ok(Some(VerifyItem {
                rel_path: rel_path.to_string(),
                pre_verified: true,
                updated_fingerprint: if needs_update {
                    Some(fp_json.to_string())
                } else {
                    None
                },
                file,
            }));
        }

        Ok(None)
    }
}

/// Append MISSING items for DB records not found on disk.
fn collect_missing(
    all_db_files: &[TrackedFile],
    db_ids_seen: &HashSet<i64>,
    items: &mut Vec<VerifyItem>,
) {
    for file in all_db_files {
        let id = file.id.unwrap_or(0);
        if id > 0 && !db_ids_seen.contains(&id) {
            let hash_preview = &file.sha256[..file.sha256.len().min(10)];
            items.push(VerifyItem {
                rel_path: format!("[sha256:{hash_preview}...]"),
                pre_verified: false,
                updated_fingerprint: None,
                file: file.clone(),
            });
        }
    }
}

/// Find the DB record with the best partial fingerprint match.
/// Requires >50% of chunks (by the shorter fingerprint's length) to match.
/// Skips records already matched to another disk file.
fn best_partial_match(
    disk_fp: &integrity::Fingerprint,
    all_db_files: &[TrackedFile],
    db_fps: &[Option<integrity::Fingerprint>],
    seen: &HashSet<i64>,
) -> Option<usize> {
    let mut best: Option<(usize, usize)> = None;

    for (i, db_fp) in db_fps.iter().enumerate() {
        let Some(db_fp) = db_fp else { continue };
        let id = all_db_files[i].id.unwrap_or(0);
        if id > 0 && seen.contains(&id) {
            continue;
        }

        let matching = disk_fp.matching_chunks(db_fp);
        let min_len = disk_fp.len().min(db_fp.len());
        if min_len == 0 {
            continue;
        }

        // More than half of the shorter fingerprint's chunks must match
        if matching * 2 > min_len && best.is_none_or(|(_, best_score)| matching > best_score) {
            best = Some((i, matching));
        }
    }

    best.map(|(idx, _)| idx)
}

/// Resolve references and return items with filesystem-derived paths.
fn collect_from_references(
    references: &[String],
    ctx: &crate::context::Context,
    _vctx: &VerifyCtx<'_>,
) -> Result<Vec<VerifyItem>> {
    let parsed: Vec<_> = references
        .iter()
        .map(|r| parse_reference(r))
        .collect::<Result<_>>()?;
    let collection = resolve_references(&parsed, ctx)?;
    if collection.files.is_empty() {
        bail!("references matched no files");
    }
    if collection.files.iter().any(|rf| rf.project_name.is_some()) {
        bail!("verify does not support cross-project references");
    }
    Ok(collection
        .files
        .into_iter()
        .map(|rf| VerifyItem {
            rel_path: rf.rel_path,
            pre_verified: false,
            updated_fingerprint: None,
            file: rf.file,
        })
        .collect())
}

fn verify_items(vctx: &VerifyCtx<'_>, items: &[VerifyItem]) -> Result<VerifyCounts> {
    let mut counts = VerifyCounts {
        ok: 0,
        modified: 0,
        missing: Vec::new(),
        skipped: 0,
        fixed: 0,
    };

    for item in items {
        let abs_path = vctx.root.join(&item.rel_path);
        let result = if item.pre_verified {
            VerifyResult::Ok
        } else {
            integrity::verify_file(&abs_path, &item.file.sha256)?
        };

        print_verify_result(vctx, &result, &abs_path, &item.rel_path, &item.file);
        match result {
            VerifyResult::Ok => {
                counts.ok += 1;
                let file_id = item.file.id.unwrap_or(0);
                if let Some(new_fp) = &item.updated_fingerprint {
                    // Fingerprint update queued during collection (stale or missing)
                    if file_id > 0 {
                        vctx.db.update_file_fingerprint(file_id, new_fp)?;
                        let ref_str = vctx.format_ref(&item.rel_path);
                        eprintln!("  {} {ref_str} updated fingerprint", style("+").cyan());
                        counts.fixed += 1;
                    }
                } else if file_id > 0 && item.file.fingerprint.is_empty() {
                    // Backfill for legacy/reference items with no stored fingerprint
                    let fp = integrity::fingerprint_file(&abs_path)?;
                    vctx.db.update_file_fingerprint(file_id, &fp.to_json())?;
                    let ref_str = vctx.format_ref(&item.rel_path);
                    eprintln!("  {} {ref_str} stored fingerprint", style("+").cyan());
                    counts.fixed += 1;
                }
            }
            VerifyResult::Modified { .. } => counts.modified += 1,
            VerifyResult::Missing => {
                counts.missing.push(vctx.format_ref(&item.rel_path));
            }
        }

        counts.fixed += check_immutable_flag(vctx, &item.file, &abs_path, &item.rel_path)?;
    }

    Ok(counts)
}

fn print_verify_result(
    vctx: &VerifyCtx<'_>,
    result: &VerifyResult,
    abs_path: &Path,
    rel_path: &str,
    file: &TrackedFile,
) {
    let ref_str = vctx.format_ref(rel_path);
    match result {
        VerifyResult::Ok => {
            eprintln!("  {} {ref_str}", style("\u{2713}").green());
        }
        VerifyResult::Modified { expected, actual } => {
            eprintln!(
                "  {} {} MODIFIED",
                style("\u{2717}").red().bold(),
                style(&ref_str).red()
            );
            eprintln!("    expected: {}", style(expected).dim());
            eprintln!("    actual:   {}", style(actual).dim());
            print_chunk_diff(abs_path, Some(file.fingerprint.as_str()));
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

fn check_immutable_flag(
    vctx: &VerifyCtx<'_>,
    file: &TrackedFile,
    file_path: &Path,
    rel_path: &str,
) -> Result<u32> {
    let expected = vctx
        .db
        .resolve_protection_for_file(&file.sha256, rel_path)?;
    let is_immutable = file_path.exists() && integrity::is_immutable(file_path).unwrap_or(false);

    if expected == ProtectionLevel::Immutable {
        ensure_immutable(vctx, file_path, rel_path)
    } else if is_immutable {
        Ok(clear_unexpected_immutable(
            vctx, file_path, rel_path, expected,
        ))
    } else {
        Ok(0)
    }
}

fn ensure_immutable(vctx: &VerifyCtx<'_>, file_path: &Path, rel_path: &str) -> Result<u32> {
    if !file_path.exists() {
        return Ok(0);
    }

    let ref_str = vctx.format_ref(rel_path);
    if !integrity::is_immutable(file_path)? {
        match integrity::set_immutable(file_path) {
            Ok(()) => {
                eprintln!("  {} {ref_str} restored immutable flag", style("+").cyan());
                return Ok(1);
            }
            Err(e) => {
                eprintln!(
                    "  {} {ref_str} failed to restore immutable flag: {e}",
                    style("!").yellow()
                );
            }
        }
    }

    Ok(0)
}

fn clear_unexpected_immutable(
    vctx: &VerifyCtx<'_>,
    file_path: &Path,
    rel_path: &str,
    expected: ProtectionLevel,
) -> u32 {
    let ref_str = vctx.format_ref(rel_path);
    if file_path.exists() {
        if let Err(e) = integrity::clear_immutable(file_path) {
            eprintln!(
                "  {} {ref_str} failed to clear immutable flag: {e}",
                style("!").yellow()
            );
            return 0;
        }
    }
    eprintln!(
        "  {} {ref_str} cleared immutable flag (policy: {expected})",
        style("+").cyan()
    );
    1
}
