use std::path::Path;

use anyhow::Result;
use console::style;

use crate::context::discover;
use crate::reference::{parse_reference, resolve_references, Reference};

pub fn run(cwd: &Path, raw_refs: &[String], _no_hash_check: bool) -> Result<()> {
    let ctx = discover(cwd)?;
    let refs = build_refs(raw_refs)?;
    let collection = resolve_references(&refs, &ctx)?;

    if collection.files.is_empty() {
        eprintln!("(no files)");
        return Ok(());
    }

    for rf in &collection.files {
        let file_name = rf.rel_path.rsplit('/').next().unwrap_or(&rf.rel_path);
        let ref_str = &rf.ref_string;
        let hash_prefix = &rf.file.sha256[..rf.file.sha256.len().min(8)];
        println!(
            "   {} {} {}",
            style(file_name).bold(),
            style(ref_str).dim(),
            style(hash_prefix).dim()
        );
    }

    Ok(())
}

fn build_refs(raw_refs: &[String]) -> Result<Vec<Reference>> {
    if raw_refs.is_empty() {
        return Ok(vec![Reference::Context {
            scope: vec![],
            tags: vec![],
            glob: None,
        }]);
    }

    raw_refs.iter().map(|r| parse_reference(r)).collect()
}
