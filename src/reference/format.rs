use crate::db::ProjectDb;
use crate::models::Category;

/// Format a file path as a canonical reference string.
///
/// Converts a filesystem-relative path into the reference syntax defined by the
/// spec. Scope levels use `.` separators; `/` precedes the filename only when
/// the filename contains `.` (to avoid ambiguity with scope levels).
///
/// With workspace context (project name provided), the reference is prefixed
/// with `:projectname.`. Without workspace context, scope starts directly.
///
/// Examples (project `acme`, category `sources` pattern `sources/**`):
///   `sources/web/data/robots.txt` → `:acme.sources.web.data/robots.txt`
///   `sources/Makefile`            → `:acme.sources.Makefile`
///   `README.md` (uncategorized)   → `:acme/README.md`
pub fn format_ref(path: &str, project_name: Option<&str>, db: &ProjectDb) -> String {
    let category = db.match_category(path).ok().flatten();

    if let Some(ref cat) = category {
        let base = Category::name_from_pattern(&cat.pattern);
        let relative = path
            .strip_prefix(&base)
            .and_then(|s| s.strip_prefix('/'))
            .unwrap_or(path);
        let body = format_scoped(&cat.name, relative);
        match project_name {
            Some(project) => format!(":{project}.{body}"),
            None => body,
        }
    } else {
        let (dir_prefix, filename) = split_dirs_and_filename(path);
        let sep = filename_separator(filename);
        match (project_name, dir_prefix) {
            (Some(project), Some(dotted)) => {
                format!(":{project}.{dotted}{sep}{filename}")
            }
            (Some(project), None) => format!(":{project}{sep}{filename}"),
            (None, Some(dotted)) => format!("{dotted}{sep}{filename}"),
            (None, None) => path.to_string(),
        }
    }
}

/// Format a path within a known category scope.
/// `category` is the scope name, `relative` is the path within that category.
fn format_scoped(category: &str, relative: &str) -> String {
    let (dir_prefix, filename) = split_dirs_and_filename(relative);
    let sep = filename_separator(filename);

    match dir_prefix {
        Some(dotted) => format!("{category}.{dotted}{sep}{filename}"),
        None => format!("{category}{sep}{filename}"),
    }
}

/// Split a relative path into dotted directory prefix and filename.
fn split_dirs_and_filename(rel_path: &str) -> (Option<String>, &str) {
    match rel_path.rfind('/') {
        Some(pos) => {
            let dotted = rel_path[..pos].replace('/', ".");
            (Some(dotted), &rel_path[pos + 1..])
        }
        None => (None, rel_path),
    }
}

