use std::path::Path;

use anyhow::{bail, Result};
use chrono::Utc;
use console::style;

use crate::context::discover;
use crate::models::{ActionConfig, ActionType, Rule, TriggerEvent, TriggerFilter};

pub fn run_add(cwd: &Path, params: &AddRuleParams<'_>) -> Result<()> {
    let ctx = discover(cwd)?;
    let (_, project_db) = ctx.require_project()?;

    let trigger_event: TriggerEvent = params.on.parse()?;
    let action_type: ActionType = params.action.parse()?;

    validate_action_params(action_type, params)?;

    let trigger_filter = TriggerFilter {
        tag_name: params.trigger_tag.map(String::from),
        category: params.category.map(String::from),
        mime_type: params.mime_type.map(String::from),
        file_type: params.file_type.map(String::from),
        pipeline: params.trigger_pipeline.map(String::from),
        sign_name: params.trigger_sign.map(String::from),
        state: params.trigger_state.map(String::from),
    };

    let action_config = build_action_config(action_type, params);

    let rule = Rule {
        id: None,
        name: params.name.to_string(),
        enabled: true,
        trigger_event,
        trigger_filter,
        action_type,
        action_config,
        priority: params.priority,
        created_at: Utc::now().to_rfc3339(),
    };

    project_db.insert_rule(&rule)?;
    eprintln!(
        "Added rule '{}': on {} -> {} {}",
        params.name,
        trigger_event,
        action_type,
        action_target(&rule.action_config),
    );

    Ok(())
}

pub fn run_list(cwd: &Path) -> Result<()> {
    let ctx = discover(cwd)?;
    let (_, project_db) = ctx.require_project()?;

    let rules = project_db.list_rules()?;
    if rules.is_empty() {
        eprintln!("No rules defined");
        return Ok(());
    }

    for rule in &rules {
        let status = if rule.enabled {
            style("on").green()
        } else {
            style("off").red()
        };
        let filter = format_filter(&rule.trigger_filter);
        let target = action_target(&rule.action_config);
        println!(
            "  [{status}] {} (p{}) : {} {filter}-> {} {target}",
            style(&rule.name).cyan(),
            rule.priority,
            rule.trigger_event,
            rule.action_type,
        );
    }

    Ok(())
}

pub fn run_remove(cwd: &Path, name: &str) -> Result<()> {
    let ctx = discover(cwd)?;
    let (_, project_db) = ctx.require_project()?;

    let count = project_db.remove_rule(name)?;
    if count == 0 {
        bail!("no rule named '{name}'");
    }
    eprintln!("Removed rule '{name}'");
    Ok(())
}

pub fn run_enable(cwd: &Path, name: &str) -> Result<()> {
    let ctx = discover(cwd)?;
    let (_, project_db) = ctx.require_project()?;

    let count = project_db.set_rule_enabled(name, true)?;
    if count == 0 {
        bail!("no rule named '{name}'");
    }
    eprintln!("Enabled rule '{name}'");
    Ok(())
}

pub fn run_disable(cwd: &Path, name: &str) -> Result<()> {
    let ctx = discover(cwd)?;
    let (_, project_db) = ctx.require_project()?;

    let count = project_db.set_rule_enabled(name, false)?;
    if count == 0 {
        bail!("no rule named '{name}'");
    }
    eprintln!("Disabled rule '{name}'");
    Ok(())
}

pub struct AddRuleParams<'a> {
    pub name: &'a str,
    pub on: &'a str,
    pub action: &'a str,
    pub tool: Option<&'a str>,
    pub tag: Option<&'a str>,
    pub category: Option<&'a str>,
    pub mime_type: Option<&'a str>,
    pub file_type: Option<&'a str>,
    pub trigger_tag: Option<&'a str>,
    pub trigger_pipeline: Option<&'a str>,
    pub trigger_sign: Option<&'a str>,
    pub trigger_state: Option<&'a str>,
    pub pipeline: Option<&'a str>,
    pub sign_name: Option<&'a str>,
    pub priority: i32,
}

