use std::collections::HashSet;
use std::path::PathBuf;

use anyhow::{bail, Result};

use crate::context::Context;
use crate::db::{ProjectDb, ProjectRow, WorkspaceDb};
use crate::integrity;
use crate::models::TrackedFile;
use crate::walk;

use super::parse::parse_reference;
use super::types::{Reference, ScopeLevel, TagFilter};

type ScopeExpansion<'a> = Vec<(Option<String>, Option<String>, OpenedDb<'a>)>;

#[derive(Debug, Clone)]
pub struct ResolvedFile {
    pub project_name: Option<String>,
    /// Filesystem-derived relative path (ephemeral, not from DB).
    pub rel_path: String,
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
    let mut next_synthetic_id: i64 = -1;

    for r in refs {
        let resolved = resolve_single(r, ctx)?;
        for rf in resolved {
            let dedup_id = rf.file.id.unwrap_or_else(|| {
                let id = next_synthetic_id;
                next_synthetic_id -= 1;
                id
            });
            let key = (rf.project_name.clone(), dedup_id);
            if seen.insert(key) {
                all_files.push(rf);
            }
        }
    }

    Ok(ResolvedCollection { files: all_files })
}

/// Resolve a single-file reference string to its `TrackedFile` and database id.
///
/// Parses the reference, resolves it against the context, asserts exactly one
/// file matched, and extracts the required database id.
pub fn resolve_file_ref(reference: &str, ctx: &Context) -> Result<(ResolvedFile, i64)> {
    let parsed = parse_reference(reference)?;
    let collection = resolve_references(&[parsed], ctx)?;
    let resolved = collection.expect_one(reference)?;
    let file_id = resolved
        .file
        .id
        .ok_or_else(|| anyhow::anyhow!("file has no id"))?;
    Ok((resolved, file_id))
}

pub struct ExpandedScope {
    pub project_name: Option<String>,
    pub category_name: Option<String>,
    pub project_root: PathBuf,
}

pub fn expand_reference_scope(scope: &[ScopeLevel], ctx: &Context) -> Result<Vec<ExpandedScope>> {
    let pairs = expand_scope(scope, ctx)?;
    pairs
        .into_iter()
        .map(|(project_name, category_name, _db)| {
            let project_root = derive_project_root(project_name.as_ref(), ctx)?;
            Ok(ExpandedScope {
                project_name,
                category_name,
                project_root,
            })
        })
        .collect()
}

fn derive_project_root(project_name: Option<&String>, ctx: &Context) -> Result<PathBuf> {
    match project_name {
        None => match ctx {
            Context::Project { project_root, .. } => Ok(project_root.clone()),
            _ => bail!("expected project context for unnamed scope"),
        },
        Some(name) => resolve_workspace_project_root(name, ctx),
    }
}

fn resolve_workspace_project_root(name: &str, ctx: &Context) -> Result<PathBuf> {
    let (ws_root, ws_db) = match ctx {
        Context::Project {
            workspace: Some(ws),
            ..
        } => (ws.workspace_root.as_path(), &ws.workspace_db),
        Context::Workspace {
            workspace_root,
            workspace_db,
        } => (workspace_root.as_path(), workspace_db),
        _ => bail!("workspace context required for project '{name}'"),
    };
    let proj = ws_db
        .get_project_by_name(name)?
        .ok_or_else(|| anyhow::anyhow!("project '{name}' not found"))?;
    Ok(ws_root.join(&proj.path))
}

fn resolve_single(reference: &Reference, ctx: &Context) -> Result<Vec<ResolvedFile>> {
    match reference {
        Reference::BarePath(path) => resolve_bare_path(path, ctx),
        Reference::Workspace { scope, tags, glob } | Reference::Context { scope, tags, glob } => {
            resolve_structured(scope, tags, glob.as_deref(), ctx)
        }
    }
}

