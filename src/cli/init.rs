use std::path::Path;

use anyhow::{bail, Result};

use crate::db::{ProjectDb, WorkspaceDb};
use crate::models::{Category, ProtectionLevel};

const DEFAULT_CATEGORIES: &[(&str, &str, &str)] = &[
    ("evidence/**", "immutable", "Evidence files"),
    ("sources/**", "immutable", "Source materials"),
    ("analysis/**", "protected", "Analysis documents"),
    ("notes/**", "editable", "Working notes"),
];

pub fn run_init_project(
    cwd: &Path,
    no_categories: bool,
    custom_categories: &[String],
) -> Result<()> {
    let db_path = cwd.join(".mkrk");
    if db_path.exists() {
        bail!("project already exists in {}", cwd.display());
    }

    let project_db = ProjectDb::create(&db_path)?;

    let categories = resolve_categories(cwd, no_categories, custom_categories)?;
    for cat in &categories {
        project_db.insert_category(cat)?;
    }

    register_in_workspace(cwd)?;

    let cat_count = categories.len();
    eprintln!("Initialized project in {}", cwd.display());
    if cat_count > 0 {
        eprintln!("  {cat_count} categories configured");
    }

    Ok(())
}

fn resolve_categories(
    cwd: &Path,
    no_categories: bool,
    custom_categories: &[String],
) -> Result<Vec<Category>> {
    if !custom_categories.is_empty() {
        return parse_custom_categories(custom_categories);
    }

    if no_categories {
        return Ok(vec![]);
    }

    if let Some(ws_cats) = load_workspace_defaults(cwd)? {
        if !ws_cats.is_empty() {
            return Ok(ws_cats);
        }
    }

    Ok(default_categories())
}

fn parse_custom_categories(specs: &[String]) -> Result<Vec<Category>> {
    specs
        .iter()
        .map(|s| {
            let parts: Vec<&str> = s.splitn(2, ':').collect();
            if parts.len() != 2 {
                bail!("invalid category format '{s}', expected 'pattern:level'");
            }
            let protection_level: ProtectionLevel = parts[1].parse()?;
            Ok(Category {
                id: None,
                pattern: parts[0].to_string(),
                protection_level,
                description: None,
            })
        })
        .collect()
}

fn load_workspace_defaults(cwd: &Path) -> Result<Option<Vec<Category>>> {
    let mut dir = cwd.to_path_buf();
    loop {
        let mksp = dir.join(".mksp");
        if mksp.exists() {
            let ws_db = WorkspaceDb::open(&mksp)?;
            let cats = ws_db.list_default_categories()?;
            return Ok(Some(cats));
        }
        if !dir.pop() {
            break;
        }
    }
    Ok(None)
}

fn default_categories() -> Vec<Category> {
    DEFAULT_CATEGORIES
        .iter()
        .map(|(pattern, level, desc)| Category {
            id: None,
            pattern: (*pattern).to_string(),
            protection_level: level.parse().unwrap(),
            description: Some((*desc).to_string()),
        })
        .collect()
}

fn register_in_workspace(project_dir: &Path) -> Result<()> {
    let mut dir = project_dir.parent().map(Path::to_path_buf);
    while let Some(d) = dir {
        let mksp = d.join(".mksp");
        if mksp.exists() {
            let ws_db = WorkspaceDb::open(&mksp)?;
            let rel_path = project_dir
                .strip_prefix(&d)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();
            let name = project_dir.file_name().map_or_else(
                || "unnamed".to_string(),
                |n| n.to_string_lossy().to_string(),
            );
            ws_db.register_project(&name, &rel_path, None)?;
            eprintln!("  Registered in workspace at {}", d.display());
            return Ok(());
        }
        dir = d.parent().map(Path::to_path_buf);
    }
    Ok(())
}

pub fn run_init_workspace(
    cwd: &Path,
    projects_dir: &str,
    inbox: bool,
    no_categories: bool,
) -> Result<()> {
    let db_path = cwd.join(".mksp");
    if db_path.exists() {
        bail!("workspace already exists in {}", cwd.display());
    }

    validate_projects_dir(projects_dir)?;

    let ws_db = WorkspaceDb::create(&db_path)?;
    ws_db.set_config("projects_dir", projects_dir)?;

    let full_projects_dir = cwd.join(projects_dir);
    std::fs::create_dir_all(&full_projects_dir)?;

    if inbox {
        let inbox_dir = cwd.join("inbox");
        std::fs::create_dir_all(&inbox_dir)?;
        ws_db.set_config("inbox_dir", "inbox")?;
    }

    if !no_categories {
        for (pattern, level, desc) in DEFAULT_CATEGORIES {
            ws_db.insert_default_category(&Category {
                id: None,
                pattern: (*pattern).to_string(),
                protection_level: level.parse().unwrap(),
                description: Some((*desc).to_string()),
            })?;
        }
    }

    eprintln!("Initialized workspace in {}", cwd.display());
    eprintln!("  Projects directory: {projects_dir}");
    if inbox {
        eprintln!("  Inbox enabled");
    }

    Ok(())
}

fn validate_projects_dir(dir: &str) -> Result<()> {
    if dir.starts_with('/') {
        bail!("projects directory must be a relative path");
    }
    if dir.contains("..") {
        bail!("projects directory must not contain '..'");
    }
    let path = Path::new(dir);
    if path.is_symlink() {
        bail!("projects directory must not be a symlink");
    }
    Ok(())
}
