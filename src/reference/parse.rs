use anyhow::{bail, Result};

use super::types::{Reference, ScopeLevel, TagFilter};

const RESERVED_CHARS: &[char] = &[':', '.', '/', '!', '{', '}', ','];
const RESERVED_NAMES: &[&str] = &["mkrk"];

pub fn validate_name(name: &str) -> Result<()> {
    if name.is_empty() {
        bail!("name must not be empty");
    }
    for ch in RESERVED_CHARS {
        if name.contains(*ch) {
            bail!("name '{name}' contains reserved character '{ch}'");
        }
    }
    if RESERVED_NAMES.contains(&name) {
        bail!("name '{name}' is reserved for internal use");
    }
    Ok(())
}

pub fn is_reserved_name(name: &str) -> bool {
    RESERVED_NAMES.contains(&name)
}

pub fn parse_reference(input: &str) -> Result<Reference> {
    if !input.starts_with(':') {
        return Ok(Reference::BarePath(input.to_string()));
    }

    let rest = &input[1..];
    let mut pos = 0;
    let bytes = rest.as_bytes();

    let scope = parse_scope(rest, &mut pos)?;
    let tags = parse_tags(rest, &mut pos)?;
    let glob = parse_glob(rest, &mut pos);

    if pos < bytes.len() {
        bail!(
            "unexpected character '{}' at position {} in reference '{input}'",
            bytes[pos] as char,
            pos + 1
        );
    }

    Ok(Reference::Structured { scope, tags, glob })
}

fn parse_scope(input: &str, pos: &mut usize) -> Result<Vec<ScopeLevel>> {
    let mut levels = Vec::new();

    loop {
        if *pos >= input.len() {
            break;
        }

        let ch = input.as_bytes()[*pos];
        if ch == b'!' || ch == b'/' {
            break;
        }

        if ch == b'.' && levels.is_empty() {
            *pos += 1;
            continue;
        }

        let level = parse_scope_level(input, pos)?;
        levels.push(level);

        if *pos < input.len() && input.as_bytes()[*pos] == b'.' {
            *pos += 1;
        } else {
            break;
        }
    }

    Ok(levels)
}

fn parse_scope_level(input: &str, pos: &mut usize) -> Result<ScopeLevel> {
    if *pos >= input.len() {
        bail!("expected scope name");
    }

    if input.as_bytes()[*pos] == b'{' {
        *pos += 1;
        let mut names = Vec::new();

        loop {
            let name = parse_name(input, pos);
            if name.is_empty() {
                bail!("empty name in brace expansion");
            }
            names.push(name);

            if *pos >= input.len() {
                bail!("unclosed brace expansion");
            }
            if input.as_bytes()[*pos] == b'}' {
                *pos += 1;
                break;
            }
            if input.as_bytes()[*pos] == b',' {
                *pos += 1;
            } else {
                bail!("expected ',' or '}}' in brace expansion");
            }
        }

        Ok(ScopeLevel { names })
    } else {
        let name = parse_name(input, pos);
        if name.is_empty() {
            bail!("expected scope name");
        }
        Ok(ScopeLevel { names: vec![name] })
    }
}

fn parse_name(input: &str, pos: &mut usize) -> String {
    let start = *pos;
    while *pos < input.len() {
        let ch = input.as_bytes()[*pos];
        if ch == b'.' || ch == b'!' || ch == b'/' || ch == b'{' || ch == b'}' || ch == b',' {
            break;
        }
        *pos += 1;
    }
    input[start..*pos].to_string()
}

fn parse_tags(input: &str, pos: &mut usize) -> Result<Vec<TagFilter>> {
    let mut filters = Vec::new();

    while *pos < input.len() && input.as_bytes()[*pos] == b'!' {
        *pos += 1;
        let mut tags = Vec::new();

        loop {
            let tag = parse_name(input, pos);
            if tag.is_empty() {
                bail!("empty tag name");
            }
            tags.push(tag);

            if *pos < input.len() && input.as_bytes()[*pos] == b',' {
                *pos += 1;
            } else {
                break;
            }
        }

        filters.push(TagFilter { tags });
    }

    Ok(filters)
}

