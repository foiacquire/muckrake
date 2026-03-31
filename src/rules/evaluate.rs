use anyhow::Result;

use crate::db::ProjectDb;
use crate::models::policy::strictest;
use crate::models::{
    ProtectionLevel, Ruleset, RulesetActionConfig, RulesetActionType, RulesetRule,
};

/// Collected outputs from evaluating all rulesets for a file.
#[derive(Debug, Default)]
pub struct EvaluationResult {
    pub protection: ProtectionLevel,
    pub tool_dispatches: Vec<ToolDispatch>,
    pub tags_to_add: Vec<String>,
    pub tags_to_remove: Vec<String>,
    pub pipelines_to_attach: Vec<String>,
    pub virtual_pipelines: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ToolDispatch {
    pub command: String,
    pub env: Option<String>,
    pub quiet: bool,
    pub file_type: String,
    pub ruleset_name: String,
}

/// File metadata available during evaluation.
pub struct EvalContext<'a> {
    pub sha256: &'a str,
    pub mime_type: Option<&'a str>,
    pub file_type: Option<&'a str>,
}

/// Evaluate all rulesets materialized for a file hash.
/// Returns collected outputs from all matching rules.
pub fn evaluate_for_file(db: &ProjectDb, ctx: &EvalContext<'_>) -> Result<EvaluationResult> {
    let rulesets = crate::db::ruleset::get_rulesets_for_sha256(db.conn(), ctx.sha256)?;
    let mut result = EvaluationResult::default();

    for ruleset in &rulesets {
        let rules = crate::db::ruleset::list_rules_for_ruleset(db.conn(), ruleset.id.unwrap_or(0))?;
        evaluate_rules(&rules, ruleset, ctx, &mut result);
    }

    Ok(result)
}

/// Resolve protection level for a file by hash.
/// Checks materialized rulesets first, falls back to legacy `scope_policy`.
pub fn resolve_protection_by_hash(db: &ProjectDb, sha256: &str) -> Result<ProtectionLevel> {
    let ctx = EvalContext {
        sha256,
        mime_type: None,
        file_type: None,
    };
    let result = evaluate_for_file(db, &ctx)?;
    if result.protection != ProtectionLevel::Editable {
        return Ok(result.protection);
    }

    // No ruleset-based policy found — this is expected during migration
    // when scope_policy hasn't been converted to rulesets yet
    Ok(ProtectionLevel::Editable)
}

fn evaluate_rules(
    rules: &[RulesetRule],
    ruleset: &Ruleset,
    ctx: &EvalContext<'_>,
    result: &mut EvaluationResult,
) {
    for rule in rules {
        if !matches_condition(rule, ctx) {
            continue;
        }
        apply_action(rule.action_type, &rule.action_config, ruleset, result);
    }
}

fn matches_condition(rule: &RulesetRule, ctx: &EvalContext<'_>) -> bool {
    let Some(ref condition) = rule.condition else {
        return true;
    };

    if let Some(ref mime_pattern) = condition.mime_type {
        let file_mime = ctx.mime_type.unwrap_or("");
        if !mime_matches(mime_pattern, file_mime) {
            return false;
        }
    }

    if let Some(ref ft) = condition.file_type {
        let file_ft = ctx.file_type.unwrap_or("");
        if ft != file_ft && ft != "*" {
            return false;
        }
    }

    true
}

fn mime_matches(pattern: &str, actual: &str) -> bool {
    if pattern == "*" || pattern == actual {
        return true;
    }
    if let Some(prefix) = pattern.strip_suffix("/*") {
        return actual.starts_with(prefix) && actual[prefix.len()..].starts_with('/');
    }
    false
}

