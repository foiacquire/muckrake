use std::path::Path;
use std::process::Command;

use anyhow::{bail, Result};
use chrono::Utc;
use console::style;

use crate::context::{discover, Context};
use crate::db::ProjectDb;
use crate::integrity;
use crate::models::{Category, Pipeline, Sign, TrackedFile, TriggerEvent};
use crate::pipeline::state::derive_file_state;
use crate::reference::{parse_reference, resolve_references};
use crate::rules::RuleEvent;
use crate::util::whoami;

pub fn run_sign(
    cwd: &Path,
    reference: &str,
    sign_name: &str,
    pipeline_name: Option<&str>,
    gpg: bool,
) -> Result<()> {
    let ctx = discover(cwd)?;
    let (project_root, project_db) = ctx.require_project()?;

    let parsed = parse_reference(reference)?;
    let collection = resolve_references(&[parsed], &ctx)?;
    let resolved = collection.expect_one(reference)?;
    let file = resolved.file;
    let file_id = file.id.ok_or_else(|| anyhow::anyhow!("file has no id"))?;

    let current_hash = rehash_file(project_root, &file)?;

    let categories = project_db.list_categories()?;
    let tags = project_db.get_tags(file_id)?;
    let pipelines = project_db.get_pipelines_for_file(file_id, &file.path, &categories, &tags)?;

    let pipeline = resolve_single_pipeline(&pipelines, pipeline_name, &file.path)?;
    let pipeline_id = pipeline.id.unwrap();

    validate_sign_name_for_pipeline(sign_name, pipeline)?;

    let signature = if gpg {
        let file_path = project_root.join(&file.path);
        Some(create_gpg_signature(&file_path)?)
    } else {
        None
    };

    let old_state = derive_pipeline_state(project_db, file_id, pipeline, &current_hash)?;

    let sign = build_sign(pipeline_id, file_id, &current_hash, sign_name, signature);
    project_db.insert_sign(&sign)?;
    audit_sign(project_db, file_id, &sign.signer, &pipeline.name, sign_name)?;

    eprintln!(
        "Signed '{}' as '{}' in pipeline '{}'",
        file.path, sign_name, pipeline.name
    );

    let new_state = derive_pipeline_state(project_db, file_id, pipeline, &current_hash)?;

    fire_pipeline_rule(&ctx, &file, &pipeline.name, Some(sign_name), &new_state);
    if old_state != new_state {
        fire_pipeline_rule(&ctx, &file, &pipeline.name, None, &new_state);
    }

    Ok(())
}

pub fn run_unsign(
    cwd: &Path,
    reference: &str,
    sign_name: &str,
    pipeline_name: Option<&str>,
) -> Result<()> {
    let ctx = discover(cwd)?;
    let (project_root, project_db) = ctx.require_project()?;

    let parsed = parse_reference(reference)?;
    let collection = resolve_references(&[parsed], &ctx)?;
    let resolved = collection.expect_one(reference)?;
    let file = resolved.file;
    let file_id = file.id.ok_or_else(|| anyhow::anyhow!("file has no id"))?;

    let categories = project_db.list_categories()?;
    let tags = project_db.get_tags(file_id)?;
    let pipelines = project_db.get_pipelines_for_file(file_id, &file.path, &categories, &tags)?;

    let pipeline = resolve_single_pipeline(&pipelines, pipeline_name, &file.path)?;
    let pipeline_id = pipeline.id.unwrap();

    let sign = project_db
        .find_sign(file_id, pipeline_id, sign_name)?
        .ok_or_else(|| {
            anyhow::anyhow!(
                "no active sign '{}' for '{}' in pipeline '{}'",
                sign_name,
                file.path,
                pipeline.name
            )
        })?;

    let current_hash = current_file_hash(project_root, &file);
    let old_state = derive_pipeline_state(project_db, file_id, pipeline, &current_hash)?;

    let now = Utc::now().to_rfc3339();
    project_db.revoke_sign(sign.id.unwrap(), &now)?;
    audit_sign(project_db, file_id, &whoami(), &pipeline.name, sign_name)?;

    eprintln!(
        "Revoked sign '{}' for '{}' in pipeline '{}'",
        sign_name, file.path, pipeline.name
    );

    let new_state = derive_pipeline_state(project_db, file_id, pipeline, &current_hash)?;
    if old_state != new_state {
        fire_pipeline_rule(&ctx, &file, &pipeline.name, None, &new_state);
    }

    Ok(())
}

