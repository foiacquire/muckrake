use crate::db::ProjectDb;
use crate::models::Category;

/// Format a file path as a valid reference string relative to the current context.
///
/// Looks up the matching category to map the filesystem path to a
/// category-relative reference. Subdirectories within the category become
/// `.`-separated scope levels; only the filename follows `/`.
///
/// For example, `sources/web/2026-01-01/robots.txt` with category "sources"
/// (pattern `sources/**`) becomes `sources.web.2026-01-01/robots.txt`.
///
/// From workspace context, the project name is prepended:
/// `:anthropic.sources.web.2026-01-01/robots.txt`.
pub fn format_ref(path: &str, project_name: Option<&str>, db: &ProjectDb) -> String {
    let category = db.match_category(path).ok().flatten();

    match category {
        Some(cat) => {
            let base = Category::name_from_pattern(&cat.pattern);
            let relative = path
                .strip_prefix(&base)
                .and_then(|s| s.strip_prefix('/'))
                .unwrap_or(path);

            let (dir_prefix, filename) = match relative.rfind('/') {
                Some(pos) => {
                    let dotted = relative[..pos].replace('/', ".");
                    (format!(".{dotted}"), &relative[pos + 1..])
                }
                None => (String::new(), relative),
            };
            let sep = if filename.contains('.') { "/" } else { "." };

            match project_name {
                Some(project) => {
                    format!(":{project}.{}{dir_prefix}{sep}{filename}", cat.name)
                }
                None => format!("{}{dir_prefix}{sep}{filename}", cat.name),
            }
        }
        None => match project_name {
            Some(project) => format!(":{project}/{path}"),
            None => path.to_string(),
        },
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
}
