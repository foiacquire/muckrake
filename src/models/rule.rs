use std::fmt;
use std::str::FromStr;

use anyhow::bail;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerEvent {
    Ingest,
    Tag,
    Untag,
    Categorize,
    Sign,
    StateChange,
    ProjectEnter,
    WorkspaceEnter,
}

impl fmt::Display for TriggerEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ingest => write!(f, "ingest"),
            Self::Tag => write!(f, "tag"),
            Self::Untag => write!(f, "untag"),
            Self::Categorize => write!(f, "categorize"),
            Self::Sign => write!(f, "sign"),
            Self::StateChange => write!(f, "state_change"),
            Self::ProjectEnter => write!(f, "project_enter"),
            Self::WorkspaceEnter => write!(f, "workspace_enter"),
        }
    }
}

impl FromStr for TriggerEvent {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "ingest" => Ok(Self::Ingest),
            "tag" => Ok(Self::Tag),
            "untag" => Ok(Self::Untag),
            "categorize" => Ok(Self::Categorize),
            "sign" => Ok(Self::Sign),
            "state_change" | "state-change" => Ok(Self::StateChange),
            "project_enter" | "project-enter" => Ok(Self::ProjectEnter),
            "workspace_enter" | "workspace-enter" => Ok(Self::WorkspaceEnter),
            other => {
                bail!(
                    "unknown trigger event: '{other}' (expected: ingest, tag, untag, \
                     categorize, sign, state_change, project_enter, workspace_enter)"
                )
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionType {
    RunTool,
    AddTag,
    RemoveTag,
    Sign,
    Unsign,
    AttachPipeline,
    DetachPipeline,
}

impl fmt::Display for ActionType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RunTool => write!(f, "run_tool"),
            Self::AddTag => write!(f, "add_tag"),
            Self::RemoveTag => write!(f, "remove_tag"),
            Self::Sign => write!(f, "sign"),
            Self::Unsign => write!(f, "unsign"),
            Self::AttachPipeline => write!(f, "attach_pipeline"),
            Self::DetachPipeline => write!(f, "detach_pipeline"),
        }
    }
}

