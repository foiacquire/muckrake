use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct Ruleset {
    pub id: Option<i64>,
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RulesetRule {
    pub id: Option<i64>,
    pub ruleset_id: i64,
    pub priority: i32,
    pub condition: Option<RuleCondition>,
    pub action_type: RulesetActionType,
    pub action_config: RulesetActionConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleCondition {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_type: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RulesetActionType {
    ApplyPolicy,
    DispatchTool,
    AddTag,
    RemoveTag,
    Sign,
    Unsign,
    AttachPipeline,
    AttachPipelineVirtual,
}

impl fmt::Display for RulesetActionType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ApplyPolicy => write!(f, "apply_policy"),
            Self::DispatchTool => write!(f, "dispatch_tool"),
            Self::AddTag => write!(f, "add_tag"),
            Self::RemoveTag => write!(f, "remove_tag"),
            Self::Sign => write!(f, "sign"),
            Self::Unsign => write!(f, "unsign"),
            Self::AttachPipeline => write!(f, "attach_pipeline"),
            Self::AttachPipelineVirtual => write!(f, "attach_pipeline_virtual"),
        }
    }
}

impl FromStr for RulesetActionType {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "apply_policy" => Ok(Self::ApplyPolicy),
            "dispatch_tool" => Ok(Self::DispatchTool),
            "add_tag" => Ok(Self::AddTag),
            "remove_tag" => Ok(Self::RemoveTag),
            "sign" => Ok(Self::Sign),
            "unsign" => Ok(Self::Unsign),
            "attach_pipeline" => Ok(Self::AttachPipeline),
            "attach_pipeline_virtual" => Ok(Self::AttachPipelineVirtual),
            other => Err(anyhow::anyhow!("unknown ruleset action type: {other}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RulesetActionConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protection_level: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quiet: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pipeline: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sign_name: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Subscription {
    pub id: Option<i64>,
    pub reference: String,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct MaterializedFile {
    pub sha256: String,
    pub subscription_id: i64,
    pub attached_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_type_roundtrip() {
        for at in &[
            RulesetActionType::ApplyPolicy,
            RulesetActionType::DispatchTool,
            RulesetActionType::AddTag,
            RulesetActionType::RemoveTag,
            RulesetActionType::Sign,
            RulesetActionType::Unsign,
            RulesetActionType::AttachPipeline,
            RulesetActionType::AttachPipelineVirtual,
        ] {
            let s = at.to_string();
            let parsed: RulesetActionType = s.parse().unwrap();
            assert_eq!(&parsed, at);
        }
    }

    #[test]
    fn action_type_invalid() {
        assert!("bogus".parse::<RulesetActionType>().is_err());
    }
}
