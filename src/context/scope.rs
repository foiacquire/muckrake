use anyhow::Result;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scope {
    Current,
    Workspace,
    Project { name: String, path: Vec<String> },
}

impl Scope {
    pub fn parse(s: &str) -> Result<Self> {
        if !s.starts_with(':') {
            anyhow::bail!("scope must start with ':'");
        }

        let rest = &s[1..];
        if rest.is_empty() {
            return Ok(Self::Workspace);
        }

        let parts: Vec<&str> = rest.split('.').collect();
        if parts.is_empty() || parts[0].is_empty() {
            return Ok(Self::Workspace);
        }

        let name = parts[0].to_string();
        let path = parts[1..].iter().map(|s| (*s).to_string()).collect();

        Ok(Self::Project { name, path })
    }

    pub fn as_path_prefix(&self) -> Option<String> {
        match self {
            Self::Current | Self::Workspace => None,
            Self::Project { path, .. } => {
                if path.is_empty() {
                    None
                } else {
                    Some(format!("{}/", path.join("/")))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_workspace_scope() {
        assert_eq!(Scope::parse(":").unwrap(), Scope::Workspace);
    }

    #[test]
    fn parse_project_scope() {
        let scope = Scope::parse(":bailey").unwrap();
        assert_eq!(
            scope,
            Scope::Project {
                name: "bailey".to_string(),
                path: vec![],
            }
        );
    }

    #[test]
    fn parse_project_with_path() {
        let scope = Scope::parse(":bailey.evidence.financial").unwrap();
        assert_eq!(
            scope,
            Scope::Project {
                name: "bailey".to_string(),
                path: vec!["evidence".to_string(), "financial".to_string()],
            }
        );
    }

    #[test]
    fn as_path_prefix_none_for_current() {
        assert!(Scope::Current.as_path_prefix().is_none());
    }

    #[test]
    fn as_path_prefix_none_for_workspace() {
        assert!(Scope::Workspace.as_path_prefix().is_none());
    }

    #[test]
    fn as_path_prefix_none_for_project_root() {
        let scope = Scope::Project {
            name: "bailey".to_string(),
            path: vec![],
        };
        assert!(scope.as_path_prefix().is_none());
    }

    #[test]
    fn as_path_prefix_with_category() {
        let scope = Scope::Project {
            name: "bailey".to_string(),
            path: vec!["evidence".to_string(), "financial".to_string()],
        };
        assert_eq!(scope.as_path_prefix().unwrap(), "evidence/financial/");
    }

    #[test]
    fn parse_invalid_no_colon() {
        assert!(Scope::parse("bailey").is_err());
    }
}
