use std::collections::HashSet;
use std::path::Path;

use anyhow::{bail, Result};
use chrono::Utc;
use console::style;

use crate::db::{ProjectDb, WorkspaceDb};
use crate::integrity;
use crate::models::{
    ActionType, AttachmentScope, Rule, Sign, TrackedFile, TriggerEvent, TriggerFilter,
};
use crate::pipeline::state::derive_file_state;
use crate::tools::{execute_tool, ExecuteToolParams};
use crate::util::whoami;

pub struct RuleContext<'a> {
    pub project_root: &'a Path,
    pub project_db: &'a ProjectDb,
    pub workspace_root: Option<&'a Path>,
    pub workspace_db: Option<&'a WorkspaceDb>,
}

pub struct RuleEvent<'a> {
    pub event: TriggerEvent,
    pub file: Option<&'a TrackedFile>,
    pub file_id: Option<i64>,
    pub rel_path: Option<&'a str>,
    pub tag_name: Option<&'a str>,
    pub target_category: Option<&'a str>,
    pub pipeline_name: Option<&'a str>,
    pub sign_name: Option<&'a str>,
    pub new_state: Option<&'a str>,
}

pub fn evaluate_rules(
    event: &RuleEvent<'_>,
    ctx: &RuleContext<'_>,
    fired: &mut HashSet<i64>,
) -> Result<()> {
    let rules = ctx.project_db.get_matching_rules(event.event)?;
    if rules.is_empty() {
        return Ok(());
    }

    for rule in &rules {
        let rule_id = rule.id.unwrap_or(0);
        if fired.contains(&rule_id) {
            continue;
        }

        if !matches_filter(&rule.trigger_filter, event, ctx.project_db) {
            continue;
        }

        fired.insert(rule_id);

        eprintln!("  {} rule '{}' triggered", style("→").dim(), rule.name,);

        match execute_action(rule, event, ctx, fired) {
            Ok(()) => audit_rule(ctx.project_db, event, rule),
            Err(e) => {
                eprintln!("  {} rule '{}' failed: {e}", style("✗").red(), rule.name,);
            }
        }
    }

    Ok(())
}

fn audit_rule(db: &ProjectDb, event: &RuleEvent<'_>, rule: &Rule) {
    let user = whoami();
    let detail = serde_json::json!({
        "rule": rule.name,
        "trigger": event.event.to_string(),
        "action": rule.action_type.to_string(),
    });
    let _ = db.insert_audit(
        "rule",
        event.file_id,
        Some(&user),
        Some(&detail.to_string()),
    );
}

fn matches_filter(filter: &TriggerFilter, event: &RuleEvent<'_>, db: &ProjectDb) -> bool {
    if let Some(ref cat_name) = filter.category {
        let Some(rel_path) = event.rel_path else {
            return false;
        };
        if !matches_category(cat_name, rel_path, db) {
            return false;
        }
    }

    if let Some(ref mime) = filter.mime_type {
        let Some(file) = event.file else {
            return false;
        };
        let file_mime = file.mime_type.as_deref().unwrap_or("");
        if !matches_mime(mime, file_mime) {
            return false;
        }
    }

    if let Some(ref ft) = filter.file_type {
        let Some(rel_path) = event.rel_path else {
            return false;
        };
        let ext = file_extension(rel_path);
        if !ext.eq_ignore_ascii_case(ft) {
            return false;
        }
    }

    matches_optional(filter.tag_name.as_deref(), event.tag_name)
        && matches_optional(filter.pipeline.as_deref(), event.pipeline_name)
        && matches_optional(filter.sign_name.as_deref(), event.sign_name)
        && matches_optional(filter.state.as_deref(), event.new_state)
}

fn matches_optional(filter_value: Option<&str>, event_value: Option<&str>) -> bool {
    match filter_value {
        Some(expected) => event_value.unwrap_or("") == expected,
        None => true,
    }
}

fn matches_category(cat_name: &str, rel_path: &str, db: &ProjectDb) -> bool {
    let Ok(Some(cat)) = db.get_category_by_name(cat_name) else {
        return false;
    };
    cat.matches(rel_path).unwrap_or(false)
}

fn matches_mime(filter_mime: &str, file_mime: &str) -> bool {
    if filter_mime == file_mime {
        return true;
    }
    if filter_mime.ends_with('/') {
        return file_mime.starts_with(filter_mime);
    }
    if filter_mime.ends_with("/*") {
        let prefix = &filter_mime[..filter_mime.len() - 1];
        return file_mime.starts_with(prefix);
    }
    false
}

fn file_extension(path: &str) -> &str {
    path.rsplit('.').next().unwrap_or("")
}

fn execute_action(
    rule: &Rule,
    event: &RuleEvent<'_>,
    ctx: &RuleContext<'_>,
    fired: &mut HashSet<i64>,
) -> Result<()> {
    match rule.action_type {
        ActionType::RunTool => action_run_tool(rule, event, ctx),
        ActionType::AddTag => action_add_tag(rule, event, ctx, fired),
        ActionType::RemoveTag => action_remove_tag(rule, event, ctx, fired),
        ActionType::Sign => action_sign(rule, event, ctx, fired),
        ActionType::Unsign => action_unsign(rule, event, ctx, fired),
        ActionType::AttachPipeline => action_attach_pipeline(rule, ctx),
        ActionType::DetachPipeline => action_detach_pipeline(rule, ctx),
    }
}

fn require_file_context<'a>(event: &'a RuleEvent<'_>) -> Result<(&'a TrackedFile, i64, &'a str)> {
    let file = event
        .file
        .ok_or_else(|| anyhow::anyhow!("action requires file context"))?;
    let file_id = event
        .file_id
        .ok_or_else(|| anyhow::anyhow!("action requires file id"))?;
    let rel_path = event
        .rel_path
        .ok_or_else(|| anyhow::anyhow!("action requires file path"))?;
    Ok((file, file_id, rel_path))
}

