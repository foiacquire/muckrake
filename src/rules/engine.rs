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

    let tag_event = RuleEvent {
        event: TriggerEvent::Tag,
        file: event.file,
        file_id: event.file_id,
        rel_path: event.rel_path,
        tag_name: Some(tag),
        target_category: None,
    };
    evaluate_rules(&tag_event, ctx, fired)
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

    let untag_event = RuleEvent {
        event: TriggerEvent::Untag,
        file: event.file,
        file_id: event.file_id,
        rel_path: event.rel_path,
        tag_name: Some(tag),
        target_category: None,
    };
    evaluate_rules(&untag_event, ctx, fired)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let file = TrackedFile {
            id: Some(1),
            name: "test.pdf".to_string(),
            path: "evidence/test.pdf".to_string(),
            sha256: None,
            mime_type: Some("application/pdf".to_string()),
            size: None,
            ingested_at: String::new(),
            provenance: None,
            immutable: false,
        };
        let event = RuleEvent {
            event: TriggerEvent::Ingest,
            file: &file,
            file_id: 1,
            rel_path: "evidence/test.pdf",
            tag_name: None,
            target_category: None,
        };
        let dir = tempfile::TempDir::new().unwrap();
        let db = ProjectDb::create(&dir.path().join(".mkrk")).unwrap();
        assert!(matches_filter(&filter, &event, &db));
    }

    #[test]
    fn filter_mime_type() {
        let filter = TriggerFilter {
            mime_type: Some("application/pdf".to_string()),
            ..Default::default()
        };
        let pdf_file = TrackedFile {
            id: Some(1),
            name: "test.pdf".to_string(),
            path: "evidence/test.pdf".to_string(),
            sha256: None,
            mime_type: Some("application/pdf".to_string()),
            size: None,
            ingested_at: String::new(),
            provenance: None,
            immutable: false,
        };
        let wav_file = TrackedFile {
            id: Some(2),
            name: "test.wav".to_string(),
            path: "evidence/test.wav".to_string(),
            sha256: None,
            mime_type: Some("audio/wav".to_string()),
            size: None,
            ingested_at: String::new(),
            provenance: None,
            immutable: false,
        };
        let dir = tempfile::TempDir::new().unwrap();
        let db = ProjectDb::create(&dir.path().join(".mkrk")).unwrap();

        let event_pdf = RuleEvent {
            event: TriggerEvent::Ingest,
            file: &pdf_file,
            file_id: 1,
            rel_path: "evidence/test.pdf",
            tag_name: None,
            target_category: None,
        };
        assert!(matches_filter(&filter, &event_pdf, &db));

        let event_wav = RuleEvent {
            event: TriggerEvent::Ingest,
            file: &wav_file,
            file_id: 2,
            rel_path: "evidence/test.wav",
            tag_name: None,
            target_category: None,
        };
        assert!(!matches_filter(&filter, &event_wav, &db));
    }

    #[test]
    fn filter_file_type() {
        let filter = TriggerFilter {
            file_type: Some("pdf".to_string()),
            ..Default::default()
        };
        let file = TrackedFile {
            id: Some(1),
            name: "test.pdf".to_string(),
            path: "evidence/test.pdf".to_string(),
            sha256: None,
            mime_type: None,
            size: None,
            ingested_at: String::new(),
            provenance: None,
            immutable: false,
        };
        let dir = tempfile::TempDir::new().unwrap();
        let db = ProjectDb::create(&dir.path().join(".mkrk")).unwrap();

        let event = RuleEvent {
            event: TriggerEvent::Ingest,
            file: &file,
            file_id: 1,
            rel_path: "evidence/test.pdf",
            tag_name: None,
            target_category: None,
        };
        assert!(matches_filter(&filter, &event, &db));

        let event_wav = RuleEvent {
            event: TriggerEvent::Ingest,
            file: &file,
            file_id: 1,
            rel_path: "evidence/test.wav",
            tag_name: None,
            target_category: None,
        };
        assert!(!matches_filter(&filter, &event_wav, &db));
    }

    #[test]
    fn filter_tag_name() {
        let filter = TriggerFilter {
            tag_name: Some("speech".to_string()),
            ..Default::default()
        };
        let file = TrackedFile {
            id: Some(1),
            name: "test.wav".to_string(),
            path: "evidence/test.wav".to_string(),
            sha256: None,
            mime_type: None,
            size: None,
            ingested_at: String::new(),
            provenance: None,
            immutable: false,
        };
        let dir = tempfile::TempDir::new().unwrap();
        let db = ProjectDb::create(&dir.path().join(".mkrk")).unwrap();

        let event_match = RuleEvent {
            event: TriggerEvent::Tag,
            file: &file,
            file_id: 1,
            rel_path: "evidence/test.wav",
            tag_name: Some("speech"),
            target_category: None,
        };
        assert!(matches_filter(&filter, &event_match, &db));

        let event_no_match = RuleEvent {
            event: TriggerEvent::Tag,
            file: &file,
            file_id: 1,
            rel_path: "evidence/test.wav",
            tag_name: Some("other"),
            target_category: None,
        };
        assert!(!matches_filter(&filter, &event_no_match, &db));
    }

    #[test]
    fn filter_category() {
        use crate::models::{Category, CategoryType};

        let dir = tempfile::TempDir::new().unwrap();
        let db = ProjectDb::create(&dir.path().join(".mkrk")).unwrap();
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
        let file = TrackedFile {
            id: Some(1),
            name: "test.pdf".to_string(),
            path: "evidence/test.pdf".to_string(),
            sha256: None,
            mime_type: None,
            size: None,
            ingested_at: String::new(),
            provenance: None,
            immutable: false,
        };

        let event_in_cat = RuleEvent {
            event: TriggerEvent::Ingest,
            file: &file,
            file_id: 1,
            rel_path: "evidence/test.pdf",
            tag_name: None,
            target_category: None,
        };
        assert!(matches_filter(&filter, &event_in_cat, &db));

        let event_outside = RuleEvent {
            event: TriggerEvent::Ingest,
            file: &file,
            file_id: 1,
            rel_path: "notes/test.pdf",
            tag_name: None,
            target_category: None,
        };
        assert!(!matches_filter(&filter, &event_outside, &db));
    }

    #[test]
    fn filter_combined_mime_and_file_type() {
        let filter = TriggerFilter {
            mime_type: Some("application/pdf".to_string()),
            file_type: Some("pdf".to_string()),
            ..Default::default()
        };
        let file = TrackedFile {
            id: Some(1),
            name: "test.pdf".to_string(),
            path: "evidence/test.pdf".to_string(),
            sha256: None,
            mime_type: Some("application/pdf".to_string()),
            size: None,
            ingested_at: String::new(),
            provenance: None,
            immutable: false,
        };
        let dir = tempfile::TempDir::new().unwrap();
        let db = ProjectDb::create(&dir.path().join(".mkrk")).unwrap();

        let event = RuleEvent {
            event: TriggerEvent::Ingest,
            file: &file,
            file_id: 1,
            rel_path: "evidence/test.pdf",
            tag_name: None,
            target_category: None,
        };
        assert!(matches_filter(&filter, &event, &db));

        // Right mime, wrong extension — AND fails
        let event_wrong_ext = RuleEvent {
            event: TriggerEvent::Ingest,
            file: &file,
            file_id: 1,
            rel_path: "evidence/test.txt",
            tag_name: None,
            target_category: None,
        };
        assert!(!matches_filter(&filter, &event_wrong_ext, &db));
    }

    #[test]
    fn filter_missing_category_returns_false() {
        let filter = TriggerFilter {
            category: Some("nonexistent".to_string()),
            ..Default::default()
        };
        let file = TrackedFile {
            id: Some(1),
            name: "test.pdf".to_string(),
            path: "evidence/test.pdf".to_string(),
            sha256: None,
            mime_type: None,
            size: None,
            ingested_at: String::new(),
            provenance: None,
            immutable: false,
        };
        let dir = tempfile::TempDir::new().unwrap();
        let db = ProjectDb::create(&dir.path().join(".mkrk")).unwrap();

        let event = RuleEvent {
            event: TriggerEvent::Ingest,
            file: &file,
            file_id: 1,
            rel_path: "evidence/test.pdf",
            tag_name: None,
            target_category: None,
        };
        assert!(!matches_filter(&filter, &event, &db));
    }

    #[test]
    fn filter_file_with_no_mime() {
        let filter = TriggerFilter {
            mime_type: Some("application/pdf".to_string()),
            ..Default::default()
        };
        let file = TrackedFile {
            id: Some(1),
            name: "test.pdf".to_string(),
            path: "evidence/test.pdf".to_string(),
            sha256: None,
            mime_type: None,
            size: None,
            ingested_at: String::new(),
            provenance: None,
            immutable: false,
        };
        let dir = tempfile::TempDir::new().unwrap();
        let db = ProjectDb::create(&dir.path().join(".mkrk")).unwrap();

        let event = RuleEvent {
            event: TriggerEvent::Ingest,
            file: &file,
            file_id: 1,
            rel_path: "evidence/test.pdf",
            tag_name: None,
            target_category: None,
        };
        // File has no mime_type, so filter should reject
        assert!(!matches_filter(&filter, &event, &db));
    }

    fn make_test_file(name: &str, path: &str) -> TrackedFile {
        TrackedFile {
            id: Some(1),
            name: name.to_string(),
            path: path.to_string(),
            sha256: None,
            mime_type: Some("application/pdf".to_string()),
            size: None,
            ingested_at: String::new(),
            provenance: None,
            immutable: false,
        }
    }

    #[test]
    fn evaluate_rules_no_rules_is_noop() {
        let dir = tempfile::TempDir::new().unwrap();
        let db = ProjectDb::create(&dir.path().join(".mkrk")).unwrap();
        let file = make_test_file("test.pdf", "evidence/test.pdf");

        let event = RuleEvent {
            event: TriggerEvent::Ingest,
            file: &file,
            file_id: 1,
            rel_path: "evidence/test.pdf",
            tag_name: None,
            target_category: None,
        };
        let ctx = RuleContext {
            project_root: dir.path(),
            project_db: &db,
            workspace_root: None,
            workspace_db: None,
        };
        let mut fired = HashSet::new();
        assert!(evaluate_rules(&event, &ctx, &mut fired).is_ok());
        assert!(fired.is_empty());
    }

    #[test]
    fn evaluate_rules_add_tag_action() {
        use crate::models::ActionConfig;

        let dir = tempfile::TempDir::new().unwrap();
        let db_path = dir.path().join(".mkrk");
        let db = ProjectDb::create(&db_path).unwrap();

        // Create a file on disk so hash_file works
        let file_path = dir.path().join("test.pdf");
        std::fs::write(&file_path, b"fake pdf content").unwrap();

        // Insert the file into DB
        let tracked = TrackedFile {
            id: None,
            name: "test.pdf".to_string(),
            path: "test.pdf".to_string(),
            sha256: None,
            mime_type: Some("application/pdf".to_string()),
            size: Some(16),
            ingested_at: String::new(),
            provenance: None,
            immutable: false,
        };
        let file_id = db.insert_file(&tracked).unwrap();

        // Create an add_tag rule
        let rule = Rule {
            id: None,
            name: "auto-tag".to_string(),
            enabled: true,
            trigger_event: TriggerEvent::Ingest,
            trigger_filter: TriggerFilter::default(),
            action_type: ActionType::AddTag,
            action_config: ActionConfig {
                tool: None,
                tag: Some("ingested".to_string()),
            },
            priority: 0,
            created_at: String::new(),
        };
        db.insert_rule(&rule).unwrap();

        // Fetch the file back so it has an id
        let file = db.get_file_by_path("test.pdf").unwrap().unwrap();

        let event = RuleEvent {
            event: TriggerEvent::Ingest,
            file: &file,
            file_id,
            rel_path: "test.pdf",
            tag_name: None,
            target_category: None,
        };
        let ctx = RuleContext {
            project_root: dir.path(),
            project_db: &db,
            workspace_root: None,
            workspace_db: None,
        };
        let mut fired = HashSet::new();
        evaluate_rules(&event, &ctx, &mut fired).unwrap();

        // The rule should have fired and added the tag
        assert_eq!(fired.len(), 1);
        let tags = db.get_tags(file_id).unwrap();
        assert!(tags.contains(&"ingested".to_string()));
    }

    #[test]
    fn evaluate_rules_remove_tag_action() {
        use crate::models::ActionConfig;

        let dir = tempfile::TempDir::new().unwrap();
        let db = ProjectDb::create(&dir.path().join(".mkrk")).unwrap();

        // Create file on disk and in DB
        let file_path = dir.path().join("test.pdf");
        std::fs::write(&file_path, b"fake pdf").unwrap();

        let tracked = TrackedFile {
            id: None,
            name: "test.pdf".to_string(),
            path: "test.pdf".to_string(),
            sha256: None,
            mime_type: None,
            size: Some(8),
            ingested_at: String::new(),
            provenance: None,
            immutable: false,
        };
        let file_id = db.insert_file(&tracked).unwrap();
        db.insert_tag(file_id, "needs-review", "fakehash").unwrap();

        // Create remove_tag rule triggered on tag event
        let rule = Rule {
            id: None,
            name: "clear-review".to_string(),
            enabled: true,
            trigger_event: TriggerEvent::Tag,
            trigger_filter: TriggerFilter {
                tag_name: Some("approved".to_string()),
                ..Default::default()
            },
            action_type: ActionType::RemoveTag,
            action_config: ActionConfig {
                tool: None,
                tag: Some("needs-review".to_string()),
            },
            priority: 0,
            created_at: String::new(),
        };
        db.insert_rule(&rule).unwrap();

        let file = db.get_file_by_path("test.pdf").unwrap().unwrap();
        let event = RuleEvent {
            event: TriggerEvent::Tag,
            file: &file,
            file_id,
            rel_path: "test.pdf",
            tag_name: Some("approved"),
            target_category: None,
        };
        let ctx = RuleContext {
            project_root: dir.path(),
            project_db: &db,
            workspace_root: None,
            workspace_db: None,
        };
        let mut fired = HashSet::new();
        evaluate_rules(&event, &ctx, &mut fired).unwrap();

        let tags = db.get_tags(file_id).unwrap();
        assert!(!tags.contains(&"needs-review".to_string()));
    }

    #[test]
    fn evaluate_rules_recursion_guard() {
        use crate::models::ActionConfig;

        let dir = tempfile::TempDir::new().unwrap();
        let db = ProjectDb::create(&dir.path().join(".mkrk")).unwrap();

        let file_path = dir.path().join("test.pdf");
        std::fs::write(&file_path, b"content").unwrap();

        let tracked = TrackedFile {
            id: None,
            name: "test.pdf".to_string(),
            path: "test.pdf".to_string(),
            sha256: None,
            mime_type: None,
            size: Some(7),
            ingested_at: String::new(),
            provenance: None,
            immutable: false,
        };
        let file_id = db.insert_file(&tracked).unwrap();

        // Rule: on tag -> add_tag (could cascade)
        let rule = Rule {
            id: None,
            name: "cascade".to_string(),
            enabled: true,
            trigger_event: TriggerEvent::Tag,
            trigger_filter: TriggerFilter::default(),
            action_type: ActionType::AddTag,
            action_config: ActionConfig {
                tool: None,
                tag: Some("cascaded".to_string()),
            },
            priority: 0,
            created_at: String::new(),
        };
        db.insert_rule(&rule).unwrap();

        let file = db.get_file_by_path("test.pdf").unwrap().unwrap();
        let event = RuleEvent {
            event: TriggerEvent::Tag,
            file: &file,
            file_id,
            rel_path: "test.pdf",
            tag_name: Some("trigger"),
            target_category: None,
        };
        let ctx = RuleContext {
            project_root: dir.path(),
            project_db: &db,
            workspace_root: None,
            workspace_db: None,
        };
        let mut fired = HashSet::new();
        // Should not loop forever — rule fires once then is skipped on cascade
        evaluate_rules(&event, &ctx, &mut fired).unwrap();
        assert_eq!(fired.len(), 1);
    }

    #[test]
    fn evaluate_rules_disabled_rule_skipped() {
        use crate::models::ActionConfig;

        let dir = tempfile::TempDir::new().unwrap();
        let db = ProjectDb::create(&dir.path().join(".mkrk")).unwrap();

        let file_path = dir.path().join("test.pdf");
        std::fs::write(&file_path, b"content").unwrap();

        let tracked = TrackedFile {
            id: None,
            name: "test.pdf".to_string(),
            path: "test.pdf".to_string(),
            sha256: None,
            mime_type: None,
            size: Some(7),
            ingested_at: String::new(),
            provenance: None,
            immutable: false,
        };
        let file_id = db.insert_file(&tracked).unwrap();

        let rule = Rule {
            id: None,
            name: "disabled-rule".to_string(),
            enabled: false,
            trigger_event: TriggerEvent::Ingest,
            trigger_filter: TriggerFilter::default(),
            action_type: ActionType::AddTag,
            action_config: ActionConfig {
                tool: None,
                tag: Some("should-not-appear".to_string()),
            },
            priority: 0,
            created_at: String::new(),
        };
        db.insert_rule(&rule).unwrap();

        let file = db.get_file_by_path("test.pdf").unwrap().unwrap();
        let event = RuleEvent {
            event: TriggerEvent::Ingest,
            file: &file,
            file_id,
            rel_path: "test.pdf",
            tag_name: None,
            target_category: None,
        };
        let ctx = RuleContext {
            project_root: dir.path(),
            project_db: &db,
            workspace_root: None,
            workspace_db: None,
        };
        let mut fired = HashSet::new();
        evaluate_rules(&event, &ctx, &mut fired).unwrap();
        assert!(fired.is_empty());
        let tags = db.get_tags(file_id).unwrap();
        assert!(tags.is_empty());
    }

    #[test]
    fn evaluate_rules_filter_rejects_mismatched_event() {
        use crate::models::ActionConfig;

        let dir = tempfile::TempDir::new().unwrap();
        let db = ProjectDb::create(&dir.path().join(".mkrk")).unwrap();

        let file_path = dir.path().join("test.wav");
        std::fs::write(&file_path, b"audio").unwrap();

        let tracked = TrackedFile {
            id: None,
            name: "test.wav".to_string(),
            path: "test.wav".to_string(),
            sha256: None,
            mime_type: Some("audio/wav".to_string()),
            size: Some(5),
            ingested_at: String::new(),
            provenance: None,
            immutable: false,
        };
        let file_id = db.insert_file(&tracked).unwrap();

        // Rule triggers on ingest but only for PDFs
        let rule = Rule {
            id: None,
            name: "pdf-only".to_string(),
            enabled: true,
            trigger_event: TriggerEvent::Ingest,
            trigger_filter: TriggerFilter {
                file_type: Some("pdf".to_string()),
                ..Default::default()
            },
            action_type: ActionType::AddTag,
            action_config: ActionConfig {
                tool: None,
                tag: Some("is-pdf".to_string()),
            },
            priority: 0,
            created_at: String::new(),
        };
        db.insert_rule(&rule).unwrap();

        let file = db.get_file_by_path("test.wav").unwrap().unwrap();
        let event = RuleEvent {
            event: TriggerEvent::Ingest,
            file: &file,
            file_id,
            rel_path: "test.wav",
            tag_name: None,
            target_category: None,
        };
        let ctx = RuleContext {
            project_root: dir.path(),
            project_db: &db,
            workspace_root: None,
            workspace_db: None,
        };
        let mut fired = HashSet::new();
        evaluate_rules(&event, &ctx, &mut fired).unwrap();
        // Rule matched event type but filter rejected it
        assert!(fired.is_empty());
        let tags = db.get_tags(file_id).unwrap();
        assert!(tags.is_empty());
    }
}