fn resolve_bare_path(path: &str, ctx: &Context) -> Result<Vec<ResolvedFile>> {
    let Context::Project {
        project_db,
        project_root,
        ..
    } = ctx
    else {
        bail!("bare path requires project context");
    };

    let abs_path = project_root.join(path);
    if !abs_path.is_file() {
        return Ok(vec![]);
    }

    let hash = integrity::hash_file(&abs_path)?;
    if let Some(mut file) = project_db.get_file_by_hash(&hash)? {
        file.path = Some(path.to_string());
        return Ok(vec![ResolvedFile {
            project_name: None,
            rel_path: path.to_string(),
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

    let tag_groups: Vec<Vec<&str>> = tags
        .iter()
        .map(|tf| tf.tags.iter().map(String::as_str).collect())
        .collect();
    let glob_pattern = glob.map(glob::Pattern::new).transpose()?;

    for (project_name, category_name, project_db) in &pairs {
        let project_root = derive_project_root(project_name.as_ref(), ctx)?;
        let patterns = walk::category_patterns(project_db, category_name.as_deref())?;
        let entries = walk::walk_and_collect(&project_root, &patterns)?;

        for rel_path in entries {
            if !matches_glob_filter(glob_pattern.as_ref(), &rel_path) {
                continue;
            }

            let abs_path = project_root.join(&rel_path);
            let hash = integrity::hash_file(&abs_path)?;
            let Some(mut db_file) = project_db.get_file_by_hash(&hash)? else {
                continue;
            };

            if !matches_tag_groups(&tag_groups, db_file.id, project_db)? {
                continue;
            }

            db_file.path = Some(rel_path.clone());
            results.push(ResolvedFile {
                project_name: project_name.clone(),
                rel_path,
                file: db_file,
            });
        }
    }

    Ok(results)
}

fn matches_glob_filter(pattern: Option<&glob::Pattern>, rel_path: &str) -> bool {
    match pattern {
        Some(p) => {
            let file_name = rel_path.rsplit('/').next().unwrap_or(rel_path);
            p.matches(file_name) || p.matches(rel_path)
        }
        None => true,
    }
}

fn matches_tag_groups(
    tag_groups: &[Vec<&str>],
    file_id: Option<i64>,
    db: &ProjectDb,
) -> Result<bool> {
    if tag_groups.is_empty() || tag_groups.iter().all(Vec::is_empty) {
        return Ok(true);
    }
    let Some(id) = file_id else {
        return Ok(false);
    };
    let file_tags = db.get_tags(id)?;
    for group in tag_groups {
        if group.is_empty() {
            continue;
        }
        if !group.iter().any(|t| file_tags.iter().any(|ft| ft == t)) {
            return Ok(false);
        }
    }
    Ok(true)
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
                if workspace_db.get_project_by_name(name)?.is_some() {
                    let db = open_project_db(workspace_db, workspace_root, name)?;
                    results.push((Some(name.clone()), None, OpenedDb::Owned(db)));
                } else {
                    let found = find_category_across_projects(workspace_db, workspace_root, name)?;
                    if found.is_empty() {
                        bail!("'{name}' is not a project or category in any project");
                    }
                    results.extend(found);
                }
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
    Ok(project_db.get_category_by_name(name)?.is_some())
}

fn open_all_workspace_project_dbs(
    workspace_db: &WorkspaceDb,
    workspace_root: &std::path::Path,
) -> Result<Vec<(ProjectRow, ProjectDb)>> {
    let projects = workspace_db.list_projects()?;
    let mut opened = Vec::new();
    for proj in projects {
        let mkrk = workspace_root.join(&proj.path).join(".mkrk");
        if mkrk.exists() {
            let db = ProjectDb::open(&mkrk)?;
            opened.push((proj, db));
        }
    }
    Ok(opened)
}

fn find_category_across_projects(
    workspace_db: &WorkspaceDb,
    workspace_root: &std::path::Path,
    category_name: &str,
) -> Result<ScopeExpansion<'static>> {
    let mut results = Vec::new();
    for (proj, db) in open_all_workspace_project_dbs(workspace_db, workspace_root)? {
        if is_category_in_project(&db, category_name)? {
            results.push((
                Some(proj.name.clone()),
                Some(category_name.to_string()),
                OpenedDb::Owned(db),
            ));
        }
    }
    Ok(results)
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
    Ok(
        open_all_workspace_project_dbs(workspace_db, workspace_root)?
            .into_iter()
            .map(|(proj, db)| (Some(proj.name), None, OpenedDb::Owned(db)))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::WorkspaceContext;
    use crate::models::TrackedFile;
    use crate::reference::parse::parse_reference;
    use tempfile::TempDir;

    /// Create a file on disk at `root/rel_path` with unique content, returning the SHA-256 hash.
    fn create_disk_file(root: &std::path::Path, rel_path: &str) -> String {
        let abs = root.join(rel_path);
        std::fs::create_dir_all(abs.parent().unwrap()).unwrap();
        // Use rel_path as content so every path gets a unique hash.
        std::fs::write(&abs, rel_path.as_bytes()).unwrap();
        crate::integrity::hash_file(&abs).unwrap()
    }

    fn make_file(_name: &str, _path: &str, sha256: &str) -> TrackedFile {
        TrackedFile {
            id: None,
            name: None,
            path: None,
            sha256: sha256.to_string(),
            fingerprint: "[]".to_string(),
            mime_type: None,
            size: None,
            ingested_at: "2025-01-01T00:00:00Z".to_string(),
            provenance: None,
            immutable: false,
        }
    }

    fn setup_project(dir: &std::path::Path) -> ProjectDb {
        let db = ProjectDb::create(&dir.join(".mkrk")).unwrap();
        let cat = crate::models::Category {
            id: None,
            name: "evidence".to_string(),
            pattern: "evidence/**".to_string(),
            category_type: crate::models::CategoryType::Files,
            description: None,
        };
        db.insert_category(&cat).unwrap();
        let cat2 = crate::models::Category {
            id: None,
            name: "notes".to_string(),
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
            project_name: None,
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
            name: "evidence".to_string(),
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
            project_name: None,
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
        let hash = create_disk_file(dir.path(), "evidence/report.pdf");
        db.insert_file(&make_file("report.pdf", "evidence/report.pdf", &hash))
            .unwrap();

        let ctx = make_project_ctx(dir.path());
        let coll = resolve_one("evidence/report.pdf", &ctx);
        assert_eq!(coll.files.len(), 1);
        assert_eq!(coll.files[0].rel_path, "evidence/report.pdf");
    }

    #[test]
    fn resolve_bare_path_returns_empty() {
        let dir = TempDir::new().unwrap();
        let db = setup_project(dir.path());
        let hash = create_disk_file(dir.path(), "evidence/report.pdf");
        db.insert_file(&make_file("report.pdf", "evidence/report.pdf", &hash))
            .unwrap();

        let ctx = make_project_ctx(dir.path());
        // Bare path with / — no file at project_root/nonexistent/file.txt
        let coll = resolve_one("nonexistent/file.txt", &ctx);
        assert!(coll.files.is_empty());
    }

    #[test]
    fn resolve_category_scope() {
        let dir = TempDir::new().unwrap();
        let db = setup_project(dir.path());
        let h1 = create_disk_file(dir.path(), "evidence/report.pdf");
        let h2 = create_disk_file(dir.path(), "notes/todo.md");
        db.insert_file(&make_file("report.pdf", "evidence/report.pdf", &h1))
            .unwrap();
        db.insert_file(&make_file("todo.md", "notes/todo.md", &h2))
            .unwrap();

        let ctx = make_project_ctx(dir.path());
        let coll = resolve_one(":evidence", &ctx);
        assert_eq!(coll.files.len(), 1);
        assert_eq!(coll.files[0].rel_path, "evidence/report.pdf");
    }

    #[test]
    fn resolve_tag_filter() {
        let dir = TempDir::new().unwrap();
        let db = setup_project(dir.path());
        let h1 = create_disk_file(dir.path(), "evidence/report.pdf");
        let h2 = create_disk_file(dir.path(), "evidence/memo.pdf");
        let id1 = db
            .insert_file(&make_file("report.pdf", "evidence/report.pdf", &h1))
            .unwrap();
        db.insert_file(&make_file("memo.pdf", "evidence/memo.pdf", &h2))
            .unwrap();
        db.insert_tag(id1, "classified", "testhash", "[]").unwrap();

        let ctx = make_project_ctx(dir.path());
        let coll = resolve_one(":evidence!classified", &ctx);
        assert_eq!(coll.files.len(), 1);
        assert_eq!(coll.files[0].rel_path, "evidence/report.pdf");
    }

    #[test]
    fn resolve_tag_and_or() {
        let dir = TempDir::new().unwrap();
        let db = setup_project(dir.path());
        let ha = create_disk_file(dir.path(), "evidence/a.pdf");
        let hb = create_disk_file(dir.path(), "evidence/b.pdf");
        let hc = create_disk_file(dir.path(), "evidence/c.pdf");
        let id1 = db
            .insert_file(&make_file("a.pdf", "evidence/a.pdf", &ha))
            .unwrap();
        let id2 = db
            .insert_file(&make_file("b.pdf", "evidence/b.pdf", &hb))
            .unwrap();
        let id3 = db
            .insert_file(&make_file("c.pdf", "evidence/c.pdf", &hc))
            .unwrap();
        db.insert_tag(id1, "classified", "testhash", "[]").unwrap();
        db.insert_tag(id1, "priority", "testhash", "[]").unwrap();
        db.insert_tag(id2, "classified", "testhash", "[]").unwrap();
        db.insert_tag(id3, "priority", "testhash", "[]").unwrap();

        let ctx = make_project_ctx(dir.path());

        // classified AND priority -> only a.pdf
        let coll = resolve_one(":evidence!classified!priority", &ctx);
        assert_eq!(coll.files.len(), 1);
        assert_eq!(coll.files[0].rel_path, "evidence/a.pdf");

        // classified OR priority -> a.pdf, b.pdf, c.pdf
        let coll = resolve_one(":evidence!classified,priority", &ctx);
        assert_eq!(coll.files.len(), 3);
    }

    #[test]
    fn resolve_glob_filter() {
        let dir = TempDir::new().unwrap();
        let db = setup_project(dir.path());
        let h1 = create_disk_file(dir.path(), "evidence/report.pdf");
        let h2 = create_disk_file(dir.path(), "evidence/photo.jpg");
        db.insert_file(&make_file("report.pdf", "evidence/report.pdf", &h1))
            .unwrap();
        db.insert_file(&make_file("photo.jpg", "evidence/photo.jpg", &h2))
            .unwrap();

        let ctx = make_project_ctx(dir.path());
        let coll = resolve_one(":evidence/*.pdf", &ctx);
        assert_eq!(coll.files.len(), 1);
        assert_eq!(coll.files[0].rel_path, "evidence/report.pdf");
    }

    #[test]
    fn resolve_cross_project() {
        let ws = setup_workspace();
        let (proj1_dir, db1) = add_workspace_project(&ws, "bailey");
        let h1 = create_disk_file(&proj1_dir, "evidence/b-report.pdf");
        db1.insert_file(&make_file("b-report.pdf", "evidence/b-report.pdf", &h1))
            .unwrap();
        let (proj2_dir, db2) = add_workspace_project(&ws, "george");
        let h2 = create_disk_file(&proj2_dir, "evidence/g-report.pdf");
        db2.insert_file(&make_file("g-report.pdf", "evidence/g-report.pdf", &h2))
            .unwrap();

        let ctx = make_ws_project_ctx(&ws, &proj1_dir);
        let coll = resolve_one(":{bailey,george}.evidence", &ctx);
        assert_eq!(coll.files.len(), 2);

        let paths: Vec<&str> = coll.files.iter().map(|f| f.rel_path.as_str()).collect();
        assert!(paths.contains(&"evidence/b-report.pdf"));
        assert!(paths.contains(&"evidence/g-report.pdf"));
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
        let h1 = create_disk_file(dir.path(), "evidence/emails/email1.eml");
        let h2 = create_disk_file(dir.path(), "evidence/photos/photo.jpg");
        db.insert_file(&make_file("email1.eml", "evidence/emails/email1.eml", &h1))
            .unwrap();
        db.insert_file(&make_file("photo.jpg", "evidence/photos/photo.jpg", &h2))
            .unwrap();

        let ctx = make_project_ctx(dir.path());
        let coll = resolve_one(":evidence.emails", &ctx);
        assert_eq!(coll.files.len(), 1);
        assert_eq!(coll.files[0].rel_path, "evidence/emails/email1.eml");
    }

    #[test]
    fn resolve_subcategory_not_category() {
        let ws = setup_workspace();
        let (proj_dir, db) = add_workspace_project(&ws, "bailey");
        let h1 = create_disk_file(&proj_dir, "evidence/doc.pdf");
        db.insert_file(&make_file("doc.pdf", "evidence/doc.pdf", &h1))
            .unwrap();

        // Inside a project where "bailey" is NOT a category
        let ctx = make_ws_project_ctx(&ws, &proj_dir);
        // :bailey.evidence → falls back to project.category
        let coll = resolve_one(":bailey.evidence", &ctx);
        assert_eq!(coll.files.len(), 1);
        assert_eq!(coll.files[0].rel_path, "evidence/doc.pdf");
        assert_eq!(coll.files[0].project_name.as_deref(), Some("bailey"));
    }

    #[test]
    fn resolve_three_levels() {
        let ws = setup_workspace();
        let (proj_dir, db) = add_workspace_project(&ws, "bailey");
        let h1 = create_disk_file(&proj_dir, "evidence/emails/email.eml");
        let h2 = create_disk_file(&proj_dir, "evidence/photos/photo.jpg");
        db.insert_file(&make_file("email.eml", "evidence/emails/email.eml", &h1))
            .unwrap();
        db.insert_file(&make_file("photo.jpg", "evidence/photos/photo.jpg", &h2))
            .unwrap();

        let ctx = Context::Workspace {
            workspace_root: ws.ws_dir.path().to_path_buf(),
            workspace_db: WorkspaceDb::open(&ws.ws_dir.path().join(".mksp")).unwrap(),
        };

        // :bailey.evidence.emails → project.category.subcategory
        let coll = resolve_one(":bailey.evidence.emails", &ctx);
        assert_eq!(coll.files.len(), 1);
        assert_eq!(coll.files[0].rel_path, "evidence/emails/email.eml");
    }

    #[test]
    fn resolve_subcategory_brace_expansion() {
        let dir = TempDir::new().unwrap();
        let db = setup_project(dir.path());
        let h1 = create_disk_file(dir.path(), "evidence/emails/email.eml");
        let h2 = create_disk_file(dir.path(), "notes/drafts/memo.md");
        let h3 = create_disk_file(dir.path(), "evidence/photos/photo.jpg");
        db.insert_file(&make_file("email.eml", "evidence/emails/email.eml", &h1))
            .unwrap();
        db.insert_file(&make_file("memo.md", "notes/drafts/memo.md", &h2))
            .unwrap();
        db.insert_file(&make_file("photo.jpg", "evidence/photos/photo.jpg", &h3))
            .unwrap();

        let ctx = make_project_ctx(dir.path());
        // :{evidence,notes}.drafts → evidence/drafts/ and notes/drafts/
        let coll = resolve_one(":{evidence,notes}.drafts", &ctx);
        assert_eq!(coll.files.len(), 1);
        assert_eq!(coll.files[0].rel_path, "notes/drafts/memo.md");
    }
}
