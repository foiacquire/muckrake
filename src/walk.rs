use std::path::Path;

use anyhow::Result;

use crate::db::ProjectDb;
use crate::models::Category;

/// Recursively walk `root`, skipping dot-prefixed entries, and collect relative
/// paths whose string form matches at least one of `patterns`.
///
/// Returns a sorted `Vec` of relative path strings suitable for further
/// filtering or DB lookups. No database operations happen here -- callers
/// decide what to do with each matched path.
pub fn walk_and_collect(root: &Path, patterns: &[glob::Pattern]) -> Result<Vec<String>> {
    let mut entries = Vec::new();
    walk_recursive(root, root, patterns, &mut entries)?;
    entries.sort();
    Ok(entries)
}

fn walk_recursive(
    root: &Path,
    dir: &Path,
    patterns: &[glob::Pattern],
    entries: &mut Vec<String>,
) -> Result<()> {
    let read_dir = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    };

    for entry in read_dir {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }

        let path = entry.path();
        if path.is_dir() {
            walk_recursive(root, &path, patterns, entries)?;
        } else if path.is_file() {
            let rel_path = path.strip_prefix(root)?.to_string_lossy().to_string();
            if patterns.iter().any(|p| p.matches(&rel_path)) {
                entries.push(rel_path);
            }
        }
    }
    Ok(())
}

/// Build glob patterns that match files belonging to a given category.
///
/// - `None` -> match everything (`**`)
/// - `Some(name)` with a matching category in DB -> patterns from the category's
///   base directory
/// - `Some(name)` with no match -> treat as a subcategory path prefix
pub fn category_patterns(
    db: &ProjectDb,
    category_name: Option<&str>,
) -> Result<Vec<glob::Pattern>> {
    let Some(name) = category_name else {
        return Ok(vec![glob::Pattern::new("**")?]);
    };

    if let Some(cat) = db.get_category_by_name(name)? {
        let base = Category::name_from_pattern(&cat.pattern);
        return Ok(vec![
            glob::Pattern::new(&format!("{base}/*"))?,
            glob::Pattern::new(&format!("{base}/**/*"))?,
        ]);
    }

    // Subcategory path (e.g. evidence/emails) -- treat as path prefix
    Ok(vec![
        glob::Pattern::new(&format!("{name}/*"))?,
        glob::Pattern::new(&format!("{name}/**/*"))?,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_tree(tmp: &Path) {
        std::fs::create_dir_all(tmp.join("alpha/sub")).unwrap();
        std::fs::create_dir_all(tmp.join(".hidden")).unwrap();
        std::fs::write(tmp.join("alpha/one.txt"), "1").unwrap();
        std::fs::write(tmp.join("alpha/sub/two.txt"), "2").unwrap();
        std::fs::write(tmp.join(".hidden/secret.txt"), "s").unwrap();
        std::fs::write(tmp.join(".dotfile"), "d").unwrap();
        std::fs::write(tmp.join("root.txt"), "r").unwrap();
    }

    #[test]
    fn walk_skips_dotfiles_and_dot_dirs() {
        let tmp = TempDir::new().unwrap();
        create_tree(tmp.path());

        let patterns = vec![glob::Pattern::new("**").unwrap()];
        let results = walk_and_collect(tmp.path(), &patterns).unwrap();

        assert!(!results.iter().any(|p| p.contains(".hidden")));
        assert!(!results.iter().any(|p| p.starts_with('.')));
        assert!(results.contains(&"root.txt".to_string()));
        assert!(results.contains(&"alpha/one.txt".to_string()));
        assert!(results.contains(&"alpha/sub/two.txt".to_string()));
    }

    #[test]
    fn walk_filters_by_pattern() {
        let tmp = TempDir::new().unwrap();
        create_tree(tmp.path());

        let patterns = vec![
            glob::Pattern::new("alpha/*").unwrap(),
            glob::Pattern::new("alpha/**/*").unwrap(),
        ];
        let results = walk_and_collect(tmp.path(), &patterns).unwrap();

        assert!(results.contains(&"alpha/one.txt".to_string()));
        assert!(results.contains(&"alpha/sub/two.txt".to_string()));
        assert!(!results.contains(&"root.txt".to_string()));
    }

    #[test]
    fn walk_returns_sorted() {
        let tmp = TempDir::new().unwrap();
        create_tree(tmp.path());

        let patterns = vec![glob::Pattern::new("**").unwrap()];
        let results = walk_and_collect(tmp.path(), &patterns).unwrap();

        let mut sorted = results.clone();
        sorted.sort();
        assert_eq!(results, sorted);
    }

    #[test]
    fn walk_handles_missing_dir() {
        let tmp = TempDir::new().unwrap();
        let missing = tmp.path().join("nonexistent");

        let patterns = vec![glob::Pattern::new("**").unwrap()];
        let results = walk_and_collect(&missing, &patterns).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn walk_empty_patterns_matches_nothing() {
        let tmp = TempDir::new().unwrap();
        create_tree(tmp.path());

        let results = walk_and_collect(tmp.path(), &[]).unwrap();
        assert!(results.is_empty());
    }
}
