use std::collections::HashSet;

use anyhow::{bail, Result};

use crate::context::Context;
use crate::db::{ProjectDb, WorkspaceDb};
use crate::models::TrackedFile;

use super::types::{Reference, ScopeLevel, TagFilter};

type ScopeExpansion<'a> = Vec<(Option<String>, Option<String>, OpenedDb<'a>)>;

#[derive(Debug, Clone)]
pub struct ResolvedFile {
    pub project_name: Option<String>,
    pub file: TrackedFile,
}

#[derive(Debug)]
pub struct ResolvedCollection {
    pub files: Vec<ResolvedFile>,
}

impl ResolvedCollection {
    pub fn expect_one(self, reference_text: &str) -> Result<ResolvedFile> {
        match self.files.len() {
            0 => bail!("reference '{reference_text}' matched no files"),
            1 => Ok(self.files.into_iter().next().unwrap()),
            n => bail!("reference '{reference_text}' matched {n} files, expected 1"),
        }
    }
}

pub fn resolve_references(refs: &[Reference], ctx: &Context) -> Result<ResolvedCollection> {
    let mut all_files = Vec::new();
    let mut seen: HashSet<(Option<String>, i64)> = HashSet::new();

    for r in refs {
        let resolved = resolve_single(r, ctx)?;
        for rf in resolved {
            let key = (rf.project_name.clone(), rf.file.id.unwrap_or(-1));
            if seen.insert(key) {
                all_files.push(rf);
            }
        }
    }

    Ok(ResolvedCollection { files: all_files })
}

fn resolve_single(reference: &Reference, ctx: &Context) -> Result<Vec<ResolvedFile>> {
    match reference {
        Reference::BarePath(path) => resolve_bare_path(path, ctx),
        Reference::Structured { scope, tags, glob } => {
            resolve_structured(scope, tags, glob.as_deref(), ctx)
        }
    }
}

fn resolve_bare_path(path: &str, ctx: &Context) -> Result<Vec<ResolvedFile>> {
    let Context::Project { project_db, .. } = ctx else {
        bail!("bare path requires project context");
    };

    if let Some(file) = project_db.get_file_by_path(path)? {
        return Ok(vec![ResolvedFile {
            project_name: None,
            file,
        }]);
    }

    if let Some(file) = project_db.get_file_by_name(path)? {
        return Ok(vec![ResolvedFile {
            project_name: None,
            file,
        }]);
    }

    Ok(vec![])
}

fn resolve_structured(
    scope: &[ScopeLevel],
    tags: &[TagFilter],
    glob: Option<&str>,
    ctx: &Context,
) -> Result<Vec<ResolvedFile>> {
    let pairs = expand_scope(scope, ctx)?;
    let mut results = Vec::new();

    for (project_name, category_name, project_db) in &pairs {
        let path_prefix = category_name.as_ref().map(|c| format!("{c}/"));
        let tag_groups: Vec<Vec<&str>> = tags
            .iter()
            .map(|tf| tf.tags.iter().map(String::as_str).collect())
            .collect();
        let tag_group_refs: Vec<&[&str]> = tag_groups.iter().map(Vec::as_slice).collect();

        let files = project_db.list_files_filtered(path_prefix.as_deref(), &tag_group_refs)?;

        let glob_pattern = glob.map(glob::Pattern::new).transpose()?;

        for file in files {
            let matches_glob = match &glob_pattern {
                Some(pattern) => {
                    let file_name = file.path.rsplit('/').next().unwrap_or(&file.path);
                    pattern.matches(file_name) || pattern.matches(&file.path)
                }
                None => true,
            };

            if matches_glob {
                results.push(ResolvedFile {
                    project_name: project_name.clone(),
                    file,
                });
            }
        }
    }

    Ok(results)
}

enum OpenedDb<'a> {
    Borrowed(&'a ProjectDb),
    Owned(ProjectDb),
}

impl std::ops::Deref for OpenedDb<'_> {
    type Target = ProjectDb;
    fn deref(&self) -> &ProjectDb {
        match self {
            Self::Borrowed(db) => db,
            Self::Owned(db) => db,
        }
    }
}

fn expand_scope<'a>(scope: &[ScopeLevel], ctx: &'a Context) -> Result<ScopeExpansion<'a>> {
    match scope.len() {
        0 => expand_zero_scope(ctx),
        1 => expand_one_scope(scope, ctx),
        _ => expand_multi_scope(scope, ctx),
    }
}