fn validate_action_params(action_type: ActionType, params: &AddRuleParams<'_>) -> Result<()> {
    match action_type {
        ActionType::RunTool => {
            if params.tool.is_none() {
                bail!("--tool is required for run-tool action");
            }
        }
        ActionType::AddTag | ActionType::RemoveTag => {
            if params.tag.is_none() {
                bail!("--tag is required for {action_type} action");
            }
        }
        ActionType::Sign | ActionType::Unsign => {
            if params.pipeline.is_none() {
                bail!("--pipeline is required for {action_type} action");
            }
            if params.sign_name.is_none() {
                bail!("--sign-name is required for {action_type} action");
            }
        }
        ActionType::AttachPipeline | ActionType::DetachPipeline => {
            if params.pipeline.is_none() {
                bail!("--pipeline is required for {action_type} action");
            }
            if params.category.is_none() && params.tag.is_none() {
                bail!(
                    "--category or --tag is required for {action_type} action (attachment scope)"
                );
            }
        }
    }
    Ok(())
}

fn build_action_config(action_type: ActionType, params: &AddRuleParams<'_>) -> ActionConfig {
    let base = ActionConfig {
        tool: None,
        tag: None,
        pipeline: None,
        sign_name: None,
        category: None,
    };
    match action_type {
        ActionType::RunTool => ActionConfig {
            tool: Some(params.tool.unwrap_or_default().to_string()),
            ..base
        },
        ActionType::AddTag | ActionType::RemoveTag => ActionConfig {
            tag: Some(params.tag.unwrap_or_default().to_string()),
            ..base
        },
        ActionType::Sign | ActionType::Unsign => ActionConfig {
            pipeline: Some(params.pipeline.unwrap_or_default().to_string()),
            sign_name: Some(params.sign_name.unwrap_or_default().to_string()),
            ..base
        },
        ActionType::AttachPipeline | ActionType::DetachPipeline => ActionConfig {
            pipeline: Some(params.pipeline.unwrap_or_default().to_string()),
            tag: params.tag.map(str::to_string),
            category: params.category.map(str::to_string),
            ..base
        },
    }
}

fn action_target(config: &ActionConfig) -> String {
    if let Some(ref tool) = config.tool {
        return tool.clone();
    }
    if let Some(ref pipeline) = config.pipeline {
        if let Some(ref sign) = config.sign_name {
            return format!("'{sign}' in '{pipeline}'");
        }
        let scope = config
            .category
            .as_deref()
            .or(config.tag.as_deref())
            .unwrap_or("?");
        return format!("'{pipeline}' on {scope}");
    }
    if let Some(ref tag) = config.tag {
        return format!("'{tag}'");
    }
    String::new()
}