pub fn run_signs(cwd: &Path, reference: Option<&str>) -> Result<()> {
    let ctx = discover(cwd)?;
    let (_, project_db) = ctx.require_project()?;
    let files = resolve_files_with_ids(reference, &ctx, project_db)?;

    let mut any_signs = false;
    for (file, file_id) in &files {
        let signs = project_db.get_signs_for_file(*file_id)?;
        if signs.is_empty() {
            continue;
        }
        any_signs = true;

        println!("{}", style(&file.path).bold());
        for sign in &signs {
            print_sign_detail(project_db, sign, file);
        }
    }

    if !any_signs {
        eprintln!("No signs found");
    }

    Ok(())
}

pub fn run_state(cwd: &Path, reference: Option<&str>, pipeline_name: Option<&str>) -> Result<()> {
    let ctx = discover(cwd)?;
    let (project_root, project_db) = ctx.require_project()?;
    let files = resolve_files_with_ids(reference, &ctx, project_db)?;
    let categories = project_db.list_categories()?;
    let mut any_state = false;

    for (file, file_id) in &files {
        let pipelines =
            resolve_file_pipelines(project_db, *file_id, file, &categories, pipeline_name)?;

        if pipelines.is_empty() {
            continue;
        }
        any_state = true;

        let hash = current_file_hash(project_root, file);

        println!("{}", style(&file.path).bold());
        for pipeline in &pipelines {
            print_pipeline_state(project_db, *file_id, pipeline, &hash)?;
        }
    }

    if !any_state {
        eprintln!("No pipeline state for the given reference");
    }

    Ok(())
}

fn resolve_files_with_ids(
    reference: Option<&str>,
    ctx: &Context,
    project_db: &ProjectDb,
) -> Result<Vec<(TrackedFile, i64)>> {
    if let Some(r) = reference {
        let parsed = parse_reference(r)?;
        let collection = resolve_references(&[parsed], ctx)?;
        Ok(collection
            .files
            .into_iter()
            .filter_map(|rf| {
                let id = rf.file.id?;
                Some((rf.file, id))
            })
            .collect())
    } else {
        let all = project_db.list_files(None)?;
        Ok(all
            .into_iter()
            .filter_map(|f| {
                let id = f.id?;
                Some((f, id))
            })
            .collect())
    }
}

fn resolve_file_pipelines(
    project_db: &ProjectDb,
    file_id: i64,
    file: &TrackedFile,
    categories: &[Category],
    pipeline_name: Option<&str>,
) -> Result<Vec<Pipeline>> {
    let tags = project_db.get_tags(file_id)?;
    let mut pipelines =
        project_db.get_pipelines_for_file(file_id, &file.path, categories, &tags)?;
    if let Some(name) = pipeline_name {
        pipelines.retain(|p| p.name == name);
    }
    Ok(pipelines)
}

fn current_file_hash(project_root: &Path, file: &TrackedFile) -> String {
    let current_hash = file.sha256.clone().unwrap_or_default();
    let file_path = project_root.join(&file.path);
    if file_path.exists() {
        integrity::hash_file(&file_path).unwrap_or(current_hash)
    } else {
        current_hash
    }
}

fn print_sign_detail(project_db: &ProjectDb, sign: &Sign, file: &TrackedFile) {
    let pipeline = project_db
        .get_pipeline_by_id(sign.pipeline_id)
        .ok()
        .flatten()
        .map_or_else(|| format!("pipeline:{}", sign.pipeline_id), |p| p.name);

    let status = if sign.revoked_at.is_some() {
        style("revoked").red().to_string()
    } else if file.sha256.as_deref() != Some(&sign.file_hash) {
        style("stale").yellow().to_string()
    } else {
        style("valid").green().to_string()
    };

    println!(
        "  {} {} by {} at {} [{}]",
        style(&sign.sign_name).cyan(),
        style(&pipeline).dim(),
        sign.signer,
        style(&sign.signed_at).dim(),
        status
    );
}

