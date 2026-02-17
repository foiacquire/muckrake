use std::collections::HashSet;
use std::path::Path;

use anyhow::Result;
use console::style;

use crate::db::{ProjectDb, WorkspaceDb};
use crate::integrity;
use crate::models::{ActionType, Rule, TrackedFile, TriggerEvent, TriggerFilter};
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
    pub file: &'a TrackedFile,
    pub file_id: i64,
    pub rel_path: &'a str,
    pub tag_name: Option<&'a str>,
    pub target_category: Option<&'a str>,
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
        Some(event.file_id),
        Some(&user),
        Some(&detail.to_string()),
    );
}

fn matches_filter(filter: &TriggerFilter, event: &RuleEvent<'_>, db: &ProjectDb) -> bool {
    if let Some(ref cat_name) = filter.category {
        if !matches_category(cat_name, event.rel_path, db) {
            return false;
        }
    }

    if let Some(ref mime) = filter.mime_type {
        let file_mime = event.file.mime_type.as_deref().unwrap_or("");
        if !matches_mime(mime, file_mime) {
            return false;
        }
    }

    if let Some(ref ft) = filter.file_type {
        let ext = file_extension(event.rel_path);
        if !ext.eq_ignore_ascii_case(ft) {
            return false;
        }
    }

    if let Some(ref tag) = filter.tag_name {
        let event_tag = event.tag_name.unwrap_or("");
        if event_tag != tag {
            return false;
        }
    }

    true
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
    }
}

fn action_run_tool(rule: &Rule, event: &RuleEvent<'_>, ctx: &RuleContext<'_>) -> Result<()> {
    let tool_name = rule.action_config.tool.as_deref().unwrap_or("unknown");
    let ext = file_extension(event.rel_path);
    let tags = ctx.project_db.get_tags(event.file_id).unwrap_or_default();
    let abs_path = ctx.project_root.join(event.rel_path);

    let params = ExecuteToolParams {
        tool_name,
        file_abs_path: &abs_path,
        file_rel_path: event.rel_path,
        file_ext: ext,
        tags: &tags,
        project_root: ctx.project_root,
        project_db: ctx.project_db,
        workspace_root: ctx.workspace_root,
        workspace_db: ctx.workspace_db,
    };
    execute_tool(&params)
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
    };
    evaluate_rules(&cascaded, ctx, fired)
}

fn action_add_tag(
    rule: &Rule,
    event: &RuleEvent<'_>,
    ctx: &RuleContext<'_>,
    fired: &mut HashSet<i64>,
) -> Result<()> {
    let tag = rule.action_config.tag.as_deref().unwrap_or("unknown");
    let abs_path = ctx.project_root.join(event.rel_path);
    let hash = integrity::hash_file(&abs_path)?;
    ctx.project_db.insert_tag(event.file_id, tag, &hash)?;
    eprintln!("    tagged '{}' with '{tag}'", event.file.name);
    cascade_tag_event(event, TriggerEvent::Tag, tag, ctx, fired)
}

fn action_remove_tag(
    rule: &Rule,
    event: &RuleEvent<'_>,
    ctx: &RuleContext<'_>,
    fired: &mut HashSet<i64>,
) -> Result<()> {
    let tag = rule.action_config.tag.as_deref().unwrap_or("unknown");
    ctx.project_db.remove_tag(event.file_id, tag)?;
    eprintln!("    untagged '{}' from '{tag}'", event.file.name);
    cascade_tag_event(event, TriggerEvent::Untag, tag, ctx, fired)
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
            file,
            file_id,
            rel_path,
            tag_name,
            target_category: None,
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
}
