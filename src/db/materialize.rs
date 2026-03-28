use anyhow::Result;

use crate::db::ProjectDb;
use crate::models::Scope;
use crate::reference::{parse_reference, Reference, ScopeLevel, TagFilter};

/// Context for checking whether a file matches a reference.
/// Built from data already available during a filesystem walk.
pub struct FileContext<'a> {
    pub rel_path: &'a str,
    pub sha256: &'a str,
    pub matching_categories: &'a [Scope],
    pub tags: &'a [String],
}

/// Check if a file matches a parsed reference based on its known metadata.
/// No filesystem walk or hashing — uses pre-computed category matches and tags.
fn file_matches_reference(file: &FileContext<'_>, reference: &Reference) -> bool {
    match reference {
        Reference::BarePath(_) => false,
        Reference::Workspace { scope, tags, glob } | Reference::Context { scope, tags, glob } => {
            matches_scope(file, scope)
                && matches_tags(file, tags)
                && matches_glob(file, glob.as_ref())
        }
    }
}

fn matches_scope(file: &FileContext<'_>, scope: &[ScopeLevel]) -> bool {
    if scope.is_empty() {
        return true;
    }
    // First scope level is category name (in project context)
    let level = &scope[0];
    level
        .names
        .iter()
        .any(|name| file.matching_categories.iter().any(|cat| cat.name == *name))
}

fn matches_tags(file: &FileContext<'_>, tag_filters: &[TagFilter]) -> bool {
    if tag_filters.is_empty() {
        return true;
    }
    for filter in tag_filters {
        if filter.tags.is_empty() {
            continue;
        }
        let group_matches = filter.tags.iter().any(|t| file.tags.contains(t));
        if !group_matches {
            return false;
        }
    }
    true
}

fn matches_glob(file: &FileContext<'_>, glob: Option<&String>) -> bool {
    let Some(pattern) = glob else {
        return true;
    };
    let Ok(compiled) = glob::Pattern::new(pattern) else {
        return false;
    };
    let file_name = file.rel_path.rsplit('/').next().unwrap_or(file.rel_path);
    compiled.matches(file_name) || compiled.matches(file.rel_path)
}

/// Materialize all pipeline subscriptions for a single file.
/// Call this after ingest or tag changes when the file's metadata is known.
pub fn materialize_pipelines_for_file(db: &ProjectDb, file: &FileContext<'_>) -> Result<()> {
    let subscriptions = db.list_all_pipeline_subscriptions()?;
    for (pipeline_id, sub) in &subscriptions {
        let Ok(reference) = parse_reference(&sub.reference) else {
            continue;
        };
        if file_matches_reference(file, &reference) {
            let sub_id = sub.id.unwrap_or(0);
            db.materialize_pipeline_file(*pipeline_id, file.sha256, sub_id)?;
        }
    }
    Ok(())
}