fn expand_zero_scope(ctx: &Context) -> Result<ScopeExpansion<'_>> {
    match ctx {
        Context::Project { project_db, .. } => {
            Ok(vec![(None, None, OpenedDb::Borrowed(project_db))])
        }
        Context::Workspace {
            workspace_db,
            workspace_root,
            ..
        } => expand_all_workspace_projects(workspace_db, workspace_root),
        Context::None => bail!("not in a muckrake project or workspace"),
    }
}

fn expand_one_scope<'a>(scope: &[ScopeLevel], ctx: &'a Context) -> Result<ScopeExpansion<'a>> {
    let level = &scope[0];

    match ctx {
        Context::Project {
            project_db,
            workspace,
            ..
        } => {
            let mut results = Vec::new();
            for name in &level.names {
                if is_category_in_project(project_db, name)? {
                    results.push((None, Some(name.clone()), OpenedDb::Borrowed(project_db)));
                } else if let Some(ws) = workspace {
                    let db = open_project_db(&ws.workspace_db, &ws.workspace_root, name)?;
                    results.push((Some(name.clone()), None, OpenedDb::Owned(db)));
                } else {
                    results.push((None, Some(name.clone()), OpenedDb::Borrowed(project_db)));
                }
            }
            Ok(results)
        }
        Context::Workspace {
            workspace_db,
            workspace_root,
            ..
        } => {
            let mut results = Vec::new();
            for name in &level.names {
                let db = open_project_db(workspace_db, workspace_root, name)?;
                results.push((Some(name.clone()), None, OpenedDb::Owned(db)));
            }
            Ok(results)
        }
        Context::None => bail!("not in a muckrake project or workspace"),
    }
}

fn expand_workspace_project_categories(
    scope: &[ScopeLevel],
    project_name: &str,
    workspace_db: &WorkspaceDb,
    workspace_root: &std::path::Path,
) -> Result<ScopeExpansion<'static>> {
    let category_paths = build_subcategory_path(scope, 1);
    if category_paths.is_empty() {
        let db = open_project_db(workspace_db, workspace_root, project_name)?;
        Ok(vec![(
            Some(project_name.to_string()),
            None,
            OpenedDb::Owned(db),
        )])
    } else {
        let mut results = Vec::new();
        for path in category_paths {
            let db = open_project_db(workspace_db, workspace_root, project_name)?;
            results.push((
                Some(project_name.to_string()),
                Some(path),
                OpenedDb::Owned(db),
            ));
        }
        Ok(results)
    }
}

fn expand_multi_scope<'a>(scope: &[ScopeLevel], ctx: &'a Context) -> Result<ScopeExpansion<'a>> {
    match ctx {
        Context::Project {
            project_db,
            workspace,
            ..
        } => {
            let mut results = Vec::new();
            for name in &scope[0].names {
                if is_category_in_project(project_db, name)? {
                    for path in build_subcategory_path(scope, 0) {
                        results.push((None, Some(path), OpenedDb::Borrowed(project_db)));
                    }
                } else if let Some(ws) = workspace {
                    results.extend(expand_workspace_project_categories(
                        scope,
                        name,
                        &ws.workspace_db,
                        &ws.workspace_root,
                    )?);
                } else {
                    bail!("cross-project reference requires workspace context");
                }
            }
            Ok(results)
        }
        Context::Workspace {
            workspace_db,
            workspace_root,
            ..
        } => {
            let mut results = Vec::new();
            for project_name in &scope[0].names {
                results.extend(expand_workspace_project_categories(
                    scope,
                    project_name,
                    workspace_db,
                    workspace_root,
                )?);
            }
            Ok(results)
        }
        Context::None => bail!("not in a muckrake project or workspace"),
    }
}

fn build_subcategory_path(scope: &[ScopeLevel], start: usize) -> Vec<String> {
    if start >= scope.len() {
        return vec![];
    }

    let mut paths: Vec<String> = scope[start].names.clone();

    for level in &scope[start + 1..] {
        let mut expanded = Vec::new();
        for prefix in &paths {
            for name in &level.names {
                expanded.push(format!("{prefix}/{name}"));
            }
        }
        paths = expanded;
    }

    paths
}

