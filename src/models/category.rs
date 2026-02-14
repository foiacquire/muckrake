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

#[derive(Debug, Clone)]
pub struct Category {
    pub id: Option<i64>,
    pub name: String,
    pub pattern: String,
    pub category_type: CategoryType,
    pub description: Option<String>,
}

impl Category {
    pub fn matches(&self, path: &str) -> Result<bool, glob::PatternError> {
        glob::Pattern::new(&self.pattern).map(|p| p.matches(path))
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
    fn category_type_roundtrip() {
        for ct in &[
            CategoryType::Files,
            CategoryType::Tools,
            CategoryType::Inbox,
        ] {
            let s = ct.to_string();
            let parsed: CategoryType = s.parse().unwrap();
            assert_eq!(&parsed, ct);
        }
    }

    #[test]
    fn category_type_invalid() {
        assert!("bogus".parse::<CategoryType>().is_err());
    }

    #[test]
    fn category_type_default() {
        assert_eq!(CategoryType::default(), CategoryType::Files);
    }

    #[test]
    fn category_glob_matching() {
        let cat = Category {
            id: None,
            name: "evidence".to_string(),
            pattern: "evidence/**".to_string(),
            category_type: CategoryType::Files,
            description: None,
        };
        assert!(cat.matches("evidence/doc.pdf").unwrap());
        assert!(cat.matches("evidence/financial/receipt.pdf").unwrap());
        assert!(!cat.matches("notes/todo.md").unwrap());
    }

    #[test]
    fn category_exact_pattern() {
        let cat = Category {
            id: None,
            name: "notes".to_string(),
            pattern: "notes/**".to_string(),
            category_type: CategoryType::Files,
            description: None,
        };
        assert!(cat.matches("notes/daily.md").unwrap());
        assert!(!cat.matches("evidence/file.pdf").unwrap());
    }

    #[test]
    fn name_from_pattern_strips_glob() {
        assert_eq!(Category::name_from_pattern("evidence/**"), "evidence");
        assert_eq!(Category::name_from_pattern("tools/*"), "tools");
        assert_eq!(Category::name_from_pattern("inbox"), "inbox");
        assert_eq!(
            Category::name_from_pattern("evidence/financial/**"),
            "evidence/financial"
        );
    }
}