/// Full rematerialization: resolve all subscriptions against all known files.
/// Requires a filesystem walk to discover paths. Call from `mkrk ingest` or
/// `mkrk pipeline rematerialize`.
pub fn rematerialize_all_pipelines(db: &ProjectDb, project_root: &std::path::Path) -> Result<u32> {
    let subscriptions = db.list_all_pipeline_subscriptions()?;
    if subscriptions.is_empty() {
        return Ok(0);
    }

    let categories = db.list_categories()?;
    let patterns = crate::walk::category_patterns(db, None)?;
    let entries = crate::walk::walk_and_collect(project_root, &patterns)?;

    let mut count = 0u32;
    for rel_path in &entries {
        let abs_path = project_root.join(rel_path);
        let (sha256, _fingerprint) = crate::integrity::hash_and_fingerprint(&abs_path)?;

        // Only materialize for files we track
        if db.get_file_by_hash(&sha256)?.is_none() {
            continue;
        }

        let matching_cats: Vec<Scope> = categories
            .iter()
            .filter(|cat| cat.matches(rel_path).unwrap_or(false))
            .cloned()
            .collect();

        let file_id = db
            .get_file_by_hash(&sha256)?
            .and_then(|f| f.id)
            .unwrap_or(0);
        let tags = if file_id > 0 {
            db.get_tags(file_id)?
        } else {
            vec![]
        };

        let file_ctx = FileContext {
            rel_path,
            sha256: &sha256,
            matching_categories: &matching_cats,
            tags: &tags,
        };

        for (pipeline_id, sub) in &subscriptions {
            let Ok(reference) = parse_reference(&sub.reference) else {
                continue;
            };
            if file_matches_reference(&file_ctx, &reference) {
                let sub_id = sub.id.unwrap_or(0);
                db.materialize_pipeline_file(*pipeline_id, &sha256, sub_id)?;
                count += 1;
            }
        }
    }

    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{scope::CategoryType, ScopeType};

    fn make_cat(name: &str, pattern: &str) -> Scope {
        Scope {
            id: None,
            name: name.to_string(),
            scope_type: ScopeType::Category,
            pattern: Some(pattern.to_string()),
            category_type: Some(CategoryType::Files),
            description: None,
            created_at: None,
        }
    }

    #[test]
    fn matches_category_reference() {
        let cats = vec![make_cat("evidence", "evidence/**")];
        let file = FileContext {
            rel_path: "evidence/doc.pdf",
            sha256: "abc",
            matching_categories: &cats,
            tags: &[],
        };

        let r = parse_reference(":evidence").unwrap();
        assert!(file_matches_reference(&file, &r));

        let r = parse_reference(":notes").unwrap();
        assert!(!file_matches_reference(&file, &r));
    }

    #[test]
    fn matches_tag_filter() {
        let cats = vec![make_cat("evidence", "evidence/**")];
        let tags = vec!["classified".to_string()];
        let file = FileContext {
            rel_path: "evidence/doc.pdf",
            sha256: "abc",
            matching_categories: &cats,
            tags: &tags,
        };

        let r = parse_reference(":evidence!classified").unwrap();
        assert!(file_matches_reference(&file, &r));

        let r = parse_reference(":evidence!secret").unwrap();
        assert!(!file_matches_reference(&file, &r));
    }

    #[test]
    fn matches_glob_filter() {
        let cats = vec![make_cat("evidence", "evidence/**")];
        let file = FileContext {
            rel_path: "evidence/report.pdf",
            sha256: "abc",
            matching_categories: &cats,
            tags: &[],
        };

        let r = parse_reference("evidence/*.pdf").unwrap();
        assert!(file_matches_reference(&file, &r));

        let r = parse_reference("evidence/*.txt").unwrap();
        assert!(!file_matches_reference(&file, &r));
    }

    #[test]
    fn empty_scope_matches_everything() {
        let cats = vec![make_cat("evidence", "evidence/**")];
        let file = FileContext {
            rel_path: "evidence/doc.pdf",
            sha256: "abc",
            matching_categories: &cats,
            tags: &[],
        };

        let r = parse_reference(":").unwrap();
        assert!(file_matches_reference(&file, &r));
    }

    #[test]
    fn tag_and_or_logic() {
        let cats = vec![make_cat("evidence", "evidence/**")];
        let tags = vec!["classified".to_string(), "priority".to_string()];
        let file = FileContext {
            rel_path: "evidence/doc.pdf",
            sha256: "abc",
            matching_categories: &cats,
            tags: &tags,
        };

        // AND: classified AND priority
        let r = parse_reference(":evidence!classified!priority").unwrap();
        assert!(file_matches_reference(&file, &r));

        // OR: classified OR secret
        let r = parse_reference(":evidence!classified,secret").unwrap();
        assert!(file_matches_reference(&file, &r));

        // AND fails: classified AND secret (no secret tag)
        let r = parse_reference(":evidence!classified!secret").unwrap();
        assert!(!file_matches_reference(&file, &r));
    }
}
