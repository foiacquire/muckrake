use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CategoryType {
    #[default]
    Files,
    Tools,
    Inbox,
}

impl fmt::Display for CategoryType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Files => write!(f, "files"),
            Self::Tools => write!(f, "tools"),
            Self::Inbox => write!(f, "inbox"),
        }
    }
}

impl FromStr for CategoryType {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "files" => Ok(Self::Files),
            "tools" => Ok(Self::Tools),
            "inbox" => Ok(Self::Inbox),
            other => Err(anyhow::anyhow!("unknown category type: {other}")),
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScopeType {
    #[default]
    Category,
    Tag,
    Project,
}

impl fmt::Display for ScopeType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Category => write!(f, "category"),
            Self::Tag => write!(f, "tag"),
            Self::Project => write!(f, "project"),
        }
    }
}

impl FromStr for ScopeType {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "category" => Ok(Self::Category),
            "tag" => Ok(Self::Tag),
            "project" => Ok(Self::Project),
            other => Err(anyhow::anyhow!("unknown scope type: {other}")),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Scope {
    pub id: Option<i64>,
    pub name: String,
    pub scope_type: ScopeType,
    pub pattern: Option<String>,
    pub category_type: Option<CategoryType>,
    pub description: Option<String>,
    pub created_at: Option<String>,
}

impl Scope {
    pub fn matches(&self, path: &str) -> Result<bool, glob::PatternError> {
        match &self.pattern {
            Some(pattern) => glob::Pattern::new(pattern).map(|p| p.matches(path)),
            None => Ok(false),
        }
    }

    pub fn name_from_pattern(pattern: &str) -> String {
        pattern
            .strip_suffix("/**")
            .or_else(|| pattern.strip_suffix("/*"))
            .unwrap_or(pattern)
            .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_type_roundtrip() {
        for st in &[ScopeType::Category, ScopeType::Tag, ScopeType::Project] {
            let s = st.to_string();
            let parsed: ScopeType = s.parse().unwrap();
            assert_eq!(&parsed, st);
        }
    }

    #[test]
    fn scope_type_invalid() {
        assert!("bogus".parse::<ScopeType>().is_err());
    }

    #[test]
    fn scope_glob_matching() {
        let scope = Scope {
            id: None,
            name: "evidence".to_string(),
            scope_type: ScopeType::Category,
            pattern: Some("evidence/**".to_string()),
            category_type: Some(CategoryType::Files),
            description: None,
            created_at: None,
        };
        assert!(scope.matches("evidence/doc.pdf").unwrap());
        assert!(scope.matches("evidence/financial/receipt.pdf").unwrap());
        assert!(!scope.matches("notes/todo.md").unwrap());
    }

    #[test]
    fn tag_scope_never_matches_path() {
        let scope = Scope {
            id: None,
            name: "classified".to_string(),
            scope_type: ScopeType::Tag,
            pattern: None,
            category_type: None,
            description: None,
            created_at: None,
        };
        assert!(!scope.matches("anything").unwrap());
    }

    #[test]
    fn name_from_pattern_strips_glob() {
        assert_eq!(Scope::name_from_pattern("evidence/**"), "evidence");
        assert_eq!(Scope::name_from_pattern("tools/*"), "tools");
        assert_eq!(Scope::name_from_pattern("inbox"), "inbox");
        assert_eq!(
            Scope::name_from_pattern("evidence/financial/**"),
            "evidence/financial"
        );
    }
}
