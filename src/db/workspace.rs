use std::path::Path;

use anyhow::{Context, Result};
use chrono::Utc;
use rusqlite::{params, Connection};

use crate::models::{Category, ProtectionLevel};

use super::schema::WORKSPACE_SCHEMA;

pub struct WorkspaceDb {
    conn: Connection,
}

impl WorkspaceDb {
    pub fn create(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)
            .with_context(|| format!("failed to create workspace db at {}", path.display()))?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        conn.execute_batch(WORKSPACE_SCHEMA)?;
        Ok(Self { conn })
    }

    pub fn open(path: &Path) -> Result<Self> {
        if !path.exists() {
            anyhow::bail!("workspace database not found: {}", path.display());
        }
        let conn = Connection::open(path)
            .with_context(|| format!("failed to open workspace db at {}", path.display()))?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        Ok(Self { conn })
    }

    pub fn get_config(&self, key: &str) -> Result<Option<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT value FROM workspace_config WHERE key = ?1")?;
        let mut rows = stmt.query_map(params![key], |row| row.get(0))?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    pub fn set_config(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO workspace_config (key, value) VALUES (?1, ?2)",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn register_project(
        &self,
        name: &str,
        path: &str,
        description: Option<&str>,
    ) -> Result<i64> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO projects (name, path, description, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![name, path, description, now],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn list_projects(&self) -> Result<Vec<ProjectRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, path, description, created_at FROM projects ORDER BY name",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(ProjectRow {
                id: row.get(0)?,
                name: row.get(1)?,
                path: row.get(2)?,
                description: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn get_project_by_name(&self, name: &str) -> Result<Option<ProjectRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, path, description, created_at FROM projects WHERE name = ?1",
        )?;
        let mut rows = stmt.query_map(params![name], |row| {
            Ok(ProjectRow {
                id: row.get(0)?,
                name: row.get(1)?,
                path: row.get(2)?,
                description: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    pub fn insert_default_category(&self, cat: &Category) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO default_categories (pattern, protection_level, description) VALUES (?1, ?2, ?3)",
            params![cat.pattern, cat.protection_level.to_string(), cat.description],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn list_default_categories(&self) -> Result<Vec<Category>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, pattern, protection_level, description FROM default_categories")?;
        let rows = stmt.query_map([], |row| {
            Ok(Category {
                id: Some(row.get(0)?),
                pattern: row.get(1)?,
                protection_level: row
                    .get::<_, String>(2)?
                    .parse()
                    .unwrap_or(ProtectionLevel::Editable),
                description: row.get(3)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn project_count(&self) -> Result<i64> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM projects", [], |row| row.get(0))?)
    }

    pub fn get_tool_config(
        &self,
        scope: Option<&str>,
        action: &str,
        file_type: &str,
    ) -> Result<Option<super::project::ToolConfigRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, scope, action, file_type, command, env FROM tool_config
             WHERE (scope = ?1 OR scope IS NULL) AND action = ?2 AND (file_type = ?3 OR file_type = '*')
             ORDER BY
                CASE WHEN scope IS NOT NULL THEN 0 ELSE 1 END,
                CASE WHEN file_type = '*' THEN 1 ELSE 0 END
             LIMIT 1",
        )?;
        let mut rows = stmt.query_map(params![scope, action, file_type], |row| {
            Ok(super::project::ToolConfigRow {
                id: row.get(0)?,
                scope: row.get(1)?,
                action: row.get(2)?,
                file_type: row.get(3)?,
                command: row.get(4)?,
                env: row.get(5)?,
            })
        })?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    pub fn get_tag_tool_configs(
        &self,
        tags: &[String],
        action: &str,
        file_type: &str,
    ) -> Result<Vec<super::project::TagToolConfigRow>> {
        if tags.is_empty() {
            return Ok(vec![]);
        }
        let placeholders: Vec<String> = tags
            .iter()
            .enumerate()
            .map(|(i, _)| format!("?{}", i + 3))
            .collect();
        let sql = format!(
            "SELECT id, tag, action, file_type, command, env FROM tag_tool_config
             WHERE tag IN ({}) AND action = ?1 AND (file_type = ?2 OR file_type = '*')
             ORDER BY tag",
            placeholders.join(", ")
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = vec![
            Box::new(action.to_string()),
            Box::new(file_type.to_string()),
        ];
        for tag in tags {
            param_values.push(Box::new(tag.clone()));
        }
        let params_ref: Vec<&dyn rusqlite::types::ToSql> = param_values
            .iter()
            .map(std::convert::AsRef::as_ref)
            .collect();
        let rows = stmt.query_map(params_ref.as_slice(), |row| {
            Ok(super::project::TagToolConfigRow {
                id: row.get(0)?,
                tag: row.get(1)?,
                action: row.get(2)?,
                file_type: row.get(3)?,
                command: row.get(4)?,
                env: row.get(5)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
}

#[derive(Debug, Clone)]
pub struct ProjectRow {
    pub id: i64,
    pub name: String,
    pub path: String,
    pub description: Option<String>,
    pub created_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup() -> (TempDir, WorkspaceDb) {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join(".mksp");
        let db = WorkspaceDb::create(&db_path).unwrap();
        (dir, db)
    }

    #[test]
    fn create_and_open() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join(".mksp");
        WorkspaceDb::create(&db_path).unwrap();
        WorkspaceDb::open(&db_path).unwrap();
    }

    #[test]
    fn config_crud() {
        let (_dir, db) = setup();
        assert!(db.get_config("projects_dir").unwrap().is_none());
        db.set_config("projects_dir", "projects").unwrap();
        assert_eq!(db.get_config("projects_dir").unwrap().unwrap(), "projects");
        db.set_config("projects_dir", "projs").unwrap();
        assert_eq!(db.get_config("projects_dir").unwrap().unwrap(), "projs");
    }

    #[test]
    fn project_registration() {
        let (_dir, db) = setup();
        db.register_project("bailey", "projects/bailey", Some("Bailey investigation"))
            .unwrap();
        db.register_project("epstein", "projects/epstein", None)
            .unwrap();

        let projects = db.list_projects().unwrap();
        assert_eq!(projects.len(), 2);
        assert_eq!(projects[0].name, "bailey");
        assert_eq!(projects[1].name, "epstein");

        let found = db.get_project_by_name("bailey").unwrap().unwrap();
        assert_eq!(found.path, "projects/bailey");
    }

    #[test]
    fn default_categories() {
        let (_dir, db) = setup();
        db.insert_default_category(&Category {
            id: None,
            pattern: "evidence/**".to_string(),
            protection_level: ProtectionLevel::Immutable,
            description: Some("Evidence".to_string()),
        })
        .unwrap();
        db.insert_default_category(&Category {
            id: None,
            pattern: "notes/**".to_string(),
            protection_level: ProtectionLevel::Editable,
            description: Some("Notes".to_string()),
        })
        .unwrap();

        let cats = db.list_default_categories().unwrap();
        assert_eq!(cats.len(), 2);
    }
}