fn apply_action(
    action_type: RulesetActionType,
    config: &RulesetActionConfig,
    ruleset: &Ruleset,
    result: &mut EvaluationResult,
) {
    match action_type {
        RulesetActionType::ApplyPolicy => {
            if let Some(ref level_str) = config.protection_level {
                if let Ok(level) = level_str.parse::<ProtectionLevel>() {
                    result.protection = strictest(&[result.protection, level]);
                }
            }
        }
        RulesetActionType::DispatchTool => {
            if let Some(ref command) = config.command {
                result.tool_dispatches.push(ToolDispatch {
                    command: command.clone(),
                    env: config.env.clone(),
                    quiet: config.quiet.unwrap_or(true),
                    file_type: config.file_type.clone().unwrap_or_else(|| "*".to_string()),
                    ruleset_name: ruleset.name.clone(),
                });
            }
        }
        RulesetActionType::AddTag => {
            if let Some(ref tag) = config.tag {
                result.tags_to_add.push(tag.clone());
            }
        }
        RulesetActionType::RemoveTag => {
            if let Some(ref tag) = config.tag {
                result.tags_to_remove.push(tag.clone());
            }
        }
        RulesetActionType::AttachPipeline => {
            if let Some(ref pipeline) = config.pipeline {
                result.pipelines_to_attach.push(pipeline.clone());
            }
        }
        RulesetActionType::AttachPipelineVirtual => {
            if let Some(ref pipeline) = config.pipeline {
                result.virtual_pipelines.push(pipeline.clone());
            }
        }
        RulesetActionType::Sign | RulesetActionType::Unsign => {
            // Sign/unsign actions require more context (pipeline state)
            // and are handled elsewhere
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{RuleCondition, RulesetActionConfig};

    fn make_policy_rule(level: &str) -> RulesetRule {
        RulesetRule {
            id: None,
            ruleset_id: 1,
            priority: 0,
            condition: None,
            action_type: RulesetActionType::ApplyPolicy,
            action_config: RulesetActionConfig {
                protection_level: Some(level.to_string()),
                command: None,
                env: None,
                quiet: None,
                file_type: None,
                tag: None,
                pipeline: None,
                sign_name: None,
            },
        }
    }

    fn make_tool_rule(command: &str, mime_filter: Option<&str>) -> RulesetRule {
        RulesetRule {
            id: None,
            ruleset_id: 1,
            priority: 1,
            condition: mime_filter.map(|m| RuleCondition {
                mime_type: Some(m.to_string()),
                file_type: None,
            }),
            action_type: RulesetActionType::DispatchTool,
            action_config: RulesetActionConfig {
                protection_level: None,
                command: Some(command.to_string()),
                env: None,
                quiet: Some(true),
                file_type: Some("*".to_string()),
                tag: None,
                pipeline: None,
                sign_name: None,
            },
        }
    }

    fn test_ruleset() -> Ruleset {
        Ruleset {
            id: Some(1),
            name: "test".to_string(),
            description: None,
        }
    }

    #[test]
    fn apply_policy_takes_strictest() {
        let rules = vec![make_policy_rule("editable"), make_policy_rule("immutable")];
        let ctx = EvalContext {
            sha256: "abc",
            mime_type: None,
            file_type: None,
        };
        let mut result = EvaluationResult::default();
        evaluate_rules(&rules, &test_ruleset(), &ctx, &mut result);
        assert_eq!(result.protection, ProtectionLevel::Immutable);
    }

    #[test]
    fn tool_dispatch_collected() {
        let rules = vec![make_tool_rule("ocr", None)];
        let ctx = EvalContext {
            sha256: "abc",
            mime_type: None,
            file_type: None,
        };
        let mut result = EvaluationResult::default();
        evaluate_rules(&rules, &test_ruleset(), &ctx, &mut result);
        assert_eq!(result.tool_dispatches.len(), 1);
        assert_eq!(result.tool_dispatches[0].command, "ocr");
    }

    #[test]
    fn condition_filters_by_mime() {
        let rules = vec![make_tool_rule("ocr", Some("application/pdf"))];
        let ctx = EvalContext {
            sha256: "abc",
            mime_type: Some("image/png"),
            file_type: None,
        };
        let mut result = EvaluationResult::default();
        evaluate_rules(&rules, &test_ruleset(), &ctx, &mut result);
        assert!(result.tool_dispatches.is_empty());
    }

    #[test]
    fn condition_wildcard_mime() {
        let rules = vec![make_tool_rule("viewer", Some("image/*"))];
        let ctx = EvalContext {
            sha256: "abc",
            mime_type: Some("image/png"),
            file_type: None,
        };
        let mut result = EvaluationResult::default();
        evaluate_rules(&rules, &test_ruleset(), &ctx, &mut result);
        assert_eq!(result.tool_dispatches.len(), 1);
    }

    #[test]
    fn no_condition_matches_everything() {
        let rules = vec![make_tool_rule("default-tool", None)];
        let ctx = EvalContext {
            sha256: "abc",
            mime_type: Some("text/plain"),
            file_type: Some("txt"),
        };
        let mut result = EvaluationResult::default();
        evaluate_rules(&rules, &test_ruleset(), &ctx, &mut result);
        assert_eq!(result.tool_dispatches.len(), 1);
    }

    #[test]
    fn multiple_action_types() {
        let rules = vec![
            make_policy_rule("protected"),
            make_tool_rule("viewer", None),
            RulesetRule {
                id: None,
                ruleset_id: 1,
                priority: 2,
                condition: None,
                action_type: RulesetActionType::AddTag,
                action_config: RulesetActionConfig {
                    protection_level: None,
                    command: None,
                    env: None,
                    quiet: None,
                    file_type: None,
                    tag: Some("auto-tagged".to_string()),
                    pipeline: None,
                    sign_name: None,
                },
            },
        ];
        let ctx = EvalContext {
            sha256: "abc",
            mime_type: None,
            file_type: None,
        };
        let mut result = EvaluationResult::default();
        evaluate_rules(&rules, &test_ruleset(), &ctx, &mut result);
        assert_eq!(result.protection, ProtectionLevel::Protected);
        assert_eq!(result.tool_dispatches.len(), 1);
        assert_eq!(result.tags_to_add, vec!["auto-tagged"]);
    }
}