fn parse_glob(input: &str, pos: &mut usize) -> Option<String> {
    if *pos < input.len() && input.as_bytes()[*pos] == b'/' {
        *pos += 1;
        let glob = input[*pos..].to_string();
        *pos = input.len();
        Some(glob)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_bare_path() {
        let r = parse_reference("evidence/report.pdf").unwrap();
        assert_eq!(r, Reference::BarePath("evidence/report.pdf".to_string()));
    }

    #[test]
    fn parse_current_project() {
        let r = parse_reference(":evidence").unwrap();
        assert_eq!(
            r,
            Reference::Structured {
                scope: vec![ScopeLevel {
                    names: vec!["evidence".to_string()]
                }],
                tags: vec![],
                glob: None,
            }
        );
    }

    #[test]
    fn parse_project_category() {
        let r = parse_reference(":bailey.evidence").unwrap();
        assert_eq!(
            r,
            Reference::Structured {
                scope: vec![
                    ScopeLevel {
                        names: vec!["bailey".to_string()]
                    },
                    ScopeLevel {
                        names: vec!["evidence".to_string()]
                    },
                ],
                tags: vec![],
                glob: None,
            }
        );
    }

    #[test]
    fn parse_brace_expansion() {
        let r = parse_reference(":{bailey,george}.evidence").unwrap();
        assert_eq!(
            r,
            Reference::Structured {
                scope: vec![
                    ScopeLevel {
                        names: vec!["bailey".to_string(), "george".to_string()]
                    },
                    ScopeLevel {
                        names: vec!["evidence".to_string()]
                    },
                ],
                tags: vec![],
                glob: None,
            }
        );
    }

    #[test]
    fn parse_cross_expansion() {
        let r = parse_reference(":{bailey,george}.{sources,evidence}").unwrap();
        assert_eq!(
            r,
            Reference::Structured {
                scope: vec![
                    ScopeLevel {
                        names: vec!["bailey".to_string(), "george".to_string()]
                    },
                    ScopeLevel {
                        names: vec!["sources".to_string(), "evidence".to_string()]
                    },
                ],
                tags: vec![],
                glob: None,
            }
        );
    }

    #[test]
    fn parse_tags_and() {
        let r = parse_reference(":george!bailey!classified").unwrap();
        assert_eq!(
            r,
            Reference::Structured {
                scope: vec![ScopeLevel {
                    names: vec!["george".to_string()]
                }],
                tags: vec![
                    TagFilter {
                        tags: vec!["bailey".to_string()]
                    },
                    TagFilter {
                        tags: vec!["classified".to_string()]
                    },
                ],
                glob: None,
            }
        );
    }

    #[test]
    fn parse_tags_or() {
        let r = parse_reference(":george!bailey,classified").unwrap();
        assert_eq!(
            r,
            Reference::Structured {
                scope: vec![ScopeLevel {
                    names: vec!["george".to_string()]
                }],
                tags: vec![TagFilter {
                    tags: vec!["bailey".to_string(), "classified".to_string()]
                }],
                glob: None,
            }
        );
    }

    #[test]
    fn parse_tags_mixed() {
        let r = parse_reference(":george!bailey,classified!priority").unwrap();
        assert_eq!(
            r,
            Reference::Structured {
                scope: vec![ScopeLevel {
                    names: vec!["george".to_string()]
                }],
                tags: vec![
                    TagFilter {
                        tags: vec!["bailey".to_string(), "classified".to_string()]
                    },
                    TagFilter {
                        tags: vec!["priority".to_string()]
                    },
                ],
                glob: None,
            }
        );
    }

    #[test]
    fn parse_glob() {
        let r = parse_reference(":evidence/*.pdf").unwrap();
        assert_eq!(
            r,
            Reference::Structured {
                scope: vec![ScopeLevel {
                    names: vec!["evidence".to_string()]
                }],
                tags: vec![],
                glob: Some("*.pdf".to_string()),
            }
        );
    }

    #[test]
    fn parse_full_reference() {
        let r = parse_reference(":{bailey,george}.evidence!classified/*.pdf").unwrap();
        assert_eq!(
            r,
            Reference::Structured {
                scope: vec![
                    ScopeLevel {
                        names: vec!["bailey".to_string(), "george".to_string()]
                    },
                    ScopeLevel {
                        names: vec!["evidence".to_string()]
                    },
                ],
                tags: vec![TagFilter {
                    tags: vec!["classified".to_string()]
                }],
                glob: Some("*.pdf".to_string()),
            }
        );
    }

    #[test]
    fn parse_workspace_scope() {
        let r = parse_reference(":").unwrap();
        assert_eq!(
            r,
            Reference::Structured {
                scope: vec![],
                tags: vec![],
                glob: None,
            }
        );
    }

    #[test]
    fn parse_leading_dot() {
        let r = parse_reference(":.sources").unwrap();
        assert_eq!(
            r,
            Reference::Structured {
                scope: vec![ScopeLevel {
                    names: vec!["sources".to_string()]
                }],
                tags: vec![],
                glob: None,
            }
        );
    }

    #[test]
    fn parse_reserved_in_name() {
        assert!(validate_name("foo:bar").is_err());
        assert!(validate_name("foo.bar").is_err());
        assert!(validate_name("foo/bar").is_err());
        assert!(validate_name("foo!bar").is_err());
        assert!(validate_name("foo{bar").is_err());
        assert!(validate_name("foo}bar").is_err());
        assert!(validate_name("foo,bar").is_err());
        assert!(validate_name("").is_err());
    }

    #[test]
    fn validate_name_ok() {
        assert!(validate_name("evidence").is_ok());
        assert!(validate_name("my-project").is_ok());
        assert!(validate_name("project_2024").is_ok());
    }

    #[test]
    fn validate_reserved_name() {
        assert!(validate_name("mkrk").is_err());
    }

    #[test]
    fn is_reserved_name_check() {
        assert!(is_reserved_name("mkrk"));
        assert!(!is_reserved_name("evidence"));
        assert!(!is_reserved_name("my-project"));
    }

    #[test]
    fn parse_glob_with_brace_expansion() {
        let r = parse_reference(":evidence/*_{response,request}.md").unwrap();
        assert_eq!(
            r,
            Reference::Structured {
                scope: vec![ScopeLevel {
                    names: vec!["evidence".to_string()]
                }],
                tags: vec![],
                glob: Some("*_{response,request}.md".to_string()),
            }
        );
    }

    #[test]
    fn parse_tags_then_glob() {
        let r = parse_reference(":evidence!classified/*.pdf").unwrap();
        assert_eq!(
            r,
            Reference::Structured {
                scope: vec![ScopeLevel {
                    names: vec!["evidence".to_string()]
                }],
                tags: vec![TagFilter {
                    tags: vec!["classified".to_string()]
                }],
                glob: Some("*.pdf".to_string()),
            }
        );
    }

    #[test]
    fn parse_only_tags() {
        let r = parse_reference(":!classified").unwrap();
        assert_eq!(
            r,
            Reference::Structured {
                scope: vec![],
                tags: vec![TagFilter {
                    tags: vec!["classified".to_string()]
                }],
                glob: None,
            }
        );
    }

    #[test]
    fn parse_only_glob() {
        let r = parse_reference(":/*.pdf").unwrap();
        assert_eq!(
            r,
            Reference::Structured {
                scope: vec![],
                tags: vec![],
                glob: Some("*.pdf".to_string()),
            }
        );
    }

    #[test]
    fn parse_single_project_all_files() {
        let r = parse_reference(":bailey").unwrap();
        assert_eq!(
            r,
            Reference::Structured {
                scope: vec![ScopeLevel {
                    names: vec!["bailey".to_string()]
                }],
                tags: vec![],
                glob: None,
            }
        );
    }

    #[test]
    fn parse_multi_project_no_category() {
        let r = parse_reference(":{bailey,george}").unwrap();
        assert_eq!(
            r,
            Reference::Structured {
                scope: vec![ScopeLevel {
                    names: vec!["bailey".to_string(), "george".to_string()]
                }],
                tags: vec![],
                glob: None,
            }
        );
    }
}