fn is_category_in_project(project_db: &ProjectDb, name: &str) -> Result<bool> {
    let categories = project_db.list_categories()?;
    let pattern_prefix = format!("{name}/");
    let pattern_glob = format!("{name}/**");
    Ok(categories
        .iter()
        .any(|c| c.pattern == pattern_glob || c.pattern.starts_with(&pattern_prefix)))
}

fn open_project_db(
    workspace_db: &WorkspaceDb,
    workspace_root: &std::path::Path,
    project_name: &str,
) -> Result<ProjectDb> {
    let project = workspace_db
        .get_project_by_name(project_name)?
        .ok_or_else(|| anyhow::anyhow!("project '{project_name}' not found in workspace"))?;
    let proj_root = workspace_root.join(&project.path);
    let mkrk = proj_root.join(".mkrk");
    ProjectDb::open(&mkrk)
}

fn expand_all_workspace_projects(
    workspace_db: &WorkspaceDb,
    workspace_root: &std::path::Path,
) -> Result<ScopeExpansion<'static>> {
    let projects = workspace_db.list_projects()?;
    let mut results = Vec::new();
    for proj in projects {
        let proj_root = workspace_root.join(&proj.path);
        let mkrk = proj_root.join(".mkrk");
        if mkrk.exists() {
            let db = ProjectDb::open(&mkrk)?;
            results.push((Some(proj.name), None, OpenedDb::Owned(db)));
        }
    }
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::WorkspaceContext;
    use crate::models::TrackedFile;
    use crate::reference::parse::parse_reference;
    use tempfile::TempDir;

    fn make_file(name: &str, path: &str) -> TrackedFile {
        TrackedFile {
            id: None,
            name: name.to_string(),
            path: path.to_string(),
            sha256: Some("abc123".to_string()),
            mime_type: None,
            size: Some(100),
            ingested_at: "2025-01-01T00:00:00Z".to_string(),
            provenance: None,
            immutable: false,
        }
    }

    fn setup_project(dir: &std::path::Path) -> ProjectDb {
        let db = ProjectDb::create(&dir.join(".mkrk")).unwrap();
        let cat = crate::models::Category {
            id: None,
            pattern: "evidence/**".to_string(),
            category_type: crate::models::CategoryType::Files,
            description: None,
        };
        db.insert_category(&cat).unwrap();
        let cat2 = crate::models::Category {
            id: None,
            pattern: "notes/**".to_string(),
            category_type: crate::models::CategoryType::Files,
            description: None,
        };
        db.insert_category(&cat2).unwrap();
        db
    }

    fn make_project_ctx(dir: &std::path::Path) -> Context {
        Context::Project {
            project_root: dir.to_path_buf(),
            project_db: ProjectDb::open(&dir.join(".mkrk")).unwrap(),
            workspace: None,
        }
    }

    fn resolve_one(reference: &str, ctx: &Context) -> ResolvedCollection {
        let refs = vec![parse_reference(reference).unwrap()];
        resolve_references(&refs, ctx).unwrap()
    }

    struct WorkspaceSetup {
        ws_dir: TempDir,
        ws_db: WorkspaceDb,
    }

    fn setup_workspace() -> WorkspaceSetup {
        let ws_dir = TempDir::new().unwrap();
        let ws_db = WorkspaceDb::create(&ws_dir.path().join(".mksp")).unwrap();
        ws_db.set_config("projects_dir", "projects").unwrap();
        WorkspaceSetup { ws_dir, ws_db }
    }

    fn add_workspace_project(ws: &WorkspaceSetup, name: &str) -> (std::path::PathBuf, ProjectDb) {
        let proj_dir = ws.ws_dir.path().join("projects").join(name);
        std::fs::create_dir_all(&proj_dir).unwrap();
        let db = ProjectDb::create(&proj_dir.join(".mkrk")).unwrap();
        db.insert_category(&crate::models::Category {
            id: None,
            pattern: "evidence/**".to_string(),
            category_type: crate::models::CategoryType::Files,
            description: None,
        })
        .unwrap();
        ws.ws_db
            .register_project(name, &format!("projects/{name}"), None)
            .unwrap();
        (proj_dir, db)
    }

    fn make_ws_project_ctx(ws: &WorkspaceSetup, proj_dir: &std::path::Path) -> Context {
        Context::Project {
            project_root: proj_dir.to_path_buf(),
            project_db: ProjectDb::open(&proj_dir.join(".mkrk")).unwrap(),
            workspace: Some(WorkspaceContext {
                workspace_root: ws.ws_dir.path().to_path_buf(),
                workspace_db: WorkspaceDb::open(&ws.ws_dir.path().join(".mksp")).unwrap(),
            }),
        }
    }

    #[test]
    fn resolve_bare_path_by_path() {
        let dir = TempDir::new().unwrap();
        let db = setup_project(dir.path());
        db.insert_file(&make_file("report.pdf", "evidence/report.pdf"))
            .unwrap();

        let ctx = make_project_ctx(dir.path());
        let coll = resolve_one("evidence/report.pdf", &ctx);
        assert_eq!(coll.files.len(), 1);
        assert_eq!(coll.files[0].file.name, "report.pdf");
    }

    #[test]
    fn resolve_bare_path_by_name() {
        let dir = TempDir::new().unwrap();
        let db = setup_project(dir.path());
        db.insert_file(&make_file("report.pdf", "evidence/report.pdf"))
            .unwrap();

        let ctx = make_project_ctx(dir.path());
        let coll = resolve_one("report.pdf", &ctx);
        assert_eq!(coll.files.len(), 1);
        assert_eq!(coll.files[0].file.path, "evidence/report.pdf");
    }

    #[test]
    fn resolve_category_scope() {
        let dir = TempDir::new().unwrap();
        let db = setup_project(dir.path());
        db.insert_file(&make_file("report.pdf", "evidence/report.pdf"))
            .unwrap();
        db.insert_file(&make_file("todo.md", "notes/todo.md"))
            .unwrap();

        let ctx = make_project_ctx(dir.path());
        let coll = resolve_one(":evidence", &ctx);
        assert_eq!(coll.files.len(), 1);
        assert_eq!(coll.files[0].file.name, "report.pdf");
    }

    #[test]
    fn resolve_tag_filter() {
        let dir = TempDir::new().unwrap();
        let db = setup_project(dir.path());
        let id1 = db
            .insert_file(&make_file("report.pdf", "evidence/report.pdf"))
            .unwrap();
        db.insert_file(&make_file("memo.pdf", "evidence/memo.pdf"))
            .unwrap();
        db.insert_tag(id1, "classified", "testhash").unwrap();

        let ctx = make_project_ctx(dir.path());
        let coll = resolve_one(":evidence!classified", &ctx);
        assert_eq!(coll.files.len(), 1);
        assert_eq!(coll.files[0].file.name, "report.pdf");
    }

    #[test]
    fn resolve_tag_and_or() {
        let dir = TempDir::new().unwrap();
        let db = setup_project(dir.path());
        let id1 = db
            .insert_file(&make_file("a.pdf", "evidence/a.pdf"))
            .unwrap();
        let id2 = db
            .insert_file(&make_file("b.pdf", "evidence/b.pdf"))
            .unwrap();
        let id3 = db
            .insert_file(&make_file("c.pdf", "evidence/c.pdf"))
            .unwrap();
        db.insert_tag(id1, "classified", "testhash").unwrap();
        db.insert_tag(id1, "priority", "testhash").unwrap();
        db.insert_tag(id2, "classified", "testhash").unwrap();
        db.insert_tag(id3, "priority", "testhash").unwrap();

        let ctx = make_project_ctx(dir.path());

        // classified AND priority -> only a.pdf
        let coll = resolve_one(":evidence!classified!priority", &ctx);
        assert_eq!(coll.files.len(), 1);
        assert_eq!(coll.files[0].file.name, "a.pdf");

        // classified OR priority -> a.pdf, b.pdf, c.pdf
        let coll = resolve_one(":evidence!classified,priority", &ctx);
        assert_eq!(coll.files.len(), 3);
    }

    #[test]
    fn resolve_glob_filter() {
        let dir = TempDir::new().unwrap();
        let db = setup_project(dir.path());
        db.insert_file(&make_file("report.pdf", "evidence/report.pdf"))
            .unwrap();
        db.insert_file(&make_file("photo.jpg", "evidence/photo.jpg"))
            .unwrap();

        let ctx = make_project_ctx(dir.path());
        let coll = resolve_one(":evidence/*.pdf", &ctx);
        assert_eq!(coll.files.len(), 1);
        assert_eq!(coll.files[0].file.name, "report.pdf");
    }

    #[test]
    fn resolve_cross_project() {
        let ws = setup_workspace();
        let (proj1_dir, db1) = add_workspace_project(&ws, "bailey");
        db1.insert_file(&make_file("b-report.pdf", "evidence/b-report.pdf"))
            .unwrap();
        let (_, db2) = add_workspace_project(&ws, "george");
        db2.insert_file(&make_file("g-report.pdf", "evidence/g-report.pdf"))
            .unwrap();

        let ctx = make_ws_project_ctx(&ws, &proj1_dir);
        let coll = resolve_one(":{bailey,george}.evidence", &ctx);
        assert_eq!(coll.files.len(), 2);

        let names: Vec<&str> = coll.files.iter().map(|f| f.file.name.as_str()).collect();
        assert!(names.contains(&"b-report.pdf"));
        assert!(names.contains(&"g-report.pdf"));
    }

    #[test]
    fn resolve_no_match() {
        let dir = TempDir::new().unwrap();
        setup_project(dir.path());

        let ctx = make_project_ctx(dir.path());
        let coll = resolve_one(":evidence", &ctx);
        assert!(coll.files.is_empty());
    }

    #[test]
    fn resolve_subcategory() {
        let dir = TempDir::new().unwrap();
        let db = setup_project(dir.path());
        db.insert_file(&make_file("email1.eml", "evidence/emails/email1.eml"))
            .unwrap();
        db.insert_file(&make_file("photo.jpg", "evidence/photos/photo.jpg"))
            .unwrap();

        let ctx = make_project_ctx(dir.path());
        let coll = resolve_one(":evidence.emails", &ctx);
        assert_eq!(coll.files.len(), 1);
        assert_eq!(coll.files[0].file.name, "email1.eml");
    }

    #[test]
    fn resolve_subcategory_not_category() {
        let ws = setup_workspace();
        let (proj_dir, db) = add_workspace_project(&ws, "bailey");
        db.insert_file(&make_file("doc.pdf", "evidence/doc.pdf"))
            .unwrap();

        // Inside a project where "bailey" is NOT a category
        let ctx = make_ws_project_ctx(&ws, &proj_dir);
        // :bailey.evidence → falls back to project.category
        let coll = resolve_one(":bailey.evidence", &ctx);
        assert_eq!(coll.files.len(), 1);
        assert_eq!(coll.files[0].file.name, "doc.pdf");
        assert_eq!(coll.files[0].project_name.as_deref(), Some("bailey"));
    }

    #[test]
    fn resolve_three_levels() {
        let ws = setup_workspace();
        let (_, db) = add_workspace_project(&ws, "bailey");
        db.insert_file(&make_file("email.eml", "evidence/emails/email.eml"))
            .unwrap();
        db.insert_file(&make_file("photo.jpg", "evidence/photos/photo.jpg"))
            .unwrap();

        let ctx = Context::Workspace {
            workspace_root: ws.ws_dir.path().to_path_buf(),
            workspace_db: WorkspaceDb::open(&ws.ws_dir.path().join(".mksp")).unwrap(),
        };

        // :bailey.evidence.emails → project.category.subcategory
        let coll = resolve_one(":bailey.evidence.emails", &ctx);
        assert_eq!(coll.files.len(), 1);
        assert_eq!(coll.files[0].file.name, "email.eml");
    }

    #[test]
    fn resolve_subcategory_brace_expansion() {
        let dir = TempDir::new().unwrap();
        let db = setup_project(dir.path());
        db.insert_file(&make_file("email.eml", "evidence/emails/email.eml"))
            .unwrap();
        db.insert_file(&make_file("memo.md", "notes/drafts/memo.md"))
            .unwrap();
        db.insert_file(&make_file("photo.jpg", "evidence/photos/photo.jpg"))
            .unwrap();

        let ctx = make_project_ctx(dir.path());
        // :{evidence,notes}.drafts → evidence/drafts/ and notes/drafts/
        let coll = resolve_one(":{evidence,notes}.drafts", &ctx);
        assert_eq!(coll.files.len(), 1);
        assert_eq!(coll.files[0].file.name, "memo.md");
    }
}