/// Choose `/` or `.` before the filename based on whether it contains `.`.
fn filename_separator(filename: &str) -> &'static str {
    if filename.contains('.') {
        "/"
    } else {
        "."
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_db(categories: &[(&str, &str)]) -> (TempDir, ProjectDb) {
        let dir = TempDir::new().unwrap();
        let db = ProjectDb::create(&dir.path().join(".mkrk")).unwrap();
        for (name, pattern) in categories {
            db.insert_category(&crate::models::Category {
                id: None,
                name: (*name).to_string(),
                pattern: (*pattern).to_string(),
                category_type: crate::models::CategoryType::Files,
                description: None,
            })
            .unwrap();
        }
        (dir, db)
    }

    #[test]
    fn simple_category() {
        let (_dir, db) = setup_db(&[("evidence", "evidence/**")]);
        assert_eq!(
            format_ref("evidence/report.pdf", None, &db),
            "evidence/report.pdf"
        );
    }

    #[test]
    fn nested_pattern() {
        let (_dir, db) = setup_db(&[("evidence", "sources/evidence/**")]);
        assert_eq!(
            format_ref("sources/evidence/report.pdf", None, &db),
            "evidence/report.pdf"
        );
    }

    #[test]
    fn nested_pattern_workspace() {
        let (_dir, db) = setup_db(&[("evidence", "sources/evidence/**")]);
        assert_eq!(
            format_ref("sources/evidence/report.pdf", Some("myproject"), &db),
            ":myproject.evidence/report.pdf"
        );
    }

    #[test]
    fn subdirectory_in_category() {
        let (_dir, db) = setup_db(&[("evidence", "sources/evidence/**")]);
        assert_eq!(
            format_ref("sources/evidence/emails/msg.eml", None, &db),
            "evidence.emails/msg.eml"
        );
    }

    #[test]
    fn deep_subdirectory_workspace() {
        let (_dir, db) = setup_db(&[("sources", "sources/**")]);
        assert_eq!(
            format_ref("sources/web/2026-01-01/robots.txt", Some("anthropic"), &db),
            ":anthropic.sources.web.2026-01-01/robots.txt"
        );
    }

    #[test]
    fn uncategorized_file() {
        let (_dir, db) = setup_db(&[("evidence", "evidence/**")]);
        assert_eq!(format_ref("readme.txt", None, &db), "readme.txt");
    }

    #[test]
    fn uncategorized_file_workspace() {
        let (_dir, db) = setup_db(&[("evidence", "evidence/**")]);
        assert_eq!(
            format_ref("readme.txt", Some("myproject"), &db),
            ":myproject/readme.txt"
        );
    }

    #[test]
    fn uncategorized_file_no_ext() {
        let (_dir, db) = setup_db(&[("evidence", "evidence/**")]);
        assert_eq!(
            format_ref("Makefile", Some("myproject"), &db),
            ":myproject.Makefile"
        );
    }

    #[test]
    fn uncategorized_subdir_workspace() {
        let (_dir, db) = setup_db(&[("evidence", "evidence/**")]);
        assert_eq!(
            format_ref("misc/notes/readme.txt", Some("myproject"), &db),
            ":myproject.misc.notes/readme.txt"
        );
    }

    #[test]
    fn workspace_simple_category() {
        let (_dir, db) = setup_db(&[("evidence", "evidence/**")]);
        assert_eq!(
            format_ref("evidence/report.pdf", Some("myproject"), &db),
            ":myproject.evidence/report.pdf"
        );
    }

    #[test]
    fn no_dot_in_filename_uses_dot_separator() {
        let (_dir, db) = setup_db(&[("tools", "tools/**")]);
        assert_eq!(format_ref("tools/Makefile", None, &db), "tools.Makefile");
    }

    #[test]
    fn no_dot_in_filename_workspace() {
        let (_dir, db) = setup_db(&[("tools", "tools/**")]);
        assert_eq!(
            format_ref("tools/Makefile", Some("myproject"), &db),
            ":myproject.tools.Makefile"
        );
    }

    #[test]
    fn no_dot_in_filename_with_subdirs() {
        let (_dir, db) = setup_db(&[("evidence", "evidence/**")]);
        assert_eq!(
            format_ref("evidence/emails/attachment", None, &db),
            "evidence.emails.attachment"
        );
    }

    // ---- Round-trip tests: format → parse → verify structure ----

    use crate::reference::parse::parse_reference;
    use crate::reference::types::{Reference, ScopeLevel};

    /// Verify that a formatted reference parses back to a structured reference
    /// with the expected scope levels and glob.
    fn assert_round_trip(
        path: &str,
        project_name: Option<&str>,
        db: &ProjectDb,
        expected_scope: &[&str],
        expected_glob: Option<&str>,
    ) {
        let formatted = format_ref(path, project_name, db);
        let parsed = parse_reference(&formatted)
            .unwrap_or_else(|e| panic!("failed to parse formatted ref '{formatted}': {e}"));

        let (scope, glob) = match parsed {
            Reference::Workspace { scope, glob, .. } => (scope, glob),
            Reference::Context { scope, glob, .. } => (scope, glob),
            Reference::BarePath(p) => {
                panic!("expected structured ref, got BarePath({p}) from '{formatted}'")
            }
        };

        let scope_names: Vec<&str> = scope
            .iter()
            .flat_map(|sl| sl.names.iter().map(String::as_str))
            .collect();
        assert_eq!(
            scope_names, expected_scope,
            "scope mismatch for '{formatted}'"
        );
        assert_eq!(
            glob.as_deref(),
            expected_glob,
            "glob mismatch for '{formatted}'"
        );
    }

    #[test]
    fn round_trip_categorized_project() {
        let (_dir, db) = setup_db(&[("evidence", "evidence/**")]);
        assert_round_trip(
            "evidence/report.pdf",
            None,
            &db,
            &["evidence"],
            Some("report.pdf"),
        );
    }

    #[test]
    fn round_trip_categorized_workspace() {
        let (_dir, db) = setup_db(&[("evidence", "evidence/**")]);
        assert_round_trip(
            "evidence/report.pdf",
            Some("acme"),
            &db,
            &["acme", "evidence"],
            Some("report.pdf"),
        );
    }

    #[test]
    fn round_trip_subdir_workspace() {
        let (_dir, db) = setup_db(&[("sources", "sources/**")]);
        assert_round_trip(
            "sources/web/2026-01-01/robots.txt",
            Some("acme"),
            &db,
            &["acme", "sources", "web", "2026-01-01"],
            Some("robots.txt"),
        );
    }

    #[test]
    fn round_trip_no_ext_filename() {
        let (_dir, db) = setup_db(&[("tools", "tools/**")]);
        assert_round_trip("tools/Makefile", None, &db, &["tools", "Makefile"], None);
    }

    #[test]
    fn round_trip_no_ext_workspace() {
        let (_dir, db) = setup_db(&[("tools", "tools/**")]);
        assert_round_trip(
            "tools/Makefile",
            Some("acme"),
            &db,
            &["acme", "tools", "Makefile"],
            None,
        );
    }

    #[test]
    fn round_trip_uncategorized_workspace() {
        let (_dir, db) = setup_db(&[("evidence", "evidence/**")]);
        assert_round_trip(
            "readme.txt",
            Some("acme"),
            &db,
            &["acme"],
            Some("readme.txt"),
        );
    }

    #[test]
    fn round_trip_uncategorized_no_ext_workspace() {
        let (_dir, db) = setup_db(&[("evidence", "evidence/**")]);
        assert_round_trip("Makefile", Some("acme"), &db, &["acme", "Makefile"], None);
    }

    #[test]
    fn round_trip_uncategorized_subdir_workspace() {
        let (_dir, db) = setup_db(&[("evidence", "evidence/**")]);
        assert_round_trip(
            "misc/notes/readme.txt",
            Some("acme"),
            &db,
            &["acme", "misc", "notes"],
            Some("readme.txt"),
        );
    }

    #[test]
    fn round_trip_deep_subdir_no_ext() {
        let (_dir, db) = setup_db(&[("evidence", "evidence/**")]);
        assert_round_trip(
            "evidence/emails/attachment",
            None,
            &db,
            &["evidence", "emails", "attachment"],
            None,
        );
    }

    #[test]
    fn round_trip_nested_pattern() {
        let (_dir, db) = setup_db(&[("evidence", "sources/evidence/**")]);
        assert_round_trip(
            "sources/evidence/emails/msg.eml",
            Some("acme"),
            &db,
            &["acme", "evidence", "emails"],
            Some("msg.eml"),
        );
    }
}