impl FromStr for ActionType {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "run_tool" | "run-tool" => Ok(Self::RunTool),
            "add_tag" | "add-tag" => Ok(Self::AddTag),
            "remove_tag" | "remove-tag" => Ok(Self::RemoveTag),
            "sign" => Ok(Self::Sign),
            "unsign" => Ok(Self::Unsign),
            "attach_pipeline" | "attach-pipeline" => Ok(Self::AttachPipeline),
            "detach_pipeline" | "detach-pipeline" => Ok(Self::DetachPipeline),
            other => {
                bail!(
                    "unknown action type: '{other}' (expected: run-tool, add-tag, remove-tag, \
                     sign, unsign, attach-pipeline, detach-pipeline)"
                )
            }
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TriggerFilter {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pipeline: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sign_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
}

impl TriggerFilter {
    pub const fn is_empty(&self) -> bool {
        self.tag_name.is_none()
            && self.category.is_none()
            && self.mime_type.is_none()
            && self.file_type.is_none()
            && self.pipeline.is_none()
            && self.sign_name.is_none()
            && self.state.is_none()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pipeline: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sign_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Rule {
    pub id: Option<i64>,
    pub name: String,
    pub enabled: bool,
    pub trigger_event: TriggerEvent,
    pub trigger_filter: TriggerFilter,
    pub action_type: ActionType,
    pub action_config: ActionConfig,
    pub priority: i32,
    pub created_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trigger_event_roundtrip() {
        for event in [
            TriggerEvent::Ingest,
            TriggerEvent::Tag,
            TriggerEvent::Untag,
            TriggerEvent::Categorize,
            TriggerEvent::Sign,
            TriggerEvent::StateChange,
            TriggerEvent::ProjectEnter,
            TriggerEvent::WorkspaceEnter,
        ] {
            let s = event.to_string();
            let parsed: TriggerEvent = s.parse().unwrap();
            assert_eq!(parsed, event);
        }
    }

    #[test]
    fn trigger_event_accepts_dashes() {
        assert_eq!(
            "state-change".parse::<TriggerEvent>().unwrap(),
            TriggerEvent::StateChange
        );
        assert_eq!(
            "project-enter".parse::<TriggerEvent>().unwrap(),
            TriggerEvent::ProjectEnter
        );
        assert_eq!(
            "workspace-enter".parse::<TriggerEvent>().unwrap(),
            TriggerEvent::WorkspaceEnter
        );
    }

    #[test]
    fn trigger_event_invalid() {
        assert!("bogus".parse::<TriggerEvent>().is_err());
    }

    #[test]
    fn action_type_roundtrip() {
        for action in [
            ActionType::RunTool,
            ActionType::AddTag,
            ActionType::RemoveTag,
            ActionType::Sign,
            ActionType::Unsign,
            ActionType::AttachPipeline,
            ActionType::DetachPipeline,
        ] {
            let s = action.to_string();
            let parsed: ActionType = s.parse().unwrap();
            assert_eq!(parsed, action);
        }
    }

    #[test]
    fn action_type_accepts_dashes() {
        assert_eq!(
            "run-tool".parse::<ActionType>().unwrap(),
            ActionType::RunTool
        );
        assert_eq!("add-tag".parse::<ActionType>().unwrap(), ActionType::AddTag);
        assert_eq!(
            "remove-tag".parse::<ActionType>().unwrap(),
            ActionType::RemoveTag,
        );
        assert_eq!(
            "attach-pipeline".parse::<ActionType>().unwrap(),
            ActionType::AttachPipeline,
        );
        assert_eq!(
            "detach-pipeline".parse::<ActionType>().unwrap(),
            ActionType::DetachPipeline,
        );
    }

    #[test]
    fn action_type_invalid() {
        assert!("bogus".parse::<ActionType>().is_err());
    }

    #[test]
    fn trigger_filter_empty() {
        let filter = TriggerFilter::default();
        assert!(filter.is_empty());
    }

    #[test]
    fn trigger_filter_not_empty() {
        let filter = TriggerFilter {
            category: Some("evidence".to_string()),
            ..Default::default()
        };
        assert!(!filter.is_empty());
    }

    #[test]
    fn trigger_filter_serde_roundtrip() {
        let filter = TriggerFilter {
            category: Some("evidence".to_string()),
            mime_type: Some("application/pdf".to_string()),
            ..Default::default()
        };
        let json = serde_json::to_string(&filter).unwrap();
        let parsed: TriggerFilter = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, filter);
    }

    #[test]
    fn trigger_filter_serde_skips_none() {
        let filter = TriggerFilter {
            category: Some("evidence".to_string()),
            ..Default::default()
        };
        let json = serde_json::to_string(&filter).unwrap();
        assert!(!json.contains("tag_name"));
        assert!(!json.contains("mime_type"));
        assert!(!json.contains("file_type"));
    }

    #[test]
    fn trigger_filter_pipeline_fields() {
        let filter = TriggerFilter {
            pipeline: Some("editorial".to_string()),
            sign_name: Some("review".to_string()),
            state: Some("reviewed".to_string()),
            ..Default::default()
        };
        assert!(!filter.is_empty());

        let json = serde_json::to_string(&filter).unwrap();
        assert!(!json.contains("tag_name"));
        assert!(json.contains("pipeline"));
        assert!(json.contains("sign_name"));
        assert!(json.contains("state"));

        let parsed: TriggerFilter = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, filter);
    }

    #[test]
    fn action_config_serde_roundtrip() {
        let config = ActionConfig {
            tool: Some("ocr".to_string()),
            tag: None,
            pipeline: None,
            sign_name: None,
            category: None,
        };
        let json = serde_json::to_string(&config).unwrap();
        let parsed: ActionConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, config);
    }

    #[test]
    fn action_config_pipeline_fields() {
        let config = ActionConfig {
            tool: None,
            tag: None,
            pipeline: Some("editorial".to_string()),
            sign_name: Some("review".to_string()),
            category: Some("evidence".to_string()),
        };
        let json = serde_json::to_string(&config).unwrap();
        assert!(!json.contains("tool"));
        assert!(json.contains("pipeline"));
        assert!(json.contains("sign_name"));
        assert!(json.contains("category"));

        let parsed: ActionConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, config);
    }
}
