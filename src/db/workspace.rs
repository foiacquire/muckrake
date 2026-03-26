use std::path::Path;

use anyhow::{Context, Result};
use chrono::Utc;
use rusqlite::Connection;
use sea_query::{
    Asterisk, CaseStatement, Cond, Expr, ExprTrait, Func, OnConflict, Order, Query,
    SqliteQueryBuilder,
};
use sea_query_rusqlite::RusqliteBinder;

use crate::models::{Category, ProtectionLevel, Scope};
use crate::reference::validate_name;

use super::iden::{ScopePolicy, ScopeToolConfig, Scopes, WorkspaceConfig};
use super::schema::WORKSPACE_SCHEMA;

pub struct WorkspaceDb {
    conn: Connection,
}

#[derive(Debug, Clone)]
pub struct ProjectRow {
    pub id: i64,
    pub name: String,
    pub path: String,
    pub description: Option<String>,
    pub created_at: String,
}

impl WorkspaceDb {
    pub fn create(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)
            .with_context(|| format!("failed to create workspace db at {}", path.display()))?;
        super::configure_conn(&conn)
            .with_context(|| format!("failed to configure workspace db at {}", path.display()))?;
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
        super::configure_conn(&conn)
            .with_context(|| format!("failed to configure workspace db at {}", path.display()))?;
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
            .into_table(Scopes::Table)
            .columns([
                Scopes::Name,
                Scopes::ScopeType,
                Scopes::Pattern,
                Scopes::Description,
                Scopes::CreatedAt,
            ])
            .values_panic([
                name.into(),
                "project".into(),
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
            .columns(SCOPE_COLUMNS)
            .from(Scopes::Table)
            .and_where(Expr::col(Scopes::ScopeType).eq("project"))
            .order_by(Scopes::Name, Order::Asc)
            .build_rusqlite(SqliteQueryBuilder);
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(&*values.as_params(), row_to_project)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn get_project_by_name(&self, name: &str) -> Result<Option<ProjectRow>> {
        let (sql, values) = Query::select()
            .columns(SCOPE_COLUMNS)
            .from(Scopes::Table)
            .and_where(Expr::col(Scopes::ScopeType).eq("project"))
            .and_where(Expr::col(Scopes::Name).eq(name))
            .build_rusqlite(SqliteQueryBuilder);
        let mut stmt = self.conn.prepare(&sql)?;
        let mut rows = stmt.query_map(&*values.as_params(), row_to_project)?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    pub fn insert_default_category(&self, cat: &Category) -> Result<i64> {
        let scope: Scope = cat.into();
        let (sql, values) = Query::insert()
            .into_table(Scopes::Table)
            .columns([
                Scopes::Name,
                Scopes::ScopeType,
                Scopes::Pattern,
                Scopes::CategoryType,
                Scopes::Description,
            ])
            .values_panic([
                scope.name.as_str().into(),
                "category".into(),
                scope.pattern.clone().into(),
                scope
                    .category_type
                    .map(|ct| ct.to_string())
                    .unwrap_or_default()
                    .into(),
                scope.description.clone().into(),
            ])
            .build_rusqlite(SqliteQueryBuilder);
        self.conn.execute(&sql, &*values.as_params())?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn list_default_categories(&self) -> Result<Vec<Category>> {
        let (sql, values) = Query::select()
            .columns(SCOPE_COLUMNS)
            .from(Scopes::Table)
            .and_where(Expr::col(Scopes::ScopeType).eq("category"))
            .build_rusqlite(SqliteQueryBuilder);
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(&*values.as_params(), |row| {
            let scope = row_to_scope(row)?;
            Ok(Category::from(scope))
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn insert_default_category_policy(
        &self,
        default_category_id: i64,
        level: &ProtectionLevel,
    ) -> Result<()> {
        let (sql, values) = Query::insert()
            .into_table(ScopePolicy::Table)
            .columns([ScopePolicy::ScopeId, ScopePolicy::ProtectionLevel])
            .values_panic([default_category_id.into(), level.to_string().into()])
            .on_conflict(
                OnConflict::column(ScopePolicy::ScopeId)
                    .update_column(ScopePolicy::ProtectionLevel)
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
            .column(ScopePolicy::ProtectionLevel)
            .from(ScopePolicy::Table)
            .and_where(Expr::col(ScopePolicy::ScopeId).eq(default_category_id))
            .build_rusqlite(SqliteQueryBuilder);
        let mut stmt = self.conn.prepare(&sql)?;
        let mut rows = stmt.query_map(&*values.as_params(), |row| row.get::<_, String>(0))?;
        match rows.next() {
            Some(row) => {
                let s = row?;
                let level = s.parse().map_err(|e| {
                    anyhow::anyhow!("invalid protection level '{s}' in scope_policy: {e}")
                })?;
                Ok(Some(level))
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
            let level = match cat.id {
                Some(id) => self
                    .get_default_category_policy(id)?
                    .unwrap_or(ProtectionLevel::Editable),
                None => ProtectionLevel::Editable,
            };
            result.push((cat, level));
        }
        Ok(result)
    }

    pub fn project_count(&self) -> Result<i64> {
        let (sql, values) = Query::select()
            .expr(Func::count(Expr::col(Asterisk)))
            .from(Scopes::Table)
            .and_where(Expr::col(Scopes::ScopeType).eq("project"))
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
            .columns(SCOPE_TOOL_CONFIG_COLUMNS)
            .from(ScopeToolConfig::Table)
            .cond_where(
                Cond::all()
                    .add(
                        Cond::any()
                            .add(
                                Expr::col(ScopeToolConfig::ScopeId).in_subquery(
                                    Query::select()
                                        .column(Scopes::Id)
                                        .from(Scopes::Table)
                                        .and_where(
                                            Expr::col(Scopes::Name).eq(scope.map(String::from)),
                                        )
                                        .to_owned(),
                                ),
                            )
                            .add(Expr::col(ScopeToolConfig::ScopeId).is_null()),
                    )
                    .add(Expr::col(ScopeToolConfig::Action).eq(action))
                    .add(
                        Cond::any()
                            .add(Expr::col(ScopeToolConfig::FileType).eq(file_type))
                            .add(Expr::col(ScopeToolConfig::FileType).eq("*")),
                    ),
            )
            .order_by_expr(
                CaseStatement::new()
                    .case(Expr::col(ScopeToolConfig::ScopeId).is_not_null(), 0)
                    .finally(1)
                    .into(),
                Order::Asc,
            )
            .order_by_expr(
                CaseStatement::new()
                    .case(Expr::col(ScopeToolConfig::FileType).eq("*"), 1)
                    .finally(0)
                    .into(),
                Order::Asc,
            )
            .limit(1)
            .build_rusqlite(SqliteQueryBuilder);
        let mut stmt = self.conn.prepare(&sql)?;
        let mut rows = stmt.query_map(&*values.as_params(), row_to_tool_config)?;
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
            .columns(SCOPE_TOOL_CONFIG_COLUMNS)
            .from(ScopeToolConfig::Table)
            .and_where(
                Expr::col(ScopeToolConfig::ScopeId).in_subquery(
                    Query::select()
                        .column(Scopes::Id)
                        .from(Scopes::Table)
                        .and_where(Expr::col(Scopes::ScopeType).eq("tag"))
                        .and_where(Expr::col(Scopes::Name).is_in(tag_values))
                        .to_owned(),
                ),
            )
            .and_where(Expr::col(ScopeToolConfig::Action).eq(action))
            .cond_where(
                Cond::any()
                    .add(Expr::col(ScopeToolConfig::FileType).eq(file_type))
                    .add(Expr::col(ScopeToolConfig::FileType).eq("*")),
            )
            .build_rusqlite(SqliteQueryBuilder);
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(&*values.as_params(), row_to_tag_tool_config)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn insert_tool_config(&self, params: &super::project::ToolConfigParams<'_>) -> Result<i64> {
        let scope_id: Option<i64> = if let Some(scope_name) = params.scope {
            self.get_scope_id_by_name(scope_name)?
        } else {
            None
        };
        let (sql, values) = Query::insert()
            .into_table(ScopeToolConfig::Table)
            .columns([
                ScopeToolConfig::ScopeId,
                ScopeToolConfig::Action,
                ScopeToolConfig::FileType,
                ScopeToolConfig::Command,
                ScopeToolConfig::Env,
                ScopeToolConfig::Quiet,
            ])
            .values_panic([
                scope_id.into(),
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
        let scope_id = self.get_or_create_tag_scope(params.tag)?;
        let (sql, values) = Query::insert()
            .into_table(ScopeToolConfig::Table)
            .columns([
                ScopeToolConfig::ScopeId,
                ScopeToolConfig::Action,
                ScopeToolConfig::FileType,
                ScopeToolConfig::Command,
                ScopeToolConfig::Env,
                ScopeToolConfig::Quiet,
            ])
            .values_panic([
                scope_id.into(),
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
            .columns(SCOPE_TOOL_CONFIG_COLUMNS)
            .from(ScopeToolConfig::Table)
            .order_by(ScopeToolConfig::Action, Order::Asc)
            .build_rusqlite(SqliteQueryBuilder);
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(&*values.as_params(), row_to_tool_config)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn list_tag_tool_configs(&self) -> Result<Vec<super::project::TagToolConfigRow>> {
        let (sql, values) = Query::select()
            .columns(SCOPE_TOOL_CONFIG_COLUMNS)
            .from(ScopeToolConfig::Table)
            .and_where(
                Expr::col(ScopeToolConfig::ScopeId).in_subquery(
                    Query::select()
                        .column(Scopes::Id)
                        .from(Scopes::Table)
                        .and_where(Expr::col(Scopes::ScopeType).eq("tag"))
                        .to_owned(),
                ),
            )
            .build_rusqlite(SqliteQueryBuilder);
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(&*values.as_params(), row_to_tag_tool_config)?;
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
            .from_table(ScopeToolConfig::Table)
            .and_where(Expr::col(ScopeToolConfig::Action).eq(action));
        match scope {
            Some(s) => {
                query.and_where(
                    Expr::col(ScopeToolConfig::ScopeId).in_subquery(
                        Query::select()
                            .column(Scopes::Id)
                            .from(Scopes::Table)
                            .and_where(Expr::col(Scopes::Name).eq(s))
                            .to_owned(),
                    ),
                );
            }
            None => {
                query.and_where(Expr::col(ScopeToolConfig::ScopeId).is_null());
            }
        }
        if let Some(ft) = file_type {
            query.and_where(Expr::col(ScopeToolConfig::FileType).eq(ft));
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
            .from_table(ScopeToolConfig::Table)
            .and_where(Expr::col(ScopeToolConfig::Action).eq(action))
            .and_where(
                Expr::col(ScopeToolConfig::ScopeId).in_subquery(
                    Query::select()
                        .column(Scopes::Id)
                        .from(Scopes::Table)
                        .and_where(Expr::col(Scopes::Name).eq(tag))
                        .and_where(Expr::col(Scopes::ScopeType).eq("tag"))
                        .to_owned(),
                ),
            );
        if let Some(ft) = file_type {
            query.and_where(Expr::col(ScopeToolConfig::FileType).eq(ft));
        }
        let (sql, values) = query.build_rusqlite(SqliteQueryBuilder);
        let count = self.conn.execute(&sql, &*values.as_params())?;
        Ok(count as u64)
    }

    fn get_scope_id_by_name(&self, name: &str) -> Result<Option<i64>> {
        let (sql, values) = Query::select()
            .column(Scopes::Id)
            .from(Scopes::Table)
            .and_where(Expr::col(Scopes::Name).eq(name))
            .build_rusqlite(SqliteQueryBuilder);
        let mut stmt = self.conn.prepare(&sql)?;
        let mut rows = stmt.query_map(&*values.as_params(), |row| row.get::<_, i64>(0))?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    fn get_or_create_tag_scope(&self, tag: &str) -> Result<Option<i64>> {
        if let Some(id) = self.get_scope_id_by_name(tag)? {
            return Ok(Some(id));
        }
        let (sql, values) = Query::insert()
            .into_table(Scopes::Table)
            .columns([Scopes::Name, Scopes::ScopeType])
            .values_panic([tag.into(), "tag".into()])
            .build_rusqlite(SqliteQueryBuilder);
        self.conn.execute(&sql, &*values.as_params())?;
        Ok(Some(self.conn.last_insert_rowid()))
    }
}

fn migrate(conn: &Connection) -> Result<()> {
    let has_legacy = conn
        .prepare("SELECT id FROM default_categories LIMIT 0")
        .is_ok();
    if has_legacy {
        migrate_default_category_type(conn);
        migrate_default_category_policy_table(conn)?;
        migrate_default_category_name(conn)?;
        migrate_tool_config_quiet(conn);
        migrate_tag_tool_config_quiet(conn);
    }
    super::pipeline::migrate_default_pipelines_table(conn)?;
    Ok(())
}

fn migrate_add_column(conn: &Connection, table: &str, column: &str, definition: &str) -> bool {
    let check = format!("SELECT {column} FROM {table} LIMIT 0");
    if conn.prepare(&check).is_ok() {
        return false;
    }
    let alter = format!("ALTER TABLE {table} ADD COLUMN {column} {definition};");
    conn.execute_batch(&alter).is_ok()
}

fn migrate_default_category_type(conn: &Connection) {
    migrate_add_column(
        conn,
        "default_categories",
        "category_type",
        "TEXT NOT NULL DEFAULT 'files'",
    );
}

fn migrate_tool_config_quiet(conn: &Connection) {
    migrate_add_column(conn, "tool_config", "quiet", "INTEGER NOT NULL DEFAULT 1");
}

fn migrate_tag_tool_config_quiet(conn: &Connection) {
    migrate_add_column(
        conn,
        "tag_tool_config",
        "quiet",
        "INTEGER NOT NULL DEFAULT 1",
    );
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
    let has_old_col = conn
        .prepare("SELECT protection_level FROM default_categories LIMIT 0")
        .is_ok();
    if has_old_col {
        conn.execute_batch(
            "INSERT OR IGNORE INTO default_category_policy (default_category_id, protection_level)
             SELECT id, protection_level FROM default_categories WHERE protection_level IS NOT NULL;",
        )?;
    }
    Ok(())
}

fn migrate_default_category_name(conn: &Connection) -> Result<()> {
    if !migrate_add_column(
        conn,
        "default_categories",
        "name",
        "TEXT NOT NULL DEFAULT ''",
    ) {
        return Ok(());
    }
    let mut stmt = conn.prepare("SELECT id, pattern FROM default_categories WHERE name = ''")?;
    let rows: Vec<(i64, String)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .filter_map(Result::ok)
        .collect();
    for (id, pattern) in rows {
        let name = crate::models::Scope::name_from_pattern(&pattern);
        conn.execute(
            "UPDATE default_categories SET name = ?1 WHERE id = ?2",
            rusqlite::params![name, id],
        )?;
    }
    Ok(())
}

const SCOPE_COLUMNS: [Scopes; 7] = [
    Scopes::Id,
    Scopes::Name,
    Scopes::ScopeType,
    Scopes::Pattern,
    Scopes::CategoryType,
    Scopes::Description,
    Scopes::CreatedAt,
];

fn row_to_scope(row: &rusqlite::Row) -> rusqlite::Result<Scope> {
    let scope_type: String = row.get(2)?;
    let cat_type_str: Option<String> = row.get(4)?;
    Ok(Scope {
        id: Some(row.get(0)?),
        name: row.get(1)?,
        scope_type: scope_type.parse().map_err(|e| sql_convert_err(2, e))?,
        pattern: row.get(3)?,
        category_type: cat_type_str
            .and_then(|s| if s.is_empty() { None } else { Some(s) })
            .map(|s| s.parse())
            .transpose()
            .map_err(|e| sql_convert_err(4, e))?,
        description: row.get(5)?,
        created_at: row.get(6)?,
    })
}

fn row_to_project(row: &rusqlite::Row) -> rusqlite::Result<ProjectRow> {
    Ok(ProjectRow {
        id: row.get(0)?,
        name: row.get(1)?,
        path: row.get::<_, Option<String>>(3)?.unwrap_or_default(),
        description: row.get(5)?,
        created_at: row.get::<_, Option<String>>(6)?.unwrap_or_default(),
    })
}

const SCOPE_TOOL_CONFIG_COLUMNS: [ScopeToolConfig; 7] = [
    ScopeToolConfig::Id,
    ScopeToolConfig::ScopeId,
    ScopeToolConfig::Action,
    ScopeToolConfig::FileType,
    ScopeToolConfig::Command,
    ScopeToolConfig::Env,
    ScopeToolConfig::Quiet,
];

fn row_to_tool_config(row: &rusqlite::Row) -> rusqlite::Result<super::project::ToolConfigRow> {
    Ok(super::project::ToolConfigRow {
        id: row.get(0)?,
        scope: row.get::<_, Option<i64>>(1)?.map(|_id| String::new()),
        action: row.get(2)?,
        file_type: row.get(3)?,
        command: row.get(4)?,
        env: row.get(5)?,
        quiet: row.get::<_, i32>(6)? != 0,
    })
}

fn row_to_tag_tool_config(
    row: &rusqlite::Row,
) -> rusqlite::Result<super::project::TagToolConfigRow> {
    Ok(super::project::TagToolConfigRow {
        id: row.get(0)?,
        tag: row
            .get::<_, Option<i64>>(1)?
            .map_or_else(String::new, |id| id.to_string()),
        action: row.get(2)?,
        file_type: row.get(3)?,
        command: row.get(4)?,
        env: row.get(5)?,
        quiet: row.get::<_, i32>(6)? != 0,
    })
}

fn sql_convert_err(col: usize, e: impl std::fmt::Display) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        col,
        rusqlite::types::Type::Text,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("{e}"),
        )),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::CategoryType;
    use tempfile::TempDir;

    fn setup() -> (TempDir, WorkspaceDb) {
        let dir = TempDir::new().unwrap();
        let db = WorkspaceDb::create(&dir.path().join(".mksp")).unwrap();
        (dir, db)
    }

    #[test]
    fn register_and_list_projects() {
        let (_dir, db) = setup();
        db.register_project("alpha", "projects/alpha", None)
            .unwrap();
        db.register_project("beta", "projects/beta", Some("Beta project"))
            .unwrap();

        let projects = db.list_projects().unwrap();
        assert_eq!(projects.len(), 2);
        assert_eq!(projects[0].name, "alpha");
        assert_eq!(projects[0].path, "projects/alpha");
        assert_eq!(projects[1].name, "beta");
        assert_eq!(projects[1].description, Some("Beta project".to_string()));
    }

    #[test]
    fn get_project_by_name() {
        let (_dir, db) = setup();
        db.register_project("alpha", "projects/alpha", None)
            .unwrap();

        let found = db.get_project_by_name("alpha").unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().path, "projects/alpha");

        assert!(db.get_project_by_name("nonexistent").unwrap().is_none());
    }

    #[test]
    fn insert_default_categories() {
        let (_dir, db) = setup();
        let cat = Category {
            id: None,
            name: "evidence".to_string(),
            pattern: "evidence/**".to_string(),
            category_type: CategoryType::Files,
            description: None,
        };
        let id = db.insert_default_category(&cat).unwrap();
        db.insert_default_category_policy(id, &ProtectionLevel::Immutable)
            .unwrap();

        let cats = db.list_default_categories().unwrap();
        assert_eq!(cats.len(), 1);
        assert_eq!(cats[0].name, "evidence");
        assert_eq!(cats[0].pattern, "evidence/**");

        let policy = db.get_default_category_policy(id).unwrap();
        assert_eq!(policy, Some(ProtectionLevel::Immutable));
    }

    #[test]
    fn list_default_categories_with_policies() {
        let (_dir, db) = setup();
        let cat = Category {
            id: None,
            name: "evidence".to_string(),
            pattern: "evidence/**".to_string(),
            category_type: CategoryType::Files,
            description: None,
        };
        let id = db.insert_default_category(&cat).unwrap();
        db.insert_default_category_policy(id, &ProtectionLevel::Protected)
            .unwrap();

        let result = db.list_default_categories_with_policies().unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0.name, "evidence");
        assert_eq!(result[0].1, ProtectionLevel::Protected);
    }

    #[test]
    fn list_with_policies_no_policy_defaults_editable() {
        let (_dir, db) = setup();
        let cat = Category {
            id: None,
            name: "notes".to_string(),
            pattern: "notes/**".to_string(),
            category_type: CategoryType::Files,
            description: None,
        };
        db.insert_default_category(&cat).unwrap();

        let result = db.list_default_categories_with_policies().unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].1, ProtectionLevel::Editable);
    }

    #[test]
    fn project_count() {
        let (_dir, db) = setup();
        assert_eq!(db.project_count().unwrap(), 0);
        db.register_project("alpha", "projects/alpha", None)
            .unwrap();
        assert_eq!(db.project_count().unwrap(), 1);
    }

    #[test]
    fn config_get_set() {
        let (_dir, db) = setup();
        assert!(db.get_config("projects_dir").unwrap().is_none());
        db.set_config("projects_dir", "projects/").unwrap();
        assert_eq!(
            db.get_config("projects_dir").unwrap(),
            Some("projects/".to_string())
        );
    }
}
