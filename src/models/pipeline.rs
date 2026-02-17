use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;

use anyhow::bail;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AttachmentScope {
    Category,
    Tag,
}

impl fmt::Display for AttachmentScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Category => write!(f, "category"),
            Self::Tag => write!(f, "tag"),
        }
    }
}

impl FromStr for AttachmentScope {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "category" => Ok(Self::Category),
            "tag" => Ok(Self::Tag),
            other => bail!("unknown attachment scope: '{other}' (expected: category, tag)"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Pipeline {
    pub id: Option<i64>,
    pub name: String,
    pub states: Vec<String>,
    pub transitions: HashMap<String, Vec<String>>,
}

impl Pipeline {
    pub fn default_transitions(states: &[String]) -> HashMap<String, Vec<String>> {
        let mut transitions = HashMap::new();
        for state in states.iter().skip(1) {
            transitions.insert(state.clone(), vec![state.clone()]);
        }
        transitions
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        if self.states.len() < 2 {
            bail!("pipeline must have at least 2 states");
        }

        let initial = &self.states[0];
        if self.transitions.contains_key(initial) {
            bail!("initial state '{initial}' must not have a transition entry");
        }

        for (target, required_signs) in &self.transitions {
            if !self.states.contains(target) {
                bail!("transition target '{target}' is not a defined state");
            }
            if required_signs.is_empty() {
                bail!("transition to '{target}' has no required signs");
            }
        }

        for state in self.states.iter().skip(1) {
            if !self.transitions.contains_key(state) {
                bail!("non-initial state '{state}' has no transition entry");
            }
        }

        Ok(())
    }

    pub fn required_sign_names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self
            .transitions
            .values()
            .flatten()
            .map(String::as_str)
            .collect();
        names.sort_unstable();
        names.dedup();
        names
    }
}

#[derive(Debug, Clone)]
pub struct PipelineAttachment {
    pub id: Option<i64>,
    pub pipeline_id: i64,
    pub scope_type: AttachmentScope,
    pub scope_value: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attachment_scope_roundtrip() {
        for scope in [AttachmentScope::Category, AttachmentScope::Tag] {
            let s = scope.to_string();
            let parsed: AttachmentScope = s.parse().unwrap();
            assert_eq!(parsed, scope);
        }
    }

    #[test]
    fn attachment_scope_invalid() {
        assert!("bogus".parse::<AttachmentScope>().is_err());
    }

    #[test]
    fn default_transitions_linear() {
        let states: Vec<String> = vec!["draft", "review", "published"]
            .into_iter()
            .map(String::from)
            .collect();
        let transitions = Pipeline::default_transitions(&states);

        assert!(!transitions.contains_key("draft"));
        assert_eq!(transitions["review"], vec!["review"]);
        assert_eq!(transitions["published"], vec!["published"]);
    }

    #[test]
    fn default_transitions_two_states() {
        let states: Vec<String> = vec!["pending", "done"]
            .into_iter()
            .map(String::from)
            .collect();
        let transitions = Pipeline::default_transitions(&states);

        assert!(!transitions.contains_key("pending"));
        assert_eq!(transitions["done"], vec!["done"]);
    }

    #[test]
    fn validate_valid_pipeline() {
        let pipeline = Pipeline {
            id: None,
            name: "test".to_string(),
            states: vec!["draft".into(), "review".into(), "published".into()],
            transitions: Pipeline::default_transitions(&[
                "draft".into(),
                "review".into(),
                "published".into(),
            ]),
        };
        assert!(pipeline.validate().is_ok());
    }

    #[test]
    fn validate_too_few_states() {
        let pipeline = Pipeline {
            id: None,
            name: "test".to_string(),
            states: vec!["only".into()],
            transitions: HashMap::new(),
        };
        let err = pipeline.validate().unwrap_err();
        assert!(err.to_string().contains("at least 2 states"));
    }

    #[test]
    fn validate_initial_state_has_transition() {
        let mut transitions = HashMap::new();
        transitions.insert("draft".to_string(), vec!["draft".to_string()]);
        transitions.insert("review".to_string(), vec!["review".to_string()]);

        let pipeline = Pipeline {
            id: None,
            name: "test".to_string(),
            states: vec!["draft".into(), "review".into()],
            transitions,
        };
        let err = pipeline.validate().unwrap_err();
        assert!(err.to_string().contains("initial state"));
    }

    #[test]
    fn validate_unknown_transition_target() {
        let mut transitions = HashMap::new();
        transitions.insert("nonexistent".to_string(), vec!["sign".to_string()]);

        let pipeline = Pipeline {
            id: None,
            name: "test".to_string(),
            states: vec!["draft".into(), "review".into()],
            transitions,
        };
        let err = pipeline.validate().unwrap_err();
        assert!(err.to_string().contains("not a defined state"));
    }

    #[test]
    fn validate_missing_transition_for_state() {
        let pipeline = Pipeline {
            id: None,
            name: "test".to_string(),
            states: vec!["draft".into(), "review".into(), "published".into()],
            transitions: {
                let mut t = HashMap::new();
                t.insert("review".to_string(), vec!["review".to_string()]);
                t
            },
        };
        let err = pipeline.validate().unwrap_err();
        assert!(err.to_string().contains("published"));
    }

    #[test]
    fn validate_empty_required_signs() {
        let mut transitions = HashMap::new();
        transitions.insert("review".to_string(), vec![]);

        let pipeline = Pipeline {
            id: None,
            name: "test".to_string(),
            states: vec!["draft".into(), "review".into()],
            transitions,
        };
        let err = pipeline.validate().unwrap_err();
        assert!(err.to_string().contains("no required signs"));
    }

    #[test]
    fn required_sign_names_deduplicates() {
        let mut transitions = HashMap::new();
        transitions.insert(
            "review".to_string(),
            vec!["editor".to_string(), "legal".to_string()],
        );
        transitions.insert("published".to_string(), vec!["legal".to_string()]);

        let pipeline = Pipeline {
            id: None,
            name: "test".to_string(),
            states: vec!["draft".into(), "review".into(), "published".into()],
            transitions,
        };
        let names = pipeline.required_sign_names();
        assert_eq!(names, vec!["editor", "legal"]);
    }

    #[test]
    fn custom_multi_sign_transitions() {
        let mut transitions = HashMap::new();
        transitions.insert(
            "reviewed".to_string(),
            vec!["editor_ok".to_string(), "legal_ok".to_string()],
        );
        transitions.insert("published".to_string(), vec!["publish_ok".to_string()]);

        let pipeline = Pipeline {
            id: None,
            name: "editorial".to_string(),
            states: vec!["draft".into(), "reviewed".into(), "published".into()],
            transitions,
        };
        assert!(pipeline.validate().is_ok());
    }
}