fn action_run_tool(rule: &Rule, event: &RuleEvent<'_>, ctx: &RuleContext<'_>) -> Result<()> {
    let tool_name = rule.action_config.tool.as_deref().unwrap_or("unknown");

    if let (Some(file), Some(file_id), Some(rel_path)) = (event.file, event.file_id, event.rel_path)
    {
        let ext = file_extension(rel_path);
        let tags = ctx.project_db.get_tags(file_id).unwrap_or_default();
        let abs_path = ctx.project_root.join(rel_path);

        let params = ExecuteToolParams {
            tool_name,
            file_abs_path: Some(&abs_path),
            file_rel_path: Some(rel_path),
            file_ext: Some(ext),
            tags: &tags,
            project_root: ctx.project_root,
            project_db: ctx.project_db,
            workspace_root: ctx.workspace_root,
            workspace_db: ctx.workspace_db,
        };
        execute_tool(&params)?;
        eprintln!("    ran tool '{tool_name}' on '{}'", file.name);
    } else {
        let params = ExecuteToolParams {
            tool_name,
            file_abs_path: None,
            file_rel_path: None,
            file_ext: None,
            tags: &[],
            project_root: ctx.project_root,
            project_db: ctx.project_db,
            workspace_root: ctx.workspace_root,
            workspace_db: ctx.workspace_db,
        };
        execute_tool(&params)?;
        eprintln!("    ran tool '{tool_name}' (no file context)");
    }
    Ok(())
}

fn cascade_tag_event(
    event: &RuleEvent<'_>,
    trigger: TriggerEvent,
    tag: &str,
    ctx: &RuleContext<'_>,
    fired: &mut HashSet<i64>,
) -> Result<()> {
    let cascaded = RuleEvent {
        event: trigger,
        file: event.file,
        file_id: event.file_id,
        rel_path: event.rel_path,
        tag_name: Some(tag),
        target_category: None,
        pipeline_name: None,
        sign_name: None,
        new_state: None,
    };
    evaluate_rules(&cascaded, ctx, fired)
}

fn action_add_tag(
    rule: &Rule,
    event: &RuleEvent<'_>,
    ctx: &RuleContext<'_>,
    fired: &mut HashSet<i64>,
) -> Result<()> {
    let (file, file_id, rel_path) = require_file_context(event)?;
    let tag = rule.action_config.tag.as_deref().unwrap_or("unknown");
    let abs_path = ctx.project_root.join(rel_path);
    let hash = integrity::hash_file(&abs_path)?;
    ctx.project_db.insert_tag(file_id, tag, &hash)?;
    eprintln!("    tagged '{}' with '{tag}'", file.name);
    cascade_tag_event(event, TriggerEvent::Tag, tag, ctx, fired)
}

fn action_remove_tag(
    rule: &Rule,
    event: &RuleEvent<'_>,
    ctx: &RuleContext<'_>,
    fired: &mut HashSet<i64>,
) -> Result<()> {
    let (file, file_id, _) = require_file_context(event)?;
    let tag = rule.action_config.tag.as_deref().unwrap_or("unknown");
    ctx.project_db.remove_tag(file_id, tag)?;
    eprintln!("    untagged '{}' from '{tag}'", file.name);
    cascade_tag_event(event, TriggerEvent::Untag, tag, ctx, fired)
}

fn action_sign(
    rule: &Rule,
    event: &RuleEvent<'_>,
    ctx: &RuleContext<'_>,
    fired: &mut HashSet<i64>,
) -> Result<()> {
    let (file, file_id, rel_path) = require_file_context(event)?;
    let (pipeline_name, sign_name) = require_pipeline_sign_config(&rule.action_config, "sign")?;

    let pipeline = lookup_pipeline(ctx.project_db, pipeline_name)?;
    let pipeline_id = pipeline.id.unwrap();

    let abs_path = ctx.project_root.join(rel_path);
    if !abs_path.exists() {
        bail!("file not found: {rel_path}");
    }
    let current_hash = integrity::hash_file(&abs_path)?;

    let old_state = pipeline_file_state(ctx.project_db, file_id, &pipeline, &current_hash)?;

    let sign = Sign {
        id: None,
        pipeline_id,
        file_id,
        file_hash: current_hash.clone(),
        sign_name: sign_name.to_string(),
        signer: whoami(),
        signed_at: Utc::now().to_rfc3339(),
        signature: None,
        revoked_at: None,
        source: Some(format!("rule:{}", rule.name)),
    };
    ctx.project_db.insert_sign(&sign)?;
    eprintln!(
        "    signed '{}' as '{sign_name}' in '{pipeline_name}'",
        file.name
    );

    let new_state = pipeline_file_state(ctx.project_db, file_id, &pipeline, &current_hash)?;

    let mut cascade_args = CascadeArgs {
        event,
        pipeline_name,
        new_state: &new_state,
        ctx,
        fired,
    };
    fire_pipeline_cascade(&mut cascade_args, Some(sign_name))?;
    if old_state != new_state {
        fire_pipeline_cascade(&mut cascade_args, None)?;
    }

    Ok(())
}

fn action_unsign(
    rule: &Rule,
    event: &RuleEvent<'_>,
    ctx: &RuleContext<'_>,
    fired: &mut HashSet<i64>,
) -> Result<()> {
    let (file, file_id, rel_path) = require_file_context(event)?;
    let (pipeline_name, sign_name) = require_pipeline_sign_config(&rule.action_config, "unsign")?;

    let pipeline = lookup_pipeline(ctx.project_db, pipeline_name)?;
    let pipeline_id = pipeline.id.unwrap();

    let existing = ctx.project_db.find_sign(file_id, pipeline_id, sign_name)?;
    let Some(existing) = existing else {
        eprintln!(
            "    no active sign '{sign_name}' for '{}' in '{pipeline_name}'",
            file.name
        );
        return Ok(());
    };

    let abs_path = ctx.project_root.join(rel_path);
    let current_hash = if abs_path.exists() {
        integrity::hash_file(&abs_path)?
    } else {
        file.sha256.clone().unwrap_or_default()
    };

    let old_state = pipeline_file_state(ctx.project_db, file_id, &pipeline, &current_hash)?;

    let now = Utc::now().to_rfc3339();
    ctx.project_db.revoke_sign(existing.id.unwrap(), &now)?;
    eprintln!(
        "    revoked sign '{sign_name}' for '{}' in '{pipeline_name}'",
        file.name
    );

    let new_state = pipeline_file_state(ctx.project_db, file_id, &pipeline, &current_hash)?;
    if old_state != new_state {
        let mut ca = CascadeArgs {
            event,
            pipeline_name,
            new_state: &new_state,
            ctx,
            fired,
        };
        fire_pipeline_cascade(&mut ca, None)?;
    }

    Ok(())
}

