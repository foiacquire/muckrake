use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProtectionLevel {
    Immutable,
    Protected,
    Editable,
}

impl fmt::Display for ProtectionLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Immutable => write!(f, "immutable"),
            Self::Protected => write!(f, "protected"),
            Self::Editable => write!(f, "editable"),
        }
    }
}

impl FromStr for ProtectionLevel {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "immutable" => Ok(Self::Immutable),
            "protected" => Ok(Self::Protected),
            "editable" => Ok(Self::Editable),
            other => Err(anyhow::anyhow!("unknown protection level: {other}")),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Category {
    pub id: Option<i64>,
    pub pattern: String,
    pub protection_level: ProtectionLevel,
    pub description: Option<String>,
}

impl Category {
    pub fn matches(&self, path: &str) -> bool {
        glob::Pattern::new(&self.pattern)
            .map(|p| p.matches(path))
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protection_level_roundtrip() {
        for level in &[
            ProtectionLevel::Immutable,
            ProtectionLevel::Protected,
            ProtectionLevel::Editable,
        ] {
            let s = level.to_string();
            let parsed: ProtectionLevel = s.parse().unwrap();
            assert_eq!(&parsed, level);
        }
    }

    #[test]
    fn protection_level_invalid() {
        assert!("bogus".parse::<ProtectionLevel>().is_err());
    }

    #[test]
    fn category_glob_matching() {
        let cat = Category {
            id: None,
            pattern: "evidence/**".to_string(),
            protection_level: ProtectionLevel::Immutable,
            description: None,
        };
        assert!(cat.matches("evidence/doc.pdf"));
        assert!(cat.matches("evidence/financial/receipt.pdf"));
        assert!(!cat.matches("notes/todo.md"));
    }

    #[test]
    fn category_exact_pattern() {
        let cat = Category {
            id: None,
            pattern: "notes/**".to_string(),
            protection_level: ProtectionLevel::Editable,
            description: None,
        };
        assert!(cat.matches("notes/daily.md"));
        assert!(!cat.matches("evidence/file.pdf"));
    }
}
