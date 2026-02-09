use std::path::Path;

use anyhow::{bail, Result};

use crate::db::{ProjectDb, WorkspaceDb};
use crate::models::{Category, CategoryType, ProtectionLevel};
use crate::reference::validate_name;

const DEFAULT_CATEGORIES: &[(&str, &str, &str, &str)] = &[
    ("evidence/**", "files", "immutable", "Evidence files"),
    ("sources/**", "files", "immutable", "Source materials"),
    ("analysis/**", "files", "protected", "Analysis documents"),
    ("notes/**", "files", "editable", "Working notes"),
    ("tools/**", "tools", "editable", "Project tools"),
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
    if cwd.join(".mksp").exists() {
        bail!("workspace already exists in {}", cwd.display());
    }

    let project_name = cwd
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    if !project_name.is_empty() {
        validate_name(&project_name)?;
    }

    let project_db = ProjectDb::create(&db_path)?;

    let items = resolve_categories_with_policies(cwd, no_categories, custom_categories)?;
    for (cat, level) in &items {
        let cat_id = project_db.insert_category(cat)?;
        project_db.insert_category_policy(cat_id, level)?;
    }

    register_in_workspace(cwd)?;

    let cat_count = items.len();
    eprintln!("Initialized project in {}", cwd.display());
    if cat_count > 0 {
        eprintln!("  {cat_count} categories configured");
    }

    Ok(())
}

fn resolve_categories_with_policies(
    cwd: &Path,
    no_categories: bool,
    custom_categories: &[String],
) -> Result<Vec<(Category, ProtectionLevel)>> {
    if !custom_categories.is_empty() {
        return parse_custom_with_policies(custom_categories);
    }

    if no_categories {
        return Ok(vec![]);
    }

    if let Some(ws_items) = load_workspace_defaults(cwd)? {
        if !ws_items.is_empty() {
            return Ok(ws_items);
        }
    }

    Ok(default_categories_with_policies())
}

fn parse_custom_with_policies(specs: &[String]) -> Result<Vec<(Category, ProtectionLevel)>> {
    specs
        .iter()
        .map(|s| {
            let parts: Vec<&str> = s.splitn(3, ':').collect();
            match parts.len() {
                2 => {
                    let protection_level: ProtectionLevel = parts[1].parse()?;
                    Ok((
                        Category {
                            id: None,
                            pattern: parts[0].to_string(),
                            category_type: CategoryType::Files,
                            description: None,
                        },
                        protection_level,
                    ))
                }
                3 => {
                    let category_type: CategoryType = parts[1].parse()?;
                    let protection_level: ProtectionLevel = parts[2].parse()?;
                    Ok((
                        Category {
                            id: None,
                            pattern: parts[0].to_string(),
                            category_type,
                            description: None,
                        },
                        protection_level,
                    ))
                }
                _ => bail!("invalid category format '{s}', expected 'pattern:level' or 'pattern:type:level'"),
            }
        })
        .collect()
}

fn load_workspace_defaults(cwd: &Path) -> Result<Option<Vec<(Category, ProtectionLevel)>>> {
    let mut dir = cwd.to_path_buf();
    loop {
        let mksp = dir.join(".mksp");
        if mksp.exists() {
            let ws_db = WorkspaceDb::open(&mksp)?;
            let items = ws_db.list_default_categories_with_policies()?;
            return Ok(Some(items));
        }
        if !dir.pop() {
            break;
        }
    }
    Ok(None)
}

fn default_categories_with_policies() -> Vec<(Category, ProtectionLevel)> {
    DEFAULT_CATEGORIES
        .iter()
        .map(|(pattern, cat_type, level, desc)| {
            (
                Category {
                    id: None,
                    pattern: (*pattern).to_string(),
                    category_type: cat_type.parse().unwrap(),
                    description: Some((*desc).to_string()),
                },
                level.parse().unwrap(),
            )
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
            validate_name(&name)?;
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
    if cwd.join(".mkrk").exists() {
        bail!("project already exists in {}", cwd.display());
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
        for (pattern, cat_type, level, desc) in DEFAULT_CATEGORIES {
            let cat = Category {
                id: None,
                pattern: (*pattern).to_string(),
                category_type: cat_type.parse().unwrap(),
                description: Some((*desc).to_string()),
            };
            let cat_id = ws_db.insert_default_category(&cat)?;
            ws_db.insert_default_category_policy(cat_id, &level.parse().unwrap())?;
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