fn action_attach_pipeline(rule: &Rule, ctx: &RuleContext<'_>) -> Result<()> {
    let pipeline_name = rule
        .action_config
        .pipeline
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("attach-pipeline action requires pipeline"))?;
    let pipeline = ctx
        .project_db
        .get_pipeline_by_name(pipeline_name)?
        .ok_or_else(|| anyhow::anyhow!("pipeline '{pipeline_name}' not found"))?;
    let pipeline_id = pipeline.id.unwrap();

    let (scope_type, scope_value) = resolve_pipeline_scope(&rule.action_config)?;

    match ctx
        .project_db
        .attach_pipeline(pipeline_id, scope_type, scope_value)
    {
        Ok(_) => eprintln!("    attached pipeline '{pipeline_name}' to {scope_type}:{scope_value}"),
        Err(_) => eprintln!(
            "    pipeline '{pipeline_name}' already attached to {scope_type}:{scope_value}"
        ),
    }
    Ok(())
}

fn action_detach_pipeline(rule: &Rule, ctx: &RuleContext<'_>) -> Result<()> {
    let pipeline_name = rule
        .action_config
        .pipeline
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("detach-pipeline action requires pipeline"))?;
    let pipeline = ctx
        .project_db
        .get_pipeline_by_name(pipeline_name)?
        .ok_or_else(|| anyhow::anyhow!("pipeline '{pipeline_name}' not found"))?;
    let pipeline_id = pipeline.id.unwrap();

    let (scope_type, scope_value) = resolve_pipeline_scope(&rule.action_config)?;

    let removed = ctx
        .project_db
        .detach_pipeline(pipeline_id, scope_type, scope_value)?;
    if removed > 0 {
        eprintln!("    detached pipeline '{pipeline_name}' from {scope_type}:{scope_value}");
    }
    Ok(())
}

fn resolve_pipeline_scope(config: &crate::models::ActionConfig) -> Result<(AttachmentScope, &str)> {
    if let Some(ref cat) = config.category {
        return Ok((AttachmentScope::Category, cat));
    }
    if let Some(ref tag) = config.tag {
        return Ok((AttachmentScope::Tag, tag));
    }
    bail!("attach/detach-pipeline requires --category or --tag scope")
}

struct CascadeArgs<'a, 'b> {
    event: &'a RuleEvent<'b>,
    pipeline_name: &'a str,
    new_state: &'a str,
    ctx: &'a RuleContext<'a>,
    fired: &'a mut HashSet<i64>,
}

fn fire_pipeline_cascade(args: &mut CascadeArgs<'_, '_>, sign_name: Option<&str>) -> Result<()> {
    let trigger = if sign_name.is_some() {
        TriggerEvent::Sign
    } else {
        TriggerEvent::StateChange
    };
    let cascaded = RuleEvent {
        event: trigger,
        file: args.event.file,
        file_id: args.event.file_id,
        rel_path: args.event.rel_path,
        tag_name: None,
        target_category: None,
        pipeline_name: Some(args.pipeline_name),
        sign_name,
        new_state: Some(args.new_state),
    };
    evaluate_rules(&cascaded, args.ctx, args.fired)
}

fn require_pipeline_sign_config<'a>(
    config: &'a crate::models::ActionConfig,
    action_name: &str,
) -> Result<(&'a str, &'a str)> {
    let pipeline = config
        .pipeline
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("{action_name} action requires pipeline"))?;
    let sign_name = config
        .sign_name
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("{action_name} action requires sign_name"))?;
    Ok((pipeline, sign_name))
}

fn lookup_pipeline(db: &ProjectDb, name: &str) -> Result<crate::models::Pipeline> {
    db.get_pipeline_by_name(name)?
        .ok_or_else(|| anyhow::anyhow!("pipeline '{name}' not found"))
}

