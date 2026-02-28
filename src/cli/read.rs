use std::io::{self, IsTerminal, Read as _, Write};
use std::path::{Path, PathBuf};

use anyhow::Result;
use console::style;

use crate::cli::ingest::track_file;
use crate::cli::list::{category_patterns, matches_tags, walk_collect};
use crate::context::{discover, Context};
use crate::db::ProjectDb;
use crate::reference::{
    expand_reference_scope, parse_reference, ExpandedScope, Reference, ScopeLevel, TagFilter,
};
use crate::util::format_size;

struct ResolvedEntry {
    abs_path: PathBuf,
    display_path: String,
}

pub fn run(
    cwd: &Path,
    raw_refs: &[String],
    path_flag: bool,
    query_flag: bool,
    raw: bool,
) -> Result<()> {
    let is_tty = io::stdout().is_terminal();
    let show_path = !raw && (path_flag || is_tty);
    let show_query = !raw && (query_flag || is_tty);
    let colorize = !raw && is_tty;

    let ctx = discover(cwd)?;
    let mut out = io::stdout().lock();
    let mut total = 0usize;

    for raw_ref in raw_refs {
        let reference = promote_and_parse(raw_ref)?;
        let entries = collect_files(&reference, &ctx)?;

        if entries.is_empty() {
            continue;
        }

        let mut first_in_ref = true;
        for entry in &entries {
            if total > 0 {
                writeln!(out)?;
            }

            if show_query && first_in_ref {
                if colorize {
                    writeln!(out, "{}", style(raw_ref).cyan())?;
                } else {
                    writeln!(out, "{raw_ref}")?;
                }
                first_in_ref = false;
            }

            if show_path {
                if colorize {
                    writeln!(out, "{}", style(&entry.display_path).bold())?;
                } else {
                    writeln!(out, "{}", entry.display_path)?;
                }
            }

            dump_content(&mut out, &entry.abs_path, colorize)?;
            total += 1;
        }
    }

    if total == 0 {
        eprintln!("(no files)");
    }

    Ok(())
}

fn promote_and_parse(raw_ref: &str) -> Result<Reference> {
    if !raw_ref.starts_with(':') && !raw_ref.contains('/') {
        parse_reference(&format!(":{raw_ref}"))
    } else {
        parse_reference(raw_ref)
    }
}

fn collect_files(reference: &Reference, ctx: &Context) -> Result<Vec<ResolvedEntry>> {
    match reference {
        Reference::BarePath(p) => collect_bare_path(p, ctx),
        Reference::Structured { scope, tags, glob } => {
            collect_structured(scope, tags, glob.as_deref(), ctx)
        }
    }
}

fn collect_bare_path(path: &str, ctx: &Context) -> Result<Vec<ResolvedEntry>> {
    let (project_root, project_db) = ctx.require_project()?;
    let abs_path = project_root.join(path);
    if !abs_path.exists() {
        return Ok(vec![]);
    }

    auto_ingest(project_db, &abs_path, path);

    Ok(vec![ResolvedEntry {
        abs_path,
        display_path: path.to_string(),
    }])
}

fn collect_structured(
    scope: &[ScopeLevel],
    tags: &[TagFilter],
    glob: Option<&str>,
    ctx: &Context,
) -> Result<Vec<ResolvedEntry>> {
    let targets = expand_reference_scope(scope, ctx)?;
    let glob_pattern = glob.map(glob::Pattern::new).transpose()?;

    let mut entries = Vec::new();
    for target in &targets {
        entries.extend(collect_target(target, tags, glob_pattern.as_ref())?);
    }
    Ok(entries)
}

fn collect_target(
    target: &ExpandedScope,
    tags: &[TagFilter],
    glob_filter: Option<&glob::Pattern>,
) -> Result<Vec<ResolvedEntry>> {
    let db = ProjectDb::open(&target.project_root.join(".mkrk"))?;
    let patterns = category_patterns(&db, target.category_name.as_deref())?;

    let mut paths = Vec::new();
    walk_collect(
        &target.project_root,
        &target.project_root,
        &patterns,
        &mut paths,
    )?;
    paths.sort();

    let mut entries = Vec::new();
    for rel_path in &paths {
        if let Some(pattern) = glob_filter {
            let file_name = Path::new(rel_path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");
            if !pattern.matches(file_name) && !pattern.matches(rel_path.as_str()) {
                continue;
            }
        }

        let abs_path = target.project_root.join(rel_path);
        auto_ingest(&db, &abs_path, rel_path);

        if !tags.is_empty() && !matches_tags(&db, rel_path, tags)? {
            continue;
        }

        let display_path = match &target.project_name {
            Some(proj) => format!("{proj}:{rel_path}"),
            None => rel_path.clone(),
        };

        entries.push(ResolvedEntry {
            abs_path,
            display_path,
        });
    }

    Ok(entries)
}

fn auto_ingest(db: &ProjectDb, abs_path: &Path, rel_path: &str) {
    if db.get_file_by_path(rel_path).ok().flatten().is_none() {
        let _ = track_file(db, abs_path, rel_path);
    }
}

fn dump_content(out: &mut impl Write, path: &Path, colorize: bool) -> Result<()> {
    let file = std::fs::File::open(path)?;
    let size = file.metadata()?.len();
    let mut reader = io::BufReader::new(file);

    let check_size = size.min(8192) as usize;
    let mut header = vec![0u8; check_size];
    let n = reader.read(&mut header)?;
    header.truncate(n);

    if header.contains(&0) {
        let size_str = format_size(size as i64);
        let msg = format!("(binary file, {size_str})");
        if colorize {
            writeln!(out, "{}", style(msg).dim())?;
        } else {
            writeln!(out, "{msg}")?;
        }
        return Ok(());
    }

    out.write_all(&header)?;
    let mut last_byte = header.last().copied();

    let mut buf = [0u8; 8192];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n])?;
        last_byte = Some(buf[n - 1]);
    }

    if last_byte.is_some_and(|b| b != b'\n') {
        writeln!(out)?;
    }

    Ok(())
}
