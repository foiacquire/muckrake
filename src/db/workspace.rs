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
use crate::reference::validate_name;

use super::iden::{
    DefaultCategories, DefaultCategoryPolicy, Projects, TagToolConfig, ToolConfig, WorkspaceConfig,
};
use super::schema::WORKSPACE_SCHEMA;

pub struct WorkspaceDb {
    conn: Connection,
}

impl WorkspaceDb {
    pub fn create(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)
            .with_context(|| format!("failed to create workspace db at {}", path.display()))?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        conn.execute_batch(&WORKSPACE_SCHEMA)?;
        migrate(&conn)?;
        Ok(Self { conn })
    }

    pub fn open(path: &Path) -> Result<Self> {
        if !path.exists() {
            anyhow::bail!("workspace database not found: {}", path.display());
        }
        let conn = Connection::open(path)
            .with_context(|| format!("failed to open workspace db at {}", path.display()))?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        migrate(&conn)?;
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
        validate_name(name)?;
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
            .columns(PROJECT_COLUMNS)
            .from(Projects::Table)
            .order_by(Projects::Name, Order::Asc)
            .build_rusqlite(SqliteQueryBuilder);
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(&*values.as_params(), row_to_project)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn get_project_by_name(&self, name: &str) -> Result<Option<ProjectRow>> {
        let (sql, values) = Query::select()
            .columns(PROJECT_COLUMNS)
            .from(Projects::Table)
            .and_where(Expr::col(Projects::Name).eq(name))
            .build_rusqlite(SqliteQueryBuilder);
        let mut stmt = self.conn.prepare(&sql)?;
        let mut rows = stmt.query_map(&*values.as_params(), row_to_project)?;
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
                DefaultCategories::CategoryType,
                DefaultCategories::Description,
            ])
            .values_panic([
                cat.pattern.as_str().into(),
                cat.category_type.to_string().into(),
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
                DefaultCategories::CategoryType,
                DefaultCategories::Description,
            ])
            .from(DefaultCategories::Table)
            .build_rusqlite(SqliteQueryBuilder);
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(&*values.as_params(), |row| {
            Ok(Category {
                id: Some(row.get(0)?),
                pattern: row.get(1)?,
                category_type: row.get::<_, String>(2)?.parse().unwrap_or_default(),
                description: row.get(3)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn insert_default_category_policy(
        &self,
        default_category_id: i64,
        level: &ProtectionLevel,
    ) -> Result<()> {
        let (sql, values) = Query::insert()
            .into_table(DefaultCategoryPolicy::Table)
            .columns([
                DefaultCategoryPolicy::DefaultCategoryId,
                DefaultCategoryPolicy::ProtectionLevel,
            ])
            .values_panic([default_category_id.into(), level.to_string().into()])
            .on_conflict(
                OnConflict::column(DefaultCategoryPolicy::DefaultCategoryId)
                    .update_column(DefaultCategoryPolicy::ProtectionLevel)
                    .to_owned(),
            )
            .build_rusqlite(SqliteQueryBuilder);
        self.conn.execute(&sql, &*values.as_params())?;
        Ok(())
    }

    pub fn get_default_category_policy(
        &self,
        default_category_id: i64,
    ) -> Result<Option<ProtectionLevel>> {
        let (sql, values) = Query::select()
            .column(DefaultCategoryPolicy::ProtectionLevel)
            .from(DefaultCategoryPolicy::Table)
            .and_where(Expr::col(DefaultCategoryPolicy::DefaultCategoryId).eq(default_category_id))
            .build_rusqlite(SqliteQueryBuilder);
        let mut stmt = self.conn.prepare(&sql)?;
        let mut rows = stmt.query_map(&*values.as_params(), |row| row.get::<_, String>(0))?;
        match rows.next() {
            Some(row) => {
                let s = row?;
                Ok(Some(s.parse().unwrap_or(ProtectionLevel::Editable)))
            }
            None => Ok(None),
        }
    }

    pub fn list_default_categories_with_policies(
        &self,
    ) -> Result<Vec<(Category, ProtectionLevel)>> {
        let cats = self.list_default_categories()?;
        let mut result = Vec::with_capacity(cats.len());
        for cat in cats {
            let level = cat
                .id
                .and_then(|id| self.get_default_category_policy(id).ok().flatten())
                .unwrap_or(ProtectionLevel::Editable);
            result.push((cat, level));
        }
        Ok(result)
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
            .columns(super::project::TOOL_CONFIG_COLUMNS)
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
        let mut rows = stmt.query_map(&*values.as_params(), super::project::row_to_tool_config)?;
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
            .columns(super::project::TAG_TOOL_CONFIG_COLUMNS)
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
        let rows = stmt.query_map(&*values.as_params(), super::project::row_to_tag_tool_config)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn insert_tool_config(&self, params: &super::project::ToolConfigParams<'_>) -> Result<i64> {
        let (sql, values) = Query::insert()
            .into_table(ToolConfig::Table)
            .columns([
                ToolConfig::Scope,
                ToolConfig::Action,
                ToolConfig::FileType,
                ToolConfig::Command,
                ToolConfig::Env,
                ToolConfig::Quiet,
            ])
            .values_panic([
                params.scope.map(String::from).into(),
                params.action.into(),
                params.file_type.into(),
                params.command.into(),
                params.env.map(String::from).into(),
                i32::from(params.quiet).into(),
            ])
            .build_rusqlite(SqliteQueryBuilder);
        self.conn.execute(&sql, &*values.as_params())?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn insert_tag_tool_config(
        &self,
        params: &super::project::TagToolConfigParams<'_>,
    ) -> Result<i64> {
        let (sql, values) = Query::insert()
            .into_table(TagToolConfig::Table)
            .columns([
                TagToolConfig::Tag,
                TagToolConfig::Action,
                TagToolConfig::FileType,
                TagToolConfig::Command,
                TagToolConfig::Env,
                TagToolConfig::Quiet,
            ])
            .values_panic([
                params.tag.into(),
                params.action.into(),
                params.file_type.into(),
                params.command.into(),
                params.env.map(String::from).into(),
                i32::from(params.quiet).into(),
            ])
            .build_rusqlite(SqliteQueryBuilder);
        self.conn.execute(&sql, &*values.as_params())?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn list_tool_configs(&self) -> Result<Vec<super::project::ToolConfigRow>> {
        let (sql, values) = Query::select()
            .columns(super::project::TOOL_CONFIG_COLUMNS)
            .from(ToolConfig::Table)
            .order_by(ToolConfig::Action, Order::Asc)
            .build_rusqlite(SqliteQueryBuilder);
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(&*values.as_params(), super::project::row_to_tool_config)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn list_tag_tool_configs(&self) -> Result<Vec<super::project::TagToolConfigRow>> {
        let (sql, values) = Query::select()
            .columns(super::project::TAG_TOOL_CONFIG_COLUMNS)
            .from(TagToolConfig::Table)
            .order_by(TagToolConfig::Tag, Order::Asc)
            .build_rusqlite(SqliteQueryBuilder);
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(&*values.as_params(), super::project::row_to_tag_tool_config)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn remove_tool_config(
        &self,
        action: &str,
        scope: Option<&str>,
        file_type: Option<&str>,
    ) -> Result<u64> {
        let mut query = Query::delete();
        query
            .from_table(ToolConfig::Table)
            .and_where(Expr::col(ToolConfig::Action).eq(action));
        match scope {
            Some(s) => {
                query.and_where(Expr::col(ToolConfig::Scope).eq(s));
            }
            None => {
                query.and_where(Expr::col(ToolConfig::Scope).is_null());
            }
        }
        if let Some(ft) = file_type {
            query.and_where(Expr::col(ToolConfig::FileType).eq(ft));
        }
        let (sql, values) = query.build_rusqlite(SqliteQueryBuilder);
        let count = self.conn.execute(&sql, &*values.as_params())?;
        Ok(count as u64)
    }

    pub fn remove_tag_tool_config(
        &self,
        action: &str,
        tag: &str,
        file_type: Option<&str>,
    ) -> Result<u64> {
        let mut query = Query::delete();
        query
            .from_table(TagToolConfig::Table)
            .and_where(Expr::col(TagToolConfig::Action).eq(action))
            .and_where(Expr::col(TagToolConfig::Tag).eq(tag));
        if let Some(ft) = file_type {
            query.and_where(Expr::col(TagToolConfig::FileType).eq(ft));
        }
        let (sql, values) = query.build_rusqlite(SqliteQueryBuilder);
        let count = self.conn.execute(&sql, &*values.as_params())?;
        Ok(count as u64)
    }
}

fn migrate(conn: &Connection) -> Result<()> {
    migrate_tool_config_quiet(conn)?;
    migrate_tag_tool_config_quiet(conn)?;
    migrate_default_category_type(conn)?;
    migrate_default_category_policy_table(conn)
}

fn migrate_add_column(conn: &Connection, table: &str, column: &str, alter_sql: &str) -> Result<()> {
    let table_exists = conn
        .prepare(&format!("SELECT id FROM {table} LIMIT 0"))
        .is_ok();
    if !table_exists {
        return Ok(());
    }
    let has_column = conn
        .prepare(&format!("SELECT {column} FROM {table} LIMIT 0"))
        .is_ok();
    if !has_column {
        conn.execute_batch(alter_sql)?;
    }
    Ok(())
}

fn migrate_tool_config_quiet(conn: &Connection) -> Result<()> {
    migrate_add_column(
        conn,
        "tool_config",
        "quiet",
        "ALTER TABLE tool_config ADD COLUMN quiet INTEGER NOT NULL DEFAULT 1;",
    )
}

fn migrate_tag_tool_config_quiet(conn: &Connection) -> Result<()> {
    migrate_add_column(
        conn,
        "tag_tool_config",
        "quiet",
        "ALTER TABLE tag_tool_config ADD COLUMN quiet INTEGER NOT NULL DEFAULT 1;",
    )
}

fn migrate_default_category_type(conn: &Connection) -> Result<()> {
    let has_column = conn
        .prepare("SELECT category_type FROM default_categories LIMIT 0")
        .is_ok();
    if !has_column {
        conn.execute_batch(
            "ALTER TABLE default_categories ADD COLUMN category_type TEXT NOT NULL DEFAULT 'files';",
        )?;
    }
    Ok(())
}

fn migrate_default_category_policy_table(conn: &Connection) -> Result<()> {
    let has_table = conn
        .prepare("SELECT id FROM default_category_policy LIMIT 0")
        .is_ok();
    if has_table {
        return Ok(());
    }
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS default_category_policy (
            id INTEGER PRIMARY KEY,
            default_category_id INTEGER NOT NULL REFERENCES default_categories(id),
            protection_level TEXT NOT NULL DEFAULT 'editable',
            UNIQUE(default_category_id)
        );",
    )?;
    let has_old_protection = conn
        .prepare("SELECT protection_level FROM default_categories LIMIT 0")
        .is_ok();
    if has_old_protection {
        conn.execute_batch(
            "INSERT INTO default_category_policy (default_category_id, protection_level)
             SELECT id, protection_level FROM default_categories;",
        )?;
    }
    Ok(())
}

const PROJECT_COLUMNS: [Projects; 5] = [
    Projects::Id,
    Projects::Name,
    Projects::Path,
    Projects::Description,
    Projects::CreatedAt,
];

#[derive(Debug, Clone)]
pub struct ProjectRow {
    pub id: i64,
    pub name: String,
    pub path: String,
    pub description: Option<String>,
    pub created_at: String,
}

fn row_to_project(row: &rusqlite::Row<'_>) -> rusqlite::Result<ProjectRow> {
    Ok(ProjectRow {
        id: row.get(0)?,
        name: row.get(1)?,
        path: row.get(2)?,
        description: row.get(3)?,
        created_at: row.get(4)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::CategoryType;
    use tempfile::TempDir;

    const OLD_SCHEMA_BASE: &str = "
        CREATE TABLE IF NOT EXISTS workspace_config (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS projects (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            path TEXT NOT NULL,
            description TEXT,
            created_at TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS tool_config (
            id INTEGER PRIMARY KEY,
            scope TEXT,
            action TEXT NOT NULL,
            file_type TEXT NOT NULL,
            command TEXT NOT NULL,
            env TEXT
        );
        CREATE TABLE IF NOT EXISTS tag_tool_config (
            id INTEGER PRIMARY KEY,
            tag TEXT NOT NULL,
            action TEXT NOT NULL,
            file_type TEXT NOT NULL,
            command TEXT NOT NULL,
            env TEXT,
            UNIQUE(tag, action, file_type)
        );
        CREATE TABLE IF NOT EXISTS entity_links (
            id INTEGER PRIMARY KEY,
            entity_name TEXT NOT NULL,
            entity_type TEXT NOT NULL,
            project_name TEXT NOT NULL,
            project_entity_id INTEGER,
            UNIQUE(entity_name, entity_type, project_name)
        );
    ";

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
        let id1 = db
            .insert_default_category(&Category {
                id: None,
                pattern: "evidence/**".to_string(),
                category_type: CategoryType::Files,
                description: Some("Evidence".to_string()),
            })
            .unwrap();
        db.insert_default_category_policy(id1, &ProtectionLevel::Immutable)
            .unwrap();

        let id2 = db
            .insert_default_category(&Category {
                id: None,
                pattern: "notes/**".to_string(),
                category_type: CategoryType::Files,
                description: Some("Notes".to_string()),
            })
            .unwrap();
        db.insert_default_category_policy(id2, &ProtectionLevel::Editable)
            .unwrap();

        let cats = db.list_default_categories().unwrap();
        assert_eq!(cats.len(), 2);
        assert_eq!(cats[0].category_type, CategoryType::Files);
    }

    #[test]
    fn default_category_policy_crud() {
        let (_dir, db) = setup();
        let cat_id = db
            .insert_default_category(&Category {
                id: None,
                pattern: "docs/**".to_string(),
                category_type: CategoryType::Files,
                description: None,
            })
            .unwrap();

        assert_eq!(db.get_default_category_policy(cat_id).unwrap(), None);

        db.insert_default_category_policy(cat_id, &ProtectionLevel::Protected)
            .unwrap();
        assert_eq!(
            db.get_default_category_policy(cat_id).unwrap(),
            Some(ProtectionLevel::Protected)
        );

        db.insert_default_category_policy(cat_id, &ProtectionLevel::Immutable)
            .unwrap();
        assert_eq!(
            db.get_default_category_policy(cat_id).unwrap(),
            Some(ProtectionLevel::Immutable)
        );
    }

    #[test]
    fn list_with_policies() {
        let (_dir, db) = setup();
        let id1 = db
            .insert_default_category(&Category {
                id: None,
                pattern: "evidence/**".to_string(),
                category_type: CategoryType::Files,
                description: None,
            })
            .unwrap();
        db.insert_default_category_policy(id1, &ProtectionLevel::Immutable)
            .unwrap();

        let id2 = db
            .insert_default_category(&Category {
                id: None,
                pattern: "notes/**".to_string(),
                category_type: CategoryType::Files,
                description: None,
            })
            .unwrap();
        db.insert_default_category_policy(id2, &ProtectionLevel::Editable)
            .unwrap();

        let with_policies = db.list_default_categories_with_policies().unwrap();
        assert_eq!(with_policies.len(), 2);
        assert_eq!(with_policies[0].0.pattern, "evidence/**");
        assert_eq!(with_policies[0].1, ProtectionLevel::Immutable);
        assert_eq!(with_policies[1].0.pattern, "notes/**");
        assert_eq!(with_policies[1].1, ProtectionLevel::Editable);
    }

    #[test]
    fn list_with_policies_no_policy_defaults_editable() {
        let (_dir, db) = setup();
        db.insert_default_category(&Category {
            id: None,
            pattern: "stuff/**".to_string(),
            category_type: CategoryType::Files,
            description: None,
        })
        .unwrap();

        let with_policies = db.list_default_categories_with_policies().unwrap();
        assert_eq!(with_policies.len(), 1);
        assert_eq!(with_policies[0].1, ProtectionLevel::Editable);
    }

    #[test]
    fn migrate_adds_category_type_column() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join(".mksp");

        let old_categories = "
            CREATE TABLE IF NOT EXISTS default_categories (
                id INTEGER PRIMARY KEY,
                pattern TEXT NOT NULL UNIQUE,
                protection_level TEXT NOT NULL,
                description TEXT
            );
        ";
        let old_schema = format!("{OLD_SCHEMA_BASE}{old_categories}");
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
            .unwrap();
        conn.execute_batch(&old_schema).unwrap();
        conn.execute(
            "INSERT INTO default_categories (pattern, protection_level, description) VALUES (?1, ?2, ?3)",
            rusqlite::params!["evidence/**", "immutable", "Evidence"],
        )
        .unwrap();
        drop(conn);

        let db = WorkspaceDb::open(&db_path).unwrap();
        let cats = db.list_default_categories().unwrap();
        assert_eq!(cats.len(), 1);
        assert_eq!(cats[0].category_type, CategoryType::Files);
    }

    #[test]
    fn default_policy_migration() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join(".mksp");

        let old_categories = "
            CREATE TABLE IF NOT EXISTS default_categories (
                id INTEGER PRIMARY KEY,
                pattern TEXT NOT NULL UNIQUE,
                category_type TEXT NOT NULL DEFAULT 'files',
                protection_level TEXT NOT NULL,
                description TEXT
            );
        ";
        let old_schema = format!("{OLD_SCHEMA_BASE}{old_categories}");
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
            .unwrap();
        conn.execute_batch(&old_schema).unwrap();
        conn.execute(
            "INSERT INTO default_categories (pattern, category_type, protection_level, description) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params!["evidence/**", "files", "immutable", "Evidence"],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO default_categories (pattern, category_type, protection_level, description) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params!["notes/**", "files", "protected", "Notes"],
        )
        .unwrap();
        drop(conn);

        let db = WorkspaceDb::open(&db_path).unwrap();
        let cats = db.list_default_categories().unwrap();
        assert_eq!(cats.len(), 2);

        let ev_id = cats
            .iter()
            .find(|c| c.pattern == "evidence/**")
            .unwrap()
            .id
            .unwrap();
        let notes_id = cats
            .iter()
            .find(|c| c.pattern == "notes/**")
            .unwrap()
            .id
            .unwrap();

        assert_eq!(
            db.get_default_category_policy(ev_id).unwrap(),
            Some(ProtectionLevel::Immutable)
        );
        assert_eq!(
            db.get_default_category_policy(notes_id).unwrap(),
            Some(ProtectionLevel::Protected)
        );
    }
}
