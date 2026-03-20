use std::io::{self, IsTerminal, Read as _, Write};
use std::path::Path;

use anyhow::Result;
use console::style;

use crate::context::{discover, Context};
use crate::reference::{parse_reference, resolve_references};
use crate::util::format_size;

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
        let reference = parse_reference(raw_ref)?;
        let collection = resolve_references(&[reference], &ctx)?;

        if collection.files.is_empty() {
            continue;
        }

        let mut first_in_ref = true;
        for rf in &collection.files {
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
                    writeln!(out, "{}", style(&rf.ref_string).bold())?;
                } else {
                    writeln!(out, "{}", rf.ref_string)?;
                }
            }

            let abs_path = resolve_abs_path(&rf.rel_path, &ctx)?;
            dump_content(&mut out, &abs_path, colorize)?;
            total += 1;
        }
    }

    if total == 0 {
        eprintln!("(no files)");
    }

    Ok(())
}

fn resolve_abs_path(rel_path: &str, ctx: &Context) -> Result<std::path::PathBuf> {
    let (project_root, _) = ctx.require_project()?;
    Ok(project_root.join(rel_path))
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