fn pipeline_file_state(
    db: &ProjectDb,
    file_id: i64,
    pipeline: &crate::models::Pipeline,
    current_hash: &str,
) -> Result<String> {
    let signs = db.get_signs_for_file(file_id)?;
    let pipeline_signs: Vec<_> = signs
        .iter()
        .filter(|s| s.pipeline_id == pipeline.id.unwrap())
        .cloned()
        .collect();
    Ok(derive_file_state(pipeline, &pipeline_signs, current_hash).current_state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ActionConfig;

    fn setup_db() -> (tempfile::TempDir, ProjectDb) {
        let dir = tempfile::TempDir::new().unwrap();
        let db = ProjectDb::create(&dir.path().join(".mkrk")).unwrap();
        (dir, db)
    }

    fn make_test_file(name: &str, path: &str, mime_type: Option<&str>) -> TrackedFile {
        TrackedFile {
            id: Some(1),
            name: name.to_string(),
            path: path.to_string(),
            sha256: None,
            mime_type: mime_type.map(String::from),
            size: None,
            ingested_at: String::new(),
            provenance: None,
            immutable: false,
        }
    }

    fn make_event<'a>(
        file: &'a TrackedFile,
        file_id: i64,
        rel_path: &'a str,
        event: TriggerEvent,
        tag_name: Option<&'a str>,
    ) -> RuleEvent<'a> {
        RuleEvent {
            event,
            file: Some(file),
            file_id: Some(file_id),
            rel_path: Some(rel_path),
            tag_name,
            target_category: None,
            pipeline_name: None,
            sign_name: None,
            new_state: None,
        }
    }

    fn make_db_file(name: &str, path: &str, mime_type: Option<&str>, size: i64) -> TrackedFile {
        TrackedFile {
            id: None,
            name: name.to_string(),
            path: path.to_string(),
            sha256: None,
            mime_type: mime_type.map(String::from),
            size: Some(size),
            ingested_at: String::new(),
            provenance: None,
            immutable: false,
        }
    }

    fn tag_action(tag: &str) -> ActionConfig {
        ActionConfig {
            tool: None,
            tag: Some(tag.to_string()),
            pipeline: None,
            sign_name: None,
            category: None,
        }
    }

    fn make_rule(
        name: &str,
        trigger: TriggerEvent,
        action_type: ActionType,
        action_config: ActionConfig,
    ) -> Rule {
        Rule {
            id: None,
            name: name.to_string(),
            enabled: true,
            trigger_event: trigger,
            trigger_filter: TriggerFilter::default(),
            action_type,
            action_config,
            priority: 0,
            created_at: String::new(),
        }
    }

    fn make_ctx<'a>(dir: &'a tempfile::TempDir, db: &'a ProjectDb) -> RuleContext<'a> {
        RuleContext {
            project_root: dir.path(),
            project_db: db,
            workspace_root: None,
            workspace_db: None,
        }
    }

    fn setup_file_env_with(
        name: &str,
        content: &[u8],
        mime: Option<&str>,
    ) -> (tempfile::TempDir, ProjectDb, i64) {
        let (dir, db) = setup_db();
        std::fs::write(dir.path().join(name), content).unwrap();
        let tracked = make_db_file(name, name, mime, content.len() as i64);
        let file_id = db.insert_file(&tracked).unwrap();
        (dir, db, file_id)
    }

    fn run_rule_eval(
        dir: &tempfile::TempDir,
        db: &ProjectDb,
        event: &RuleEvent<'_>,
    ) -> HashSet<i64> {
        let ctx = make_ctx(dir, db);
        let mut fired = HashSet::new();
        evaluate_rules(event, &ctx, &mut fired).unwrap();
        fired
    }

    #[test]
    fn mime_exact_match() {
        assert!(matches_mime("application/pdf", "application/pdf"));
        assert!(!matches_mime("application/pdf", "image/jpeg"));
    }

    #[test]
    fn mime_prefix_match_slash() {
        assert!(matches_mime("image/", "image/jpeg"));
        assert!(matches_mime("image/", "image/png"));
        assert!(!matches_mime("image/", "application/pdf"));
    }

    #[test]
    fn mime_prefix_match_star() {
        assert!(matches_mime("image/*", "image/jpeg"));
        assert!(!matches_mime("image/*", "application/pdf"));
    }

    #[test]
    fn file_ext_extraction() {
        assert_eq!(file_extension("evidence/report.pdf"), "pdf");
        assert_eq!(file_extension("archive.tar.gz"), "gz");
        assert_eq!(file_extension("noext"), "noext");
    }

    #[test]
    fn filter_empty_matches_everything() {
        let filter = TriggerFilter::default();
        let file = make_test_file("test.pdf", "evidence/test.pdf", Some("application/pdf"));
        let event = make_event(&file, 1, "evidence/test.pdf", TriggerEvent::Ingest, None);
        let (_dir, db) = setup_db();
        assert!(matches_filter(&filter, &event, &db));
    }

    #[test]
    fn filter_mime_type() {
        let filter = TriggerFilter {
            mime_type: Some("application/pdf".to_string()),
            ..Default::default()
        };
        let pdf_file = make_test_file("test.pdf", "evidence/test.pdf", Some("application/pdf"));
        let mut wav_file = make_test_file("test.wav", "evidence/test.wav", Some("audio/wav"));
        wav_file.id = Some(2);
        let (_dir, db) = setup_db();

        let event_pdf = make_event(
            &pdf_file,
            1,
            "evidence/test.pdf",
            TriggerEvent::Ingest,
            None,
        );
        assert!(matches_filter(&filter, &event_pdf, &db));

        let event_wav = make_event(
            &wav_file,
            2,
            "evidence/test.wav",
            TriggerEvent::Ingest,
            None,
        );
        assert!(!matches_filter(&filter, &event_wav, &db));
    }

    #[test]
    fn filter_file_type() {
        let filter = TriggerFilter {
            file_type: Some("pdf".to_string()),
            ..Default::default()
        };
        let file = make_test_file("test.pdf", "evidence/test.pdf", None);
        let (_dir, db) = setup_db();

        let event = make_event(&file, 1, "evidence/test.pdf", TriggerEvent::Ingest, None);
        assert!(matches_filter(&filter, &event, &db));

        let event_wav = make_event(&file, 1, "evidence/test.wav", TriggerEvent::Ingest, None);
        assert!(!matches_filter(&filter, &event_wav, &db));
    }

    #[test]
    fn filter_tag_name() {
        let filter = TriggerFilter {
            tag_name: Some("speech".to_string()),
            ..Default::default()
        };
        let file = make_test_file("test.wav", "evidence/test.wav", None);
        let (_dir, db) = setup_db();

        let event_match = make_event(
            &file,
            1,
            "evidence/test.wav",
            TriggerEvent::Tag,
            Some("speech"),
        );
        assert!(matches_filter(&filter, &event_match, &db));

        let event_no_match = make_event(
            &file,
            1,
            "evidence/test.wav",
            TriggerEvent::Tag,
            Some("other"),
        );
        assert!(!matches_filter(&filter, &event_no_match, &db));
    }

    #[test]
    fn filter_category() {
        use crate::models::{Category, CategoryType};

        let (_dir, db) = setup_db();
        db.insert_category(&Category {
            id: None,
            name: "evidence".to_string(),
            pattern: "evidence/**".to_string(),
            category_type: CategoryType::Files,
            description: None,
        })
        .unwrap();

        let filter = TriggerFilter {
            category: Some("evidence".to_string()),
            ..Default::default()
        };
        let file = make_test_file("test.pdf", "evidence/test.pdf", None);

        let event_in_cat = make_event(&file, 1, "evidence/test.pdf", TriggerEvent::Ingest, None);
        assert!(matches_filter(&filter, &event_in_cat, &db));

        let event_outside = make_event(&file, 1, "notes/test.pdf", TriggerEvent::Ingest, None);
        assert!(!matches_filter(&filter, &event_outside, &db));
    }

    #[test]
    fn filter_combined_mime_and_file_type() {
        let filter = TriggerFilter {
            mime_type: Some("application/pdf".to_string()),
            file_type: Some("pdf".to_string()),
            ..Default::default()
        };
        let file = make_test_file("test.pdf", "evidence/test.pdf", Some("application/pdf"));
        let (_dir, db) = setup_db();

        let event = make_event(&file, 1, "evidence/test.pdf", TriggerEvent::Ingest, None);
        assert!(matches_filter(&filter, &event, &db));

        // Right mime, wrong extension — AND fails
        let event_wrong_ext = make_event(&file, 1, "evidence/test.txt", TriggerEvent::Ingest, None);
        assert!(!matches_filter(&filter, &event_wrong_ext, &db));
    }

    #[test]
    fn filter_missing_category_returns_false() {
        let filter = TriggerFilter {
            category: Some("nonexistent".to_string()),
            ..Default::default()
        };
        let file = make_test_file("test.pdf", "evidence/test.pdf", None);
        let (_dir, db) = setup_db();

        let event = make_event(&file, 1, "evidence/test.pdf", TriggerEvent::Ingest, None);
        assert!(!matches_filter(&filter, &event, &db));
    }

    #[test]
    fn filter_file_with_no_mime() {
        let filter = TriggerFilter {
            mime_type: Some("application/pdf".to_string()),
            ..Default::default()
        };
        let file = make_test_file("test.pdf", "evidence/test.pdf", None);
        let (_dir, db) = setup_db();

        let event = make_event(&file, 1, "evidence/test.pdf", TriggerEvent::Ingest, None);
        // File has no mime_type, so filter should reject
        assert!(!matches_filter(&filter, &event, &db));
    }

    #[test]
    fn evaluate_rules_no_rules_is_noop() {
        let (dir, db) = setup_db();
        let file = make_test_file("test.pdf", "evidence/test.pdf", Some("application/pdf"));
        let event = make_event(&file, 1, "evidence/test.pdf", TriggerEvent::Ingest, None);
        let fired = run_rule_eval(&dir, &db, &event);
        assert!(fired.is_empty());
    }

    #[test]
    fn evaluate_rules_add_tag_action() {
        let (dir, db, file_id) =
            setup_file_env_with("test.pdf", b"fake pdf content", Some("application/pdf"));

        let rule = make_rule(
            "auto-tag",
            TriggerEvent::Ingest,
            ActionType::AddTag,
            tag_action("ingested"),
        );
        db.insert_rule(&rule).unwrap();

        let file = db.get_file_by_path("test.pdf").unwrap().unwrap();
        let event = make_event(&file, file_id, "test.pdf", TriggerEvent::Ingest, None);
        let fired = run_rule_eval(&dir, &db, &event);

        assert_eq!(fired.len(), 1);
        let tags = db.get_tags(file_id).unwrap();
        assert!(tags.contains(&"ingested".to_string()));
    }

    #[test]
    fn evaluate_rules_remove_tag_action() {
        let (dir, db, file_id) = setup_file_env_with("test.pdf", b"fake pdf", None);
        db.insert_tag(file_id, "needs-review", "fakehash").unwrap();

        let mut rule = make_rule(
            "clear-review",
            TriggerEvent::Tag,
            ActionType::RemoveTag,
            tag_action("needs-review"),
        );
        rule.trigger_filter = TriggerFilter {
            tag_name: Some("approved".to_string()),
            ..Default::default()
        };
        db.insert_rule(&rule).unwrap();

        let file = db.get_file_by_path("test.pdf").unwrap().unwrap();
        let event = make_event(
            &file,
            file_id,
            "test.pdf",
            TriggerEvent::Tag,
            Some("approved"),
        );
        let fired = run_rule_eval(&dir, &db, &event);

        let tags = db.get_tags(file_id).unwrap();
        assert!(!tags.contains(&"needs-review".to_string()));
        assert_eq!(fired.len(), 1);
    }

    #[test]
    fn evaluate_rules_recursion_guard() {
        let (dir, db, file_id) = setup_file_env_with("test.pdf", b"content", None);

        // Rule: on tag -> add_tag (could cascade)
        let rule = make_rule(
            "cascade",
            TriggerEvent::Tag,
            ActionType::AddTag,
            tag_action("cascaded"),
        );
        db.insert_rule(&rule).unwrap();

        let file = db.get_file_by_path("test.pdf").unwrap().unwrap();
        let event = make_event(
            &file,
            file_id,
            "test.pdf",
            TriggerEvent::Tag,
            Some("trigger"),
        );
        // Should not loop forever — rule fires once then is skipped on cascade
        let fired = run_rule_eval(&dir, &db, &event);
        assert_eq!(fired.len(), 1);
    }

    #[test]
    fn evaluate_rules_disabled_rule_skipped() {
        let (dir, db, file_id) = setup_file_env_with("test.pdf", b"content", None);

        let mut rule = make_rule(
            "disabled-rule",
            TriggerEvent::Ingest,
            ActionType::AddTag,
            tag_action("should-not-appear"),
        );
        rule.enabled = false;
        db.insert_rule(&rule).unwrap();

        let file = db.get_file_by_path("test.pdf").unwrap().unwrap();
        let event = make_event(&file, file_id, "test.pdf", TriggerEvent::Ingest, None);
        let fired = run_rule_eval(&dir, &db, &event);
        assert!(fired.is_empty());
        let tags = db.get_tags(file_id).unwrap();
        assert!(tags.is_empty());
    }

    #[test]
    fn evaluate_rules_filter_rejects_mismatched_event() {
        let (dir, db, file_id) = setup_file_env_with("test.wav", b"audio", Some("audio/wav"));

        // Rule triggers on ingest but only for PDFs
        let mut rule = make_rule(
            "pdf-only",
            TriggerEvent::Ingest,
            ActionType::AddTag,
            tag_action("is-pdf"),
        );
        rule.trigger_filter = TriggerFilter {
            file_type: Some("pdf".to_string()),
            ..Default::default()
        };
        db.insert_rule(&rule).unwrap();

        let file = db.get_file_by_path("test.wav").unwrap().unwrap();
        let event = make_event(&file, file_id, "test.wav", TriggerEvent::Ingest, None);
        // Rule matched event type but filter rejected it
        let fired = run_rule_eval(&dir, &db, &event);
        assert!(fired.is_empty());
        let tags = db.get_tags(file_id).unwrap();
        assert!(tags.is_empty());
    }

    #[test]
    fn filter_pipeline_name() {
        let filter = TriggerFilter {
            pipeline: Some("editorial".to_string()),
            ..Default::default()
        };
        let file = make_test_file("test.pdf", "evidence/test.pdf", None);
        let (_dir, db) = setup_db();

        let mut event = make_event(&file, 1, "evidence/test.pdf", TriggerEvent::Sign, None);
        event.pipeline_name = Some("editorial");
        assert!(matches_filter(&filter, &event, &db));

        event.pipeline_name = Some("other");
        assert!(!matches_filter(&filter, &event, &db));
    }

    #[test]
    fn filter_sign_name() {
        let filter = TriggerFilter {
            sign_name: Some("review".to_string()),
            ..Default::default()
        };
        let file = make_test_file("test.pdf", "evidence/test.pdf", None);
        let (_dir, db) = setup_db();

        let mut event = make_event(&file, 1, "evidence/test.pdf", TriggerEvent::Sign, None);
        event.sign_name = Some("review");
        assert!(matches_filter(&filter, &event, &db));

        event.sign_name = Some("publish");
        assert!(!matches_filter(&filter, &event, &db));
    }

    #[test]
    fn filter_state() {
        let filter = TriggerFilter {
            state: Some("reviewed".to_string()),
            ..Default::default()
        };
        let file = make_test_file("test.pdf", "evidence/test.pdf", None);
        let (_dir, db) = setup_db();

        let mut event = make_event(
            &file,
            1,
            "evidence/test.pdf",
            TriggerEvent::StateChange,
            None,
        );
        event.new_state = Some("reviewed");
        assert!(matches_filter(&filter, &event, &db));

        event.new_state = Some("draft");
        assert!(!matches_filter(&filter, &event, &db));
    }

    #[test]
    fn lifecycle_event_skips_file_dependent_filters() {
        let filter = TriggerFilter {
            category: Some("evidence".to_string()),
            ..Default::default()
        };
        let (_dir, db) = setup_db();

        let event = RuleEvent {
            event: TriggerEvent::ProjectEnter,
            file: None,
            file_id: None,
            rel_path: None,
            tag_name: None,
            target_category: None,
            pipeline_name: None,
            sign_name: None,
            new_state: None,
        };
        assert!(!matches_filter(&filter, &event, &db));
    }

    #[test]
    fn lifecycle_event_empty_filter_matches() {
        let filter = TriggerFilter::default();
        let (_dir, db) = setup_db();

        let event = RuleEvent {
            event: TriggerEvent::ProjectEnter,
            file: None,
            file_id: None,
            rel_path: None,
            tag_name: None,
            target_category: None,
            pipeline_name: None,
            sign_name: None,
            new_state: None,
        };
        assert!(matches_filter(&filter, &event, &db));
    }

    fn make_pipeline(name: &str) -> crate::models::Pipeline {
        let states = vec![
            "draft".to_string(),
            "reviewed".to_string(),
            "published".to_string(),
        ];
        let transitions = crate::models::Pipeline::default_transitions(&states);
        crate::models::Pipeline {
            id: None,
            name: name.to_string(),
            states,
            transitions,
        }
    }

    fn sign_action(pipeline: &str, sign_name: &str) -> ActionConfig {
        ActionConfig {
            tool: None,
            tag: None,
            pipeline: Some(pipeline.to_string()),
            sign_name: Some(sign_name.to_string()),
            category: None,
        }
    }

    fn pipeline_scope_action(
        pipeline: &str,
        category: Option<&str>,
        tag: Option<&str>,
    ) -> ActionConfig {
        ActionConfig {
            tool: None,
            tag: tag.map(str::to_string),
            pipeline: Some(pipeline.to_string()),
            sign_name: None,
            category: category.map(str::to_string),
        }
    }

    #[test]
    fn action_sign_creates_sign_with_provenance() {
        let (dir, db, file_id) = setup_file_env_with("report.pdf", b"evidence content", None);

        let pipeline = make_pipeline("editorial");
        let pipeline_id = db.insert_pipeline(&pipeline).unwrap();
        db.attach_pipeline(
            pipeline_id,
            crate::models::AttachmentScope::Category,
            "evidence",
        )
        .unwrap();

        let rule = make_rule(
            "auto-review",
            TriggerEvent::Ingest,
            ActionType::Sign,
            sign_action("editorial", "reviewed"),
        );
        db.insert_rule(&rule).unwrap();

        let file = db.get_file_by_path("report.pdf").unwrap().unwrap();
        let event = make_event(&file, file_id, "report.pdf", TriggerEvent::Ingest, None);
        let fired = run_rule_eval(&dir, &db, &event);

        assert_eq!(fired.len(), 1);

        let signs = db.get_signs_for_file(file_id).unwrap();
        assert_eq!(signs.len(), 1);
        assert_eq!(signs[0].sign_name, "reviewed");
        assert_eq!(signs[0].pipeline_id, pipeline_id);
        assert_eq!(signs[0].source.as_deref(), Some("rule:auto-review"));
    }

    #[test]
    fn action_unsign_revokes_existing_sign() {
        let (dir, db, file_id) = setup_file_env_with("report.pdf", b"evidence content", None);

        let pipeline = make_pipeline("editorial");
        let pipeline_id = db.insert_pipeline(&pipeline).unwrap();

        let hash = integrity::hash_file(&dir.path().join("report.pdf")).unwrap();
        let sign = Sign {
            id: None,
            pipeline_id,
            file_id,
            file_hash: hash,
            sign_name: "reviewed".to_string(),
            signer: "tester".to_string(),
            signed_at: Utc::now().to_rfc3339(),
            signature: None,
            revoked_at: None,
            source: None,
        };
        db.insert_sign(&sign).unwrap();

        let rule = make_rule(
            "revoke-review",
            TriggerEvent::Tag,
            ActionType::Unsign,
            sign_action("editorial", "reviewed"),
        );
        db.insert_rule(&rule).unwrap();

        let file = db.get_file_by_path("report.pdf").unwrap().unwrap();
        let event = make_event(
            &file,
            file_id,
            "report.pdf",
            TriggerEvent::Tag,
            Some("retract"),
        );
        let fired = run_rule_eval(&dir, &db, &event);

        assert_eq!(fired.len(), 1);

        let active = db.find_sign(file_id, pipeline_id, "reviewed").unwrap();
        assert!(active.is_none(), "sign should be revoked");
    }

    #[test]
    fn action_unsign_missing_sign_is_noop() {
        let (dir, db, file_id) = setup_file_env_with("report.pdf", b"evidence content", None);

        let pipeline = make_pipeline("editorial");
        db.insert_pipeline(&pipeline).unwrap();

        let rule = make_rule(
            "revoke-phantom",
            TriggerEvent::Ingest,
            ActionType::Unsign,
            sign_action("editorial", "reviewed"),
        );
        db.insert_rule(&rule).unwrap();

        let file = db.get_file_by_path("report.pdf").unwrap().unwrap();
        let event = make_event(&file, file_id, "report.pdf", TriggerEvent::Ingest, None);
        let fired = run_rule_eval(&dir, &db, &event);

        assert_eq!(fired.len(), 1);
        let signs = db.get_signs_for_file(file_id).unwrap();
        assert!(signs.is_empty());
    }

    #[test]
    fn action_attach_pipeline_creates_attachment() {
        let (dir, db, _file_id) = setup_file_env_with("report.pdf", b"content", None);

        let pipeline = make_pipeline("security");
        let pipeline_id = db.insert_pipeline(&pipeline).unwrap();

        let rule = make_rule(
            "auto-attach",
            TriggerEvent::Ingest,
            ActionType::AttachPipeline,
            pipeline_scope_action("security", Some("evidence"), None),
        );
        db.insert_rule(&rule).unwrap();

        let file = db.get_file_by_path("report.pdf").unwrap().unwrap();
        let event = make_event(&file, _file_id, "report.pdf", TriggerEvent::Ingest, None);
        let fired = run_rule_eval(&dir, &db, &event);

        assert_eq!(fired.len(), 1);
        let attachments = db.list_attachments_for_pipeline(pipeline_id).unwrap();
        assert_eq!(attachments.len(), 1);
        assert_eq!(attachments[0].scope_value, "evidence");
    }

    #[test]
    fn action_attach_pipeline_duplicate_is_idempotent() {
        let (dir, db, _file_id) = setup_file_env_with("report.pdf", b"content", None);

        let pipeline = make_pipeline("security");
        let pipeline_id = db.insert_pipeline(&pipeline).unwrap();

        db.attach_pipeline(
            pipeline_id,
            crate::models::AttachmentScope::Category,
            "evidence",
        )
        .unwrap();

        let rule = make_rule(
            "re-attach",
            TriggerEvent::Ingest,
            ActionType::AttachPipeline,
            pipeline_scope_action("security", Some("evidence"), None),
        );
        db.insert_rule(&rule).unwrap();

        let file = db.get_file_by_path("report.pdf").unwrap().unwrap();
        let event = make_event(&file, _file_id, "report.pdf", TriggerEvent::Ingest, None);
        let fired = run_rule_eval(&dir, &db, &event);

        assert_eq!(fired.len(), 1);
        let attachments = db.list_attachments_for_pipeline(pipeline_id).unwrap();
        assert_eq!(attachments.len(), 1);
    }

    #[test]
    fn action_detach_pipeline_removes_attachment() {
        let (dir, db, _file_id) = setup_file_env_with("report.pdf", b"content", None);

        let pipeline = make_pipeline("security");
        let pipeline_id = db.insert_pipeline(&pipeline).unwrap();
        db.attach_pipeline(
            pipeline_id,
            crate::models::AttachmentScope::Tag,
            "classified",
        )
        .unwrap();

        let rule = make_rule(
            "auto-detach",
            TriggerEvent::Untag,
            ActionType::DetachPipeline,
            pipeline_scope_action("security", None, Some("classified")),
        );
        db.insert_rule(&rule).unwrap();

        let file = db.get_file_by_path("report.pdf").unwrap().unwrap();
        let event = make_event(
            &file,
            _file_id,
            "report.pdf",
            TriggerEvent::Untag,
            Some("classified"),
        );
        let fired = run_rule_eval(&dir, &db, &event);

        assert_eq!(fired.len(), 1);
        let attachments = db.list_attachments_for_pipeline(pipeline_id).unwrap();
        assert!(attachments.is_empty());
    }

    #[test]
    fn sign_action_cascades_state_change() {
        let (dir, db, file_id) = setup_file_env_with("report.pdf", b"evidence content", None);

        let pipeline = make_pipeline("editorial");
        let pipeline_id = db.insert_pipeline(&pipeline).unwrap();
        db.attach_pipeline(
            pipeline_id,
            crate::models::AttachmentScope::Category,
            "evidence",
        )
        .unwrap();

        let sign_rule = make_rule(
            "auto-review",
            TriggerEvent::Ingest,
            ActionType::Sign,
            sign_action("editorial", "reviewed"),
        );
        db.insert_rule(&sign_rule).unwrap();

        let mut tag_on_state = make_rule(
            "tag-on-reviewed",
            TriggerEvent::StateChange,
            ActionType::AddTag,
            tag_action("review-complete"),
        );
        tag_on_state.trigger_filter = TriggerFilter {
            state: Some("reviewed".to_string()),
            pipeline: Some("editorial".to_string()),
            ..Default::default()
        };
        db.insert_rule(&tag_on_state).unwrap();

        let file = db.get_file_by_path("report.pdf").unwrap().unwrap();
        let event = make_event(&file, file_id, "report.pdf", TriggerEvent::Ingest, None);
        let fired = run_rule_eval(&dir, &db, &event);

        assert_eq!(fired.len(), 2);
        let tags = db.get_tags(file_id).unwrap();
        assert!(
            tags.contains(&"review-complete".to_string()),
            "state change should have cascaded to tag rule"
        );
    }

    #[test]
    fn sign_cascade_recursion_guard() {
        let (dir, db, file_id) = setup_file_env_with("report.pdf", b"evidence content", None);

        let states = vec!["draft".to_string(), "reviewed".to_string()];
        let transitions = crate::models::Pipeline::default_transitions(&states);
        let pipeline = crate::models::Pipeline {
            id: None,
            name: "loop".to_string(),
            states,
            transitions,
        };
        db.insert_pipeline(&pipeline).unwrap();

        let rule = make_rule(
            "sign-on-sign",
            TriggerEvent::Sign,
            ActionType::Sign,
            sign_action("loop", "reviewed"),
        );
        db.insert_rule(&rule).unwrap();

        let file = db.get_file_by_path("report.pdf").unwrap().unwrap();
        let mut event = make_event(&file, file_id, "report.pdf", TriggerEvent::Sign, None);
        event.pipeline_name = Some("loop");
        event.sign_name = Some("reviewed");
        let fired = run_rule_eval(&dir, &db, &event);

        assert_eq!(fired.len(), 1, "recursion guard should prevent re-firing");
    }

    #[test]
    fn resolve_pipeline_scope_category() {
        let config = ActionConfig {
            tool: None,
            tag: None,
            pipeline: Some("editorial".to_string()),
            sign_name: None,
            category: Some("evidence".to_string()),
        };
        let (scope, value) = resolve_pipeline_scope(&config).unwrap();
        assert_eq!(scope, crate::models::AttachmentScope::Category);
        assert_eq!(value, "evidence");
    }

    #[test]
    fn resolve_pipeline_scope_tag() {
        let config = ActionConfig {
            tool: None,
            tag: Some("classified".to_string()),
            pipeline: Some("security".to_string()),
            sign_name: None,
            category: None,
        };
        let (scope, value) = resolve_pipeline_scope(&config).unwrap();
        assert_eq!(scope, crate::models::AttachmentScope::Tag);
        assert_eq!(value, "classified");
    }

    #[test]
    fn resolve_pipeline_scope_neither_errors() {
        let config = ActionConfig {
            tool: None,
            tag: None,
            pipeline: Some("editorial".to_string()),
            sign_name: None,
            category: None,
        };
        assert!(resolve_pipeline_scope(&config).is_err());
    }

    #[test]
    fn lifecycle_event_sign_action_requires_file_context() {
        let (dir, db, _file_id) = setup_file_env_with("report.pdf", b"content", None);

        let pipeline = make_pipeline("editorial");
        db.insert_pipeline(&pipeline).unwrap();

        let rule = make_rule(
            "lifecycle-sign",
            TriggerEvent::ProjectEnter,
            ActionType::Sign,
            sign_action("editorial", "reviewed"),
        );
        db.insert_rule(&rule).unwrap();

        let ctx = make_ctx(&dir, &db);
        let event = RuleEvent {
            event: TriggerEvent::ProjectEnter,
            file: None,
            file_id: None,
            rel_path: None,
            tag_name: None,
            target_category: None,
            pipeline_name: None,
            sign_name: None,
            new_state: None,
        };
        let mut fired = HashSet::new();
        // Should not panic — the action fails gracefully and logs the error
        evaluate_rules(&event, &ctx, &mut fired).unwrap();
        assert_eq!(fired.len(), 1, "rule should fire even though action fails");

        let signs = db.get_signs_for_file(_file_id).unwrap();
        assert!(
            signs.is_empty(),
            "no sign should be created without file context"
        );
    }

    #[test]
    fn lifecycle_attach_pipeline_works_without_file() {
        let (dir, db, _file_id) = setup_file_env_with("report.pdf", b"content", None);

        let pipeline = make_pipeline("monitoring");
        let pipeline_id = db.insert_pipeline(&pipeline).unwrap();

        let rule = make_rule(
            "lifecycle-attach",
            TriggerEvent::ProjectEnter,
            ActionType::AttachPipeline,
            pipeline_scope_action("monitoring", Some("evidence"), None),
        );
        db.insert_rule(&rule).unwrap();

        let ctx = make_ctx(&dir, &db);
        let event = RuleEvent {
            event: TriggerEvent::ProjectEnter,
            file: None,
            file_id: None,
            rel_path: None,
            tag_name: None,
            target_category: None,
            pipeline_name: None,
            sign_name: None,
            new_state: None,
        };
        let mut fired = HashSet::new();
        evaluate_rules(&event, &ctx, &mut fired).unwrap();

        assert_eq!(fired.len(), 1);
        let attachments = db.list_attachments_for_pipeline(pipeline_id).unwrap();
        assert_eq!(attachments.len(), 1);
    }
}