fn format_filter(filter: &TriggerFilter) -> String {
    if filter.is_empty() {
        return String::new();
    }
    let mut parts = Vec::new();
    if let Some(ref cat) = filter.category {
        parts.push(format!("cat={cat}"));
    }
    if let Some(ref tag) = filter.tag_name {
        parts.push(format!("tag={tag}"));
    }
    if let Some(ref mime) = filter.mime_type {
        parts.push(format!("mime={mime}"));
    }
    if let Some(ref ft) = filter.file_type {
        parts.push(format!("ext={ft}"));
    }
    if let Some(ref p) = filter.pipeline {
        parts.push(format!("pipeline={p}"));
    }
    if let Some(ref s) = filter.sign_name {
        parts.push(format!("sign={s}"));
    }
    if let Some(ref st) = filter.state {
        parts.push(format!("state={st}"));
    }
    format!("[{}] ", parts.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params_with<'a>(
        action: &'a str,
        tool: Option<&'a str>,
        tag: Option<&'a str>,
    ) -> AddRuleParams<'a> {
        AddRuleParams {
            name: "test",
            on: "ingest",
            action,
            tool,
            tag,
            category: None,
            mime_type: None,
            file_type: None,
            trigger_tag: None,
            trigger_pipeline: None,
            trigger_sign: None,
            trigger_state: None,
            pipeline: None,
            sign_name: None,
            priority: 0,
        }
    }

    #[test]
    fn validate_run_tool_requires_tool() {
        let p = params_with("run-tool", None, None);
        let err = validate_action_params(ActionType::RunTool, &p);
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("--tool"));
    }

    #[test]
    fn validate_run_tool_accepts_tool() {
        let p = params_with("run-tool", Some("ocr"), None);
        assert!(validate_action_params(ActionType::RunTool, &p).is_ok());
    }

    #[test]
    fn validate_add_tag_requires_tag() {
        let p = params_with("add-tag", None, None);
        let err = validate_action_params(ActionType::AddTag, &p);
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("--tag"));
    }

    #[test]
    fn validate_add_tag_accepts_tag() {
        let p = params_with("add-tag", None, Some("reviewed"));
        assert!(validate_action_params(ActionType::AddTag, &p).is_ok());
    }

    #[test]
    fn validate_remove_tag_requires_tag() {
        let p = params_with("remove-tag", None, None);
        let err = validate_action_params(ActionType::RemoveTag, &p);
        assert!(err.is_err());
    }

    #[test]
    fn validate_sign_requires_pipeline_and_sign_name() {
        let p = params_with("sign", None, None);
        let err = validate_action_params(ActionType::Sign, &p);
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("--pipeline"));

        let mut p2 = params_with("sign", None, None);
        p2.pipeline = Some("editorial");
        let err = validate_action_params(ActionType::Sign, &p2);
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("--sign-name"));

        let mut p3 = params_with("sign", None, None);
        p3.pipeline = Some("editorial");
        p3.sign_name = Some("review");
        assert!(validate_action_params(ActionType::Sign, &p3).is_ok());
    }

    #[test]
    fn validate_attach_pipeline_requires_pipeline_and_scope() {
        let p = params_with("attach-pipeline", None, None);
        let err = validate_action_params(ActionType::AttachPipeline, &p);
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("--pipeline"));

        let mut p2 = params_with("attach-pipeline", None, None);
        p2.pipeline = Some("editorial");
        let err = validate_action_params(ActionType::AttachPipeline, &p2);
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("--category or --tag"));

        let mut p3 = params_with("attach-pipeline", None, Some("classified"));
        p3.pipeline = Some("editorial");
        assert!(validate_action_params(ActionType::AttachPipeline, &p3).is_ok());
    }

    #[test]
    fn action_target_tool() {
        let config = ActionConfig {
            tool: Some("ocr".to_string()),
            tag: None,
            pipeline: None,
            sign_name: None,
            category: None,
        };
        assert_eq!(action_target(&config), "ocr");
    }

    #[test]
    fn action_target_tag() {
        let config = ActionConfig {
            tool: None,
            tag: Some("reviewed".to_string()),
            pipeline: None,
            sign_name: None,
            category: None,
        };
        assert_eq!(action_target(&config), "'reviewed'");
    }

    #[test]
    fn action_target_pipeline_sign() {
        let config = ActionConfig {
            tool: None,
            tag: None,
            pipeline: Some("editorial".to_string()),
            sign_name: Some("review".to_string()),
            category: None,
        };
        assert_eq!(action_target(&config), "'review' in 'editorial'");
    }

    #[test]
    fn action_target_pipeline_scope() {
        let config = ActionConfig {
            tool: None,
            tag: Some("classified".to_string()),
            pipeline: Some("security".to_string()),
            sign_name: None,
            category: None,
        };
        assert_eq!(action_target(&config), "'security' on classified");
    }

    #[test]
    fn action_target_empty() {
        let config = ActionConfig {
            tool: None,
            tag: None,
            pipeline: None,
            sign_name: None,
            category: None,
        };
        assert_eq!(action_target(&config), "");
    }

    #[test]
    fn format_filter_empty() {
        let filter = TriggerFilter::default();
        assert_eq!(format_filter(&filter), "");
    }

    #[test]
    fn format_filter_single_field() {
        let filter = TriggerFilter {
            category: Some("evidence".to_string()),
            ..Default::default()
        };
        assert_eq!(format_filter(&filter), "[cat=evidence] ");
    }

    #[test]
    fn format_filter_multiple_fields() {
        let filter = TriggerFilter {
            category: Some("evidence".to_string()),
            mime_type: Some("application/pdf".to_string()),
            file_type: Some("pdf".to_string()),
            ..Default::default()
        };
        assert_eq!(
            format_filter(&filter),
            "[cat=evidence, mime=application/pdf, ext=pdf] "
        );
    }

    #[test]
    fn format_filter_all_fields() {
        let filter = TriggerFilter {
            category: Some("ev".to_string()),
            tag_name: Some("speech".to_string()),
            mime_type: Some("audio/wav".to_string()),
            file_type: Some("wav".to_string()),
            pipeline: Some("editorial".to_string()),
            sign_name: Some("review".to_string()),
            state: Some("reviewed".to_string()),
        };
        let result = format_filter(&filter);
        assert!(result.contains("cat=ev"));
        assert!(result.contains("tag=speech"));
        assert!(result.contains("mime=audio/wav"));
        assert!(result.contains("ext=wav"));
        assert!(result.contains("pipeline=editorial"));
        assert!(result.contains("sign=review"));
        assert!(result.contains("state=reviewed"));
    }
}
