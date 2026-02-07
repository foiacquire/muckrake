use std::path::Path;

use anyhow::{Context, Result};
use chrono::Utc;
use rusqlite::Connection;
use sea_query::{
    Asterisk, CaseStatement, Cond, Expr, ExprTrait, Func, OnConflict, Order, Query,
    SqliteQueryBuilder,
};
use sea_query_rusqlite::RusqliteBinder;

use crate::models::{Category, ProtectionLevel};

use super::iden::{DefaultCategories, Projects, TagToolConfig, ToolConfig, WorkspaceConfig};
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
        let (sql, values) = Query::select()
            .column(WorkspaceConfig::Value)
            .from(WorkspaceConfig::Table)
            .and_where(Expr::col(WorkspaceConfig::Key).eq(key))
            .build_rusqlite(SqliteQueryBuilder);
        let mut stmt = self.conn.prepare(&sql)?;
        let mut rows = stmt.query_map(&*values.as_params(), |row| row.get(0))?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    pub fn set_config(&self, key: &str, value: &str) -> Result<()> {
        let (sql, values) = Query::insert()
            .into_table(WorkspaceConfig::Table)
            .columns([WorkspaceConfig::Key, WorkspaceConfig::Value])
            .values_panic([key.into(), value.into()])
            .on_conflict(
                OnConflict::column(WorkspaceConfig::Key)
                    .update_column(WorkspaceConfig::Value)
                    .to_owned(),
            )
            .build_rusqlite(SqliteQueryBuilder);
        self.conn.execute(&sql, &*values.as_params())?;
        Ok(())
    }

    pub fn register_project(
        &self,
        name: &str,
        path: &str,
        description: Option<&str>,
    ) -> Result<i64> {
        let now = Utc::now().to_rfc3339();
        let (sql, values) = Query::insert()
            .into_table(Projects::Table)
            .columns([
                Projects::Name,
                Projects::Path,
                Projects::Description,
                Projects::CreatedAt,
            ])
            .values_panic([
                name.into(),
                path.into(),
                description.map(String::from).into(),
                now.into(),
            ])
            .build_rusqlite(SqliteQueryBuilder);
        self.conn.execute(&sql, &*values.as_params())?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn list_projects(&self) -> Result<Vec<ProjectRow>> {
        let (sql, values) = Query::select()
            .columns([
                Projects::Id,
                Projects::Name,
                Projects::Path,
                Projects::Description,
                Projects::CreatedAt,
            ])
            .from(Projects::Table)
            .order_by(Projects::Name, Order::Asc)
            .build_rusqlite(SqliteQueryBuilder);
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(&*values.as_params(), |row| {
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
        let (sql, values) = Query::select()
            .columns([
                Projects::Id,
                Projects::Name,
                Projects::Path,
                Projects::Description,
                Projects::CreatedAt,
            ])
            .from(Projects::Table)
            .and_where(Expr::col(Projects::Name).eq(name))
            .build_rusqlite(SqliteQueryBuilder);
        let mut stmt = self.conn.prepare(&sql)?;
        let mut rows = stmt.query_map(&*values.as_params(), |row| {
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
        let (sql, values) = Query::insert()
            .into_table(DefaultCategories::Table)
            .columns([
                DefaultCategories::Pattern,
                DefaultCategories::ProtectionLevel,
                DefaultCategories::Description,
            ])
            .values_panic([
                cat.pattern.as_str().into(),
                cat.protection_level.to_string().into(),
                cat.description.clone().into(),
            ])
            .build_rusqlite(SqliteQueryBuilder);
        self.conn.execute(&sql, &*values.as_params())?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn list_default_categories(&self) -> Result<Vec<Category>> {
        let (sql, values) = Query::select()
            .columns([
                DefaultCategories::Id,
                DefaultCategories::Pattern,
                DefaultCategories::ProtectionLevel,
                DefaultCategories::Description,
            ])
            .from(DefaultCategories::Table)
            .build_rusqlite(SqliteQueryBuilder);
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(&*values.as_params(), |row| {
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
        let (sql, values) = Query::select()
            .expr(Func::count(Expr::col(Asterisk)))
            .from(Projects::Table)
            .build_rusqlite(SqliteQueryBuilder);
        Ok(self
            .conn
            .query_row(&sql, &*values.as_params(), |row| row.get(0))?)
    }

    pub fn get_tool_config(
        &self,
        scope: Option<&str>,
        action: &str,
        file_type: &str,
    ) -> Result<Option<super::project::ToolConfigRow>> {
        let (sql, values) = Query::select()
            .columns([
                ToolConfig::Id,
                ToolConfig::Scope,
                ToolConfig::Action,
                ToolConfig::FileType,
                ToolConfig::Command,
                ToolConfig::Env,
            ])
            .from(ToolConfig::Table)
            .cond_where(
                Cond::all()
                    .add(
                        Cond::any()
                            .add(Expr::col(ToolConfig::Scope).eq(scope.map(String::from)))
                            .add(Expr::col(ToolConfig::Scope).is_null()),
                    )
                    .add(Expr::col(ToolConfig::Action).eq(action))
                    .add(
                        Cond::any()
                            .add(Expr::col(ToolConfig::FileType).eq(file_type))
                            .add(Expr::col(ToolConfig::FileType).eq("*")),
                    ),
            )
            .order_by_expr(
                CaseStatement::new()
                    .case(Expr::col(ToolConfig::Scope).is_not_null(), 0)
                    .finally(1)
                    .into(),
                Order::Asc,
            )
            .order_by_expr(
                CaseStatement::new()
                    .case(Expr::col(ToolConfig::FileType).eq("*"), 1)
                    .finally(0)
                    .into(),
                Order::Asc,
            )
            .limit(1)
            .build_rusqlite(SqliteQueryBuilder);
        let mut stmt = self.conn.prepare(&sql)?;
        let mut rows = stmt.query_map(&*values.as_params(), |row| {
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
        let tag_values: Vec<sea_query::Value> = tags.iter().map(|t| t.as_str().into()).collect();
        let (sql, values) = Query::select()
            .columns([
                TagToolConfig::Id,
                TagToolConfig::Tag,
                TagToolConfig::Action,
                TagToolConfig::FileType,
                TagToolConfig::Command,
                TagToolConfig::Env,
            ])
            .from(TagToolConfig::Table)
            .and_where(Expr::col(TagToolConfig::Tag).is_in(tag_values))
            .and_where(Expr::col(TagToolConfig::Action).eq(action))
            .cond_where(
                Cond::any()
                    .add(Expr::col(TagToolConfig::FileType).eq(file_type))
                    .add(Expr::col(TagToolConfig::FileType).eq("*")),
            )
            .order_by(TagToolConfig::Tag, Order::Asc)
            .build_rusqlite(SqliteQueryBuilder);
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(&*values.as_params(), |row| {
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