fn print_pipeline_state(
    project_db: &ProjectDb,
    file_id: i64,
    pipeline: &Pipeline,
    hash: &str,
) -> Result<()> {
    let signs = project_db.get_signs_for_file(file_id)?;
    let pipeline_signs: Vec<_> = signs
        .into_iter()
        .filter(|s| s.pipeline_id == pipeline.id.unwrap())
        .collect();

    let state = derive_file_state(pipeline, &pipeline_signs, hash);

    print!(
        "  {}: {}",
        style(&pipeline.name).cyan(),
        style(&state.current_state).bold()
    );

    if !state.stale_signs.is_empty() {
        print!(
            " {}",
            style(format!("(stale: {})", state.stale_signs.join(", "))).yellow()
        );
    }

    println!();
    Ok(())
}

fn resolve_single_pipeline<'a>(
    pipelines: &'a [Pipeline],
    name: Option<&str>,
    file_path: &str,
) -> Result<&'a Pipeline> {
    match name {
        Some(n) => pipelines
            .iter()
            .find(|p| p.name == n)
            .ok_or_else(|| anyhow::anyhow!("pipeline '{n}' is not attached to '{file_path}'")),
        None => match pipelines.len() {
            0 => bail!("no pipelines are attached to '{file_path}'"),
            1 => Ok(&pipelines[0]),
            _ => {
                let names: Vec<&str> = pipelines.iter().map(|p| p.name.as_str()).collect();
                bail!(
                    "file '{}' is in multiple pipelines ({}); use --pipeline to specify",
                    file_path,
                    names.join(", ")
                );
            }
        },
    }
}

fn validate_sign_name_for_pipeline(sign_name: &str, pipeline: &Pipeline) -> Result<()> {
    let valid_names = pipeline.required_sign_names();
    if valid_names.contains(&sign_name) {
        return Ok(());
    }
    bail!(
        "sign name '{}' is not used by any transition in pipeline '{}' (valid: {})",
        sign_name,
        pipeline.name,
        valid_names.join(", ")
    );
}

fn rehash_file(project_root: &Path, file: &TrackedFile) -> Result<String> {
    let file_path = project_root.join(&file.path);
    if !file_path.exists() {
        bail!("file not found: {}", file.path);
    }
    integrity::hash_file(&file_path)
}

fn derive_pipeline_state(
    project_db: &ProjectDb,
    file_id: i64,
    pipeline: &Pipeline,
    current_hash: &str,
) -> Result<String> {
    let signs = project_db.get_signs_for_file(file_id)?;
    let pipeline_signs: Vec<_> = signs
        .iter()
        .filter(|s| s.pipeline_id == pipeline.id.unwrap())
        .cloned()
        .collect();
    Ok(derive_file_state(pipeline, &pipeline_signs, current_hash).current_state)
}

fn fire_pipeline_rule(
    ctx: &Context,
    file: &TrackedFile,
    pipeline_name: &str,
    sign_name: Option<&str>,
    new_state: &str,
) {
    let event = if sign_name.is_some() {
        TriggerEvent::Sign
    } else {
        TriggerEvent::StateChange
    };
    let rule_event = RuleEvent {
        event,
        file: Some(file),
        file_id: file.id,
        rel_path: Some(&file.path),
        tag_name: None,
        target_category: None,
        pipeline_name: Some(pipeline_name),
        sign_name,
        new_state: Some(new_state),
    };
    crate::rules::fire_rules(ctx, &rule_event);
}

fn build_sign(
    pipeline_id: i64,
    file_id: i64,
    current_hash: &str,
    sign_name: &str,
    signature: Option<String>,
) -> Sign {
    Sign {
        id: None,
        pipeline_id,
        file_id,
        file_hash: current_hash.to_string(),
        sign_name: sign_name.to_string(),
        signer: whoami(),
        signed_at: Utc::now().to_rfc3339(),
        signature,
        revoked_at: None,
        source: None,
    }
}

fn audit_sign(
    project_db: &ProjectDb,
    file_id: i64,
    signer: &str,
    pipeline_name: &str,
    sign_name: &str,
) -> Result<()> {
    project_db.insert_audit(
        "sign",
        Some(file_id),
        Some(signer),
        Some(
            &serde_json::json!({
                "pipeline": pipeline_name,
                "sign_name": sign_name,
            })
            .to_string(),
        ),
    )
}

fn create_gpg_signature(path: &Path) -> Result<String> {
    let output = Command::new("gpg")
        .args(["--detach-sign", "--armor", "--output", "-"])
        .arg(path)
        .output()
        .map_err(|e| anyhow::anyhow!("failed to run gpg: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("gpg signing failed: {stderr}");
    }

    Ok(String::from_utf8(output.stdout)?)
}
