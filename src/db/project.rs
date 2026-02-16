use std::path::Path;

use anyhow::{Context, Result};
use chrono::Utc;
use rusqlite::Connection;
use sea_query::{
    Asterisk, CaseStatement, Cond, Expr, ExprTrait, Func, OnConflict, Order, Query,
    SqliteQueryBuilder,
};
use sea_query_rusqlite::RusqliteBinder;

use crate::models::policy::strictest;
use crate::models::{
    ActionConfig, ActionType, Category, FileTag, ProtectionLevel, Rule, TrackedFile, TriggerEvent,
    TriggerFilter,
};

use super::iden::{
    AuditLog, Categories, CategoryPolicy, FileTags, Files, Rules, TagToolConfig, ToolConfig,
};
use super::schema::PROJECT_SCHEMA;

pub struct ProjectDb {
    conn: Connection,
}

impl ProjectDb {
    pub fn create(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)
            .with_context(|| format!("failed to create project db at {}", path.display()))?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
            .with_context(|| format!("failed to configure project db at {}", path.display()))?;
        conn.execute_batch(&PROJECT_SCHEMA)?;
        migrate(&conn)?;
        Ok(Self { conn })
    }

    pub fn open(path: &Path) -> Result<Self> {
        if !path.exists() {
            anyhow::bail!("project database not found: {}", path.display());
        }
        let conn = Connection::open(path)
            .with_context(|| format!("failed to open project db at {}", path.display()))?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
            .with_context(|| format!("failed to configure project db at {}", path.display()))?;
        migrate(&conn)?;
        Ok(Self { conn })
    }

    pub fn insert_category(&self, cat: &Category) -> Result<i64> {
        let (sql, values) = Query::insert()
            .into_table(Categories::Table)
            .columns([
                Categories::Name,
                Categories::Pattern,
                Categories::CategoryType,
                Categories::Description,
            ])
            .values_panic([
                cat.name.as_str().into(),
                cat.pattern.as_str().into(),
                cat.category_type.to_string().into(),
                cat.description.clone().into(),
            ])
            .build_rusqlite(SqliteQueryBuilder);
        self.conn.execute(&sql, &*values.as_params())?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn list_categories(&self) -> Result<Vec<Category>> {
        let (sql, values) = Query::select()
            .columns(CATEGORY_COLUMNS)
            .from(Categories::Table)
            .build_rusqlite(SqliteQueryBuilder);
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(&*values.as_params(), row_to_category)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn get_category_by_pattern(&self, pattern: &str) -> Result<Option<Category>> {
        let (sql, values) = Query::select()
            .columns(CATEGORY_COLUMNS)
            .from(Categories::Table)
            .and_where(Expr::col(Categories::Pattern).eq(pattern))
            .build_rusqlite(SqliteQueryBuilder);
        let mut stmt = self.conn.prepare(&sql)?;
        let mut rows = stmt.query_map(&*values.as_params(), row_to_category)?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    pub fn get_category_by_name(&self, name: &str) -> Result<Option<Category>> {
        let (sql, values) = Query::select()
            .columns(CATEGORY_COLUMNS)
            .from(Categories::Table)
            .and_where(Expr::col(Categories::Name).eq(name))
            .build_rusqlite(SqliteQueryBuilder);
        let mut stmt = self.conn.prepare(&sql)?;
        let mut rows = stmt.query_map(&*values.as_params(), row_to_category)?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    pub fn update_category_pattern(&self, category_id: i64, new_pattern: &str) -> Result<()> {
        let (sql, values) = Query::update()
            .table(Categories::Table)
            .value(Categories::Pattern, new_pattern)
            .and_where(Expr::col(Categories::Id).eq(category_id))
            .build_rusqlite(SqliteQueryBuilder);
        self.conn.execute(&sql, &*values.as_params())?;
        Ok(())
    }

    pub fn remove_category(&self, category_id: i64) -> Result<()> {
        let (sql, values) = Query::delete()
            .from_table(CategoryPolicy::Table)
            .and_where(Expr::col(CategoryPolicy::CategoryId).eq(category_id))
            .build_rusqlite(SqliteQueryBuilder);
        self.conn.execute(&sql, &*values.as_params())?;

        let (sql, values) = Query::delete()
            .from_table(Categories::Table)
            .and_where(Expr::col(Categories::Id).eq(category_id))
            .build_rusqlite(SqliteQueryBuilder);
        self.conn.execute(&sql, &*values.as_params())?;
        Ok(())
    }

    pub fn match_category(&self, rel_path: &str) -> Result<Option<Category>> {
        let categories = self.list_categories()?;
        let mut best: Option<&Category> = None;
        let mut best_specificity = 0usize;

        for cat in &categories {
            if cat.matches(rel_path)? {
                let specificity = cat.pattern.len();
                if specificity > best_specificity {
                    best = Some(cat);
                    best_specificity = specificity;
                }
            }
        }

        Ok(best.cloned())
    }

    pub fn insert_category_policy(&self, category_id: i64, level: &ProtectionLevel) -> Result<()> {
        let (sql, values) = Query::insert()
            .into_table(CategoryPolicy::Table)
            .columns([CategoryPolicy::CategoryId, CategoryPolicy::ProtectionLevel])
            .values_panic([category_id.into(), level.to_string().into()])
            .on_conflict(
                OnConflict::column(CategoryPolicy::CategoryId)
                    .update_column(CategoryPolicy::ProtectionLevel)
                    .to_owned(),
            )
            .build_rusqlite(SqliteQueryBuilder);
        self.conn.execute(&sql, &*values.as_params())?;
        Ok(())
    }

    pub fn get_policy_for_category(&self, category_id: i64) -> Result<Option<ProtectionLevel>> {
        let (sql, values) = Query::select()
            .column(CategoryPolicy::ProtectionLevel)
            .from(CategoryPolicy::Table)
            .and_where(Expr::col(CategoryPolicy::CategoryId).eq(category_id))
            .build_rusqlite(SqliteQueryBuilder);
        let mut stmt = self.conn.prepare(&sql)?;
        let mut rows = stmt.query_map(&*values.as_params(), |row| row.get::<_, String>(0))?;
        match rows.next() {
            Some(row) => {
                let s = row?;
                let level = s.parse().map_err(|e| {
                    anyhow::anyhow!("invalid protection level '{s}' in category_policy: {e}")
                })?;
                Ok(Some(level))
            }
            None => Ok(None),
        }
    }

    pub fn resolve_protection(&self, rel_path: &str) -> Result<ProtectionLevel> {
        let categories = self.list_categories()?;
        let mut levels = Vec::new();
        for cat in &categories {
            if cat.matches(rel_path)? {
                if let Some(id) = cat.id {
                    if let Some(level) = self.get_policy_for_category(id)? {
                        levels.push(level);
                    }
                }
            }
        }
        Ok(strictest(&levels))
    }

    pub fn insert_file(&self, file: &TrackedFile) -> Result<i64> {
        let (sql, values) = Query::insert()
            .into_table(Files::Table)
            .columns([
                Files::Name,
                Files::Path,
                Files::Sha256,
                Files::MimeType,
                Files::Size,
                Files::IngestedAt,
                Files::Provenance,
                Files::Immutable,
            ])
            .values_panic([
                file.name.as_str().into(),
                file.path.as_str().into(),
                file.sha256.clone().into(),
                file.mime_type.clone().into(),
                file.size.into(),
                file.ingested_at.as_str().into(),
                file.provenance.clone().into(),
                i32::from(file.immutable).into(),
            ])
            .build_rusqlite(SqliteQueryBuilder);
        self.conn.execute(&sql, &*values.as_params())?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn get_file_by_name(&self, name: &str) -> Result<Option<TrackedFile>> {
        let (sql, values) = Query::select()
            .columns(FILE_COLUMNS)
            .from(Files::Table)
            .and_where(Expr::col(Files::Name).eq(name))
            .build_rusqlite(SqliteQueryBuilder);
        let mut stmt = self.conn.prepare(&sql)?;
        let mut rows = stmt.query_map(&*values.as_params(), row_to_file)?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    pub fn get_file_by_path(&self, path: &str) -> Result<Option<TrackedFile>> {
        let (sql, values) = Query::select()
            .columns(FILE_COLUMNS)
            .from(Files::Table)
            .and_where(Expr::col(Files::Path).eq(path))
            .build_rusqlite(SqliteQueryBuilder);
        let mut stmt = self.conn.prepare(&sql)?;
        let mut rows = stmt.query_map(&*values.as_params(), row_to_file)?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    pub fn list_files(&self, path_prefix: Option<&str>) -> Result<Vec<TrackedFile>> {
        let (sql, values) = Query::select()
            .columns(FILE_COLUMNS)
            .from(Files::Table)
            .apply_if(path_prefix, |q, prefix| {
                q.and_where(Expr::col(Files::Path).like(format!("{prefix}%")));
            })
            .order_by(Files::Path, Order::Asc)
            .build_rusqlite(SqliteQueryBuilder);
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(&*values.as_params(), row_to_file)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn list_files_by_tag(&self, tag: &str) -> Result<Vec<TrackedFile>> {
        let (sql, values) = Query::select()
            .columns([
                (Files::Table, Files::Id),
                (Files::Table, Files::Name),
                (Files::Table, Files::Path),
                (Files::Table, Files::Sha256),
                (Files::Table, Files::MimeType),
                (Files::Table, Files::Size),
                (Files::Table, Files::IngestedAt),
                (Files::Table, Files::Provenance),
                (Files::Table, Files::Immutable),
            ])
            .from(Files::Table)
            .inner_join(
                FileTags::Table,
                Expr::col((Files::Table, Files::Id)).equals((FileTags::Table, FileTags::FileId)),
            )
            .and_where(Expr::col((FileTags::Table, FileTags::Tag)).eq(tag))
            .order_by((Files::Table, Files::Path), Order::Asc)
            .build_rusqlite(SqliteQueryBuilder);
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(&*values.as_params(), row_to_file)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn list_files_filtered(
        &self,
        path_prefix: Option<&str>,
        tag_groups: &[&[&str]],
    ) -> Result<Vec<TrackedFile>> {
        if tag_groups.is_empty() {
            return self.list_files(path_prefix);
        }

        let mut candidates = self.list_files(path_prefix)?;

        for group in tag_groups {
            if group.is_empty() {
                continue;
            }
            let tag_values: Vec<sea_query::Value> = group.iter().map(|t| (*t).into()).collect();
            let (sql, values) = Query::select()
                .column((FileTags::Table, FileTags::FileId))
                .from(FileTags::Table)
                .and_where(Expr::col((FileTags::Table, FileTags::Tag)).is_in(tag_values))
                .build_rusqlite(SqliteQueryBuilder);
            let mut stmt = self.conn.prepare(&sql)?;
            let matching_ids: std::collections::HashSet<i64> = stmt
                .query_map(&*values.as_params(), |row| row.get::<_, i64>(0))?
                .collect::<Result<_, _>>()?;
            candidates.retain(|f| f.id.is_some_and(|id| matching_ids.contains(&id)));
        }

        Ok(candidates)
    }

    pub fn update_file_path(&self, file_id: i64, new_path: &str) -> Result<()> {
        let (sql, values) = Query::update()
            .table(Files::Table)
            .value(Files::Path, new_path)
            .and_where(Expr::col(Files::Id).eq(file_id))
            .build_rusqlite(SqliteQueryBuilder);
        self.conn.execute(&sql, &*values.as_params())?;
        Ok(())
    }

    pub fn update_file_immutable(&self, file_id: i64, immutable: bool) -> Result<()> {
        let (sql, values) = Query::update()
            .table(Files::Table)
            .value(Files::Immutable, i32::from(immutable))
            .and_where(Expr::col(Files::Id).eq(file_id))
            .build_rusqlite(SqliteQueryBuilder);
        self.conn.execute(&sql, &*values.as_params())?;
        Ok(())
    }

    pub fn update_file_sha256(&self, file_id: i64, sha256: &str) -> Result<()> {
        let (sql, values) = Query::update()
            .table(Files::Table)
            .value(Files::Sha256, sha256)
            .and_where(Expr::col(Files::Id).eq(file_id))
            .build_rusqlite(SqliteQueryBuilder);
        self.conn.execute(&sql, &*values.as_params())?;
        Ok(())
    }

    pub fn insert_tag(&self, file_id: i64, tag: &str, file_hash: &str) -> Result<()> {
        let (sql, values) = Query::insert()
            .into_table(FileTags::Table)
            .columns([FileTags::FileId, FileTags::Tag, FileTags::FileHash])
            .values_panic([file_id.into(), tag.into(), file_hash.into()])
            .on_conflict(
                OnConflict::columns([FileTags::FileId, FileTags::Tag])
                    .update_column(FileTags::FileHash)
                    .to_owned(),
            )
            .build_rusqlite(SqliteQueryBuilder);
        self.conn.execute(&sql, &*values.as_params())?;
        Ok(())
    }

    pub fn remove_tag(&self, file_id: i64, tag: &str) -> Result<usize> {
        let (sql, values) = Query::delete()
            .from_table(FileTags::Table)
            .and_where(Expr::col(FileTags::FileId).eq(file_id))
            .and_where(Expr::col(FileTags::Tag).eq(tag))
            .build_rusqlite(SqliteQueryBuilder);
        let count = self.conn.execute(&sql, &*values.as_params())?;
        Ok(count)
    }

    pub fn get_tags(&self, file_id: i64) -> Result<Vec<String>> {
        let (sql, values) = Query::select()
            .column(FileTags::Tag)
            .from(FileTags::Table)
            .and_where(Expr::col(FileTags::FileId).eq(file_id))
            .order_by(FileTags::Tag, Order::Asc)
            .build_rusqlite(SqliteQueryBuilder);
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(&*values.as_params(), |row| row.get(0))?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn list_all_tags(&self) -> Result<Vec<FileTag>> {
        let (sql, values) = Query::select()
            .columns([FileTags::FileId, FileTags::Tag, FileTags::FileHash])
            .from(FileTags::Table)
            .order_by(FileTags::Tag, Order::Asc)
            .build_rusqlite(SqliteQueryBuilder);
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(&*values.as_params(), |row| {
            Ok(FileTag {
                file_id: row.get(0)?,
                tag: row.get(1)?,
                file_hash: row.get(2)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn get_file_tag_hash(&self, file_id: i64, tag: &str) -> Result<Option<String>> {
        let (sql, values) = Query::select()
            .column(FileTags::FileHash)
            .from(FileTags::Table)
            .and_where(Expr::col(FileTags::FileId).eq(file_id))
            .and_where(Expr::col(FileTags::Tag).eq(tag))
            .build_rusqlite(SqliteQueryBuilder);
        let mut stmt = self.conn.prepare(&sql)?;
        let mut rows =
            stmt.query_map(&*values.as_params(), |row| row.get::<_, Option<String>>(0))?;
        match rows.next() {
            Some(row) => Ok(row?),
            None => Ok(None),
        }
    }

    pub fn insert_audit(
        &self,
        operation: &str,
        file_id: Option<i64>,
        user: Option<&str>,
        detail: Option<&str>,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let (sql, values) = Query::insert()
            .into_table(AuditLog::Table)
            .columns([
                AuditLog::Timestamp,
                AuditLog::Operation,
                AuditLog::FileId,
                AuditLog::User,
                AuditLog::Detail,
            ])
            .values_panic([
                now.into(),
                operation.into(),
                file_id.into(),
                user.map(String::from).into(),
                detail.map(String::from).into(),
            ])
            .build_rusqlite(SqliteQueryBuilder);
        self.conn.execute(&sql, &*values.as_params())?;
        Ok(())
    }

    pub fn get_tool_config(
        &self,
        scope: Option<&str>,
        action: &str,
        file_type: &str,
    ) -> Result<Option<ToolConfigRow>> {
        let (sql, values) = Query::select()
            .columns(TOOL_CONFIG_COLUMNS)
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
    ) -> Result<Vec<TagToolConfigRow>> {
        if tags.is_empty() {
            return Ok(vec![]);
        }
        let tag_values: Vec<sea_query::Value> = tags.iter().map(|t| t.as_str().into()).collect();
        let (sql, values) = Query::select()
            .columns(TAG_TOOL_CONFIG_COLUMNS)
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
        let rows = stmt.query_map(&*values.as_params(), row_to_tag_tool_config)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn insert_tool_config(&self, params: &ToolConfigParams<'_>) -> Result<i64> {
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

    pub fn insert_tag_tool_config(&self, params: &TagToolConfigParams<'_>) -> Result<i64> {
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

    pub fn list_tool_configs(&self) -> Result<Vec<ToolConfigRow>> {
        let (sql, values) = Query::select()
            .columns(TOOL_CONFIG_COLUMNS)
            .from(ToolConfig::Table)
            .order_by(ToolConfig::Action, Order::Asc)
            .build_rusqlite(SqliteQueryBuilder);
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(&*values.as_params(), row_to_tool_config)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn list_tag_tool_configs(&self) -> Result<Vec<TagToolConfigRow>> {
        let (sql, values) = Query::select()
            .columns(TAG_TOOL_CONFIG_COLUMNS)
            .from(TagToolConfig::Table)
            .order_by(TagToolConfig::Tag, Order::Asc)
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

    pub fn file_count(&self) -> Result<i64> {
        let (sql, values) = Query::select()
            .expr(Func::count(Expr::col(Asterisk)))
            .from(Files::Table)
            .build_rusqlite(SqliteQueryBuilder);
        Ok(self
            .conn
            .query_row(&sql, &*values.as_params(), |row| row.get(0))?)
    }

    pub fn category_count(&self) -> Result<i64> {
        let (sql, values) = Query::select()
            .expr(Func::count(Expr::col(Asterisk)))
            .from(Categories::Table)
            .build_rusqlite(SqliteQueryBuilder);
        Ok(self
            .conn
            .query_row(&sql, &*values.as_params(), |row| row.get(0))?)
    }

    pub fn tag_count(&self) -> Result<i64> {
        let (sql, values) = Query::select()
            .expr(Func::count_distinct(Expr::col(FileTags::Tag)))
            .from(FileTags::Table)
            .build_rusqlite(SqliteQueryBuilder);
        Ok(self
            .conn
            .query_row(&sql, &*values.as_params(), |row| row.get(0))?)
    }

    pub fn last_verify_time(&self) -> Result<Option<String>> {
        let (sql, values) = Query::select()
            .column(AuditLog::Timestamp)
            .from(AuditLog::Table)
            .and_where(Expr::col(AuditLog::Operation).eq("verify"))
            .order_by(AuditLog::Timestamp, Order::Desc)
            .limit(1)
            .build_rusqlite(SqliteQueryBuilder);
        let mut stmt = self.conn.prepare(&sql)?;
        let mut rows = stmt.query_map(&*values.as_params(), |row| row.get(0))?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    // ── Rules ────────────────────────────────────────────────────────

    pub fn insert_rule(&self, rule: &Rule) -> Result<i64> {
        let filter_json = if rule.trigger_filter.is_empty() {
            None
        } else {
            Some(serde_json::to_string(&rule.trigger_filter)?)
        };
        let action_json = serde_json::to_string(&rule.action_config)?;

        let (sql, values) = Query::insert()
            .into_table(Rules::Table)
            .columns(RULE_COLUMNS_INSERT)
            .values_panic([
                rule.name.as_str().into(),
                i64::from(rule.enabled).into(),
                rule.trigger_event.to_string().into(),
                filter_json.into(),
                rule.action_type.to_string().into(),
                action_json.into(),
                i64::from(rule.priority).into(),
                rule.created_at.as_str().into(),
            ])
            .build_rusqlite(SqliteQueryBuilder);
        self.conn.execute(&sql, &*values.as_params())?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn list_rules(&self) -> Result<Vec<Rule>> {
        let (sql, values) = Query::select()
            .columns(RULE_COLUMNS)
            .from(Rules::Table)
            .order_by(Rules::Priority, Order::Asc)
            .order_by(Rules::Name, Order::Asc)
            .build_rusqlite(SqliteQueryBuilder);
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(&*values.as_params(), row_to_rule)?;
        rows.map(|r| Ok(r?)).collect()
    }

    pub fn get_rule_by_name(&self, name: &str) -> Result<Option<Rule>> {
        let (sql, values) = Query::select()
            .columns(RULE_COLUMNS)
            .from(Rules::Table)
            .and_where(Expr::col(Rules::Name).eq(name))
            .build_rusqlite(SqliteQueryBuilder);
        let mut stmt = self.conn.prepare(&sql)?;
        let mut rows = stmt.query_map(&*values.as_params(), row_to_rule)?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    pub fn remove_rule(&self, name: &str) -> Result<u64> {
        let (sql, values) = Query::delete()
            .from_table(Rules::Table)
            .and_where(Expr::col(Rules::Name).eq(name))
            .build_rusqlite(SqliteQueryBuilder);
        let count = self.conn.execute(&sql, &*values.as_params())?;
        Ok(count as u64)
    }

    pub fn set_rule_enabled(&self, name: &str, enabled: bool) -> Result<u64> {
        let (sql, values) = Query::update()
            .table(Rules::Table)
            .value(Rules::Enabled, i64::from(enabled))
            .and_where(Expr::col(Rules::Name).eq(name))
            .build_rusqlite(SqliteQueryBuilder);
        let count = self.conn.execute(&sql, &*values.as_params())?;
        Ok(count as u64)
    }

    pub fn get_matching_rules(&self, event: TriggerEvent) -> Result<Vec<Rule>> {
        let (sql, values) = Query::select()
            .columns(RULE_COLUMNS)
            .from(Rules::Table)
            .and_where(Expr::col(Rules::Enabled).eq(1_i64))
            .and_where(Expr::col(Rules::TriggerEvent).eq(event.to_string()))
            .order_by(Rules::Priority, Order::Asc)
            .order_by(Rules::Name, Order::Asc)
            .build_rusqlite(SqliteQueryBuilder);
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(&*values.as_params(), row_to_rule)?;
        rows.map(|r| Ok(r?)).collect()
    }
}

fn migrate(conn: &Connection) -> Result<()> {
    migrate_category_type(conn)?;
    migrate_tool_config_quiet(conn)?;
    migrate_tag_tool_config_quiet(conn)?;
    migrate_category_policy_table(conn)?;
    migrate_file_tags_hash(conn)?;
    migrate_category_name(conn)?;
    migrate_rules_table(conn)
}

fn migrate_category_type(conn: &Connection) -> Result<()> {
    let has_column = conn
        .prepare("SELECT category_type FROM categories LIMIT 0")
        .is_ok();
    if !has_column {
        conn.execute_batch(
            "ALTER TABLE categories ADD COLUMN category_type TEXT NOT NULL DEFAULT 'files';",
        )?;
    }
    Ok(())
}

fn migrate_tool_config_quiet(conn: &Connection) -> Result<()> {
    let table_exists = conn.prepare("SELECT id FROM tool_config LIMIT 0").is_ok();
    if !table_exists {
        return Ok(());
    }
    let has_quiet = conn
        .prepare("SELECT quiet FROM tool_config LIMIT 0")
        .is_ok();
    if !has_quiet {
        conn.execute_batch("ALTER TABLE tool_config ADD COLUMN quiet INTEGER NOT NULL DEFAULT 1;")?;
    }
    Ok(())
}

fn migrate_tag_tool_config_quiet(conn: &Connection) -> Result<()> {
    let table_exists = conn
        .prepare("SELECT id FROM tag_tool_config LIMIT 0")
        .is_ok();
    if !table_exists {
        return Ok(());
    }
    let has_quiet = conn
        .prepare("SELECT quiet FROM tag_tool_config LIMIT 0")
        .is_ok();
    if !has_quiet {
        conn.execute_batch(
            "ALTER TABLE tag_tool_config ADD COLUMN quiet INTEGER NOT NULL DEFAULT 1;",
        )?;
    }
    Ok(())
}

fn migrate_category_policy_table(conn: &Connection) -> Result<()> {
    let has_table = conn
        .prepare("SELECT id FROM category_policy LIMIT 0")
        .is_ok();
    if has_table {
        return Ok(());
    }
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS category_policy (
            id INTEGER PRIMARY KEY,
            category_id INTEGER NOT NULL REFERENCES categories(id),
            protection_level TEXT NOT NULL DEFAULT 'editable',
            UNIQUE(category_id)
        );",
    )?;
    let has_old_protection = conn
        .prepare("SELECT protection_level FROM categories LIMIT 0")
        .is_ok();
    if has_old_protection {
        conn.execute_batch(
            "INSERT INTO category_policy (category_id, protection_level)
             SELECT id, protection_level FROM categories;",
        )?;
    }
    Ok(())
}

fn migrate_file_tags_hash(conn: &Connection) -> Result<()> {
    let table_exists = conn
        .prepare("SELECT file_id FROM file_tags LIMIT 0")
        .is_ok();
    if !table_exists {
        return Ok(());
    }
    let has_file_hash = conn
        .prepare("SELECT file_hash FROM file_tags LIMIT 0")
        .is_ok();
    if !has_file_hash {
        conn.execute_batch("ALTER TABLE file_tags ADD COLUMN file_hash TEXT;")?;
    }
    Ok(())
}

fn migrate_category_name(conn: &Connection) -> Result<()> {
    let has_column = conn.prepare("SELECT name FROM categories LIMIT 0").is_ok();
    if has_column {
        return Ok(());
    }
    conn.execute_batch("ALTER TABLE categories ADD COLUMN name TEXT NOT NULL DEFAULT '';")?;
    conn.execute_batch(
        "UPDATE categories SET name = CASE
            WHEN pattern LIKE '%/**' THEN SUBSTR(pattern, 1, LENGTH(pattern) - 3)
            WHEN pattern LIKE '%/*' THEN SUBSTR(pattern, 1, LENGTH(pattern) - 2)
            ELSE pattern
        END
        WHERE name = '';",
    )?;
    Ok(())
}

const CATEGORY_COLUMNS: [Categories; 5] = [
    Categories::Id,
    Categories::Name,
    Categories::Pattern,
    Categories::CategoryType,
    Categories::Description,
];

fn row_to_category(row: &rusqlite::Row) -> rusqlite::Result<Category> {
    Ok(Category {
        id: Some(row.get(0)?),
        name: row.get(1)?,
        pattern: row.get(2)?,
        category_type: row.get::<_, String>(3)?.parse().map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(
                3,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("{e}"),
                )),
            )
        })?,
        description: row.get(4)?,
    })
}

const FILE_COLUMNS: [Files; 9] = [
    Files::Id,
    Files::Name,
    Files::Path,
    Files::Sha256,
    Files::MimeType,
    Files::Size,
    Files::IngestedAt,
    Files::Provenance,
    Files::Immutable,
];

fn row_to_file(row: &rusqlite::Row) -> rusqlite::Result<TrackedFile> {
    Ok(TrackedFile {
        id: Some(row.get(0)?),
        name: row.get(1)?,
        path: row.get(2)?,
        sha256: row.get(3)?,
        mime_type: row.get(4)?,
        size: row.get(5)?,
        ingested_at: row.get(6)?,
        provenance: row.get(7)?,
        immutable: row.get::<_, i32>(8)? != 0,
    })
}

#[derive(Debug, Clone)]
pub struct ToolConfigRow {
    pub id: i64,
    pub scope: Option<String>,
    pub action: String,
    pub file_type: String,
    pub command: String,
    pub env: Option<String>,
    pub quiet: bool,
}

#[derive(Debug, Clone)]
pub struct TagToolConfigRow {
    pub id: i64,
    pub tag: String,
    pub action: String,
    pub file_type: String,
    pub command: String,
    pub env: Option<String>,
    pub quiet: bool,
}

pub struct ToolConfigParams<'a> {
    pub scope: Option<&'a str>,
    pub action: &'a str,
    pub file_type: &'a str,
    pub command: &'a str,
    pub env: Option<&'a str>,
    pub quiet: bool,
}

pub struct TagToolConfigParams<'a> {
    pub tag: &'a str,
    pub action: &'a str,
    pub file_type: &'a str,
    pub command: &'a str,
    pub env: Option<&'a str>,
    pub quiet: bool,
}

pub(crate) const TOOL_CONFIG_COLUMNS: [ToolConfig; 7] = [
    ToolConfig::Id,
    ToolConfig::Scope,
    ToolConfig::Action,
    ToolConfig::FileType,
    ToolConfig::Command,
    ToolConfig::Env,
    ToolConfig::Quiet,
];

pub(crate) fn row_to_tool_config(row: &rusqlite::Row) -> rusqlite::Result<ToolConfigRow> {
    Ok(ToolConfigRow {
        id: row.get(0)?,
        scope: row.get(1)?,
        action: row.get(2)?,
        file_type: row.get(3)?,
        command: row.get(4)?,
        env: row.get(5)?,
        quiet: row.get::<_, i32>(6)? != 0,
    })
}

pub(crate) const TAG_TOOL_CONFIG_COLUMNS: [TagToolConfig; 7] = [
    TagToolConfig::Id,
    TagToolConfig::Tag,
    TagToolConfig::Action,
    TagToolConfig::FileType,
    TagToolConfig::Command,
    TagToolConfig::Env,
    TagToolConfig::Quiet,
];

pub(crate) fn row_to_tag_tool_config(row: &rusqlite::Row) -> rusqlite::Result<TagToolConfigRow> {
    Ok(TagToolConfigRow {
        id: row.get(0)?,
        tag: row.get(1)?,
        action: row.get(2)?,
        file_type: row.get(3)?,
        command: row.get(4)?,
        env: row.get(5)?,
        quiet: row.get::<_, i32>(6)? != 0,
    })
}

const RULE_COLUMNS: [Rules; 9] = [
    Rules::Id,
    Rules::Name,
    Rules::Enabled,
    Rules::TriggerEvent,
    Rules::TriggerFilter,
    Rules::ActionType,
    Rules::ActionConfig,
    Rules::Priority,
    Rules::CreatedAt,
];

const RULE_COLUMNS_INSERT: [Rules; 8] = [
    Rules::Name,
    Rules::Enabled,
    Rules::TriggerEvent,
    Rules::TriggerFilter,
    Rules::ActionType,
    Rules::ActionConfig,
    Rules::Priority,
    Rules::CreatedAt,
];

fn col_conversion_err(col: usize, e: impl std::fmt::Display) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        col,
        rusqlite::types::Type::Text,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("{e}"),
        )),
    )
}

fn row_to_rule(row: &rusqlite::Row) -> rusqlite::Result<Rule> {
    let trigger_event_str: String = row.get(3)?;
    let trigger_filter_json: Option<String> = row.get(4)?;
    let action_type_str: String = row.get(5)?;
    let action_config_json: String = row.get(6)?;

    let trigger_event: TriggerEvent = trigger_event_str
        .parse()
        .map_err(|e| col_conversion_err(3, e))?;

    let trigger_filter: TriggerFilter = trigger_filter_json
        .as_deref()
        .map(serde_json::from_str)
        .transpose()
        .map_err(|e| col_conversion_err(4, e))?
        .unwrap_or_default();

    let action_type: ActionType = action_type_str
        .parse()
        .map_err(|e| col_conversion_err(5, e))?;

    let action_config: ActionConfig =
        serde_json::from_str(&action_config_json).map_err(|e| col_conversion_err(6, e))?;

    Ok(Rule {
        id: Some(row.get(0)?),
        name: row.get(1)?,
        enabled: row.get::<_, i32>(2)? != 0,
        trigger_event,
        trigger_filter,
        action_type,
        action_config,
        priority: row.get(7)?,
        created_at: row.get(8)?,
    })
}

fn migrate_rules_table(conn: &Connection) -> Result<()> {
    let has_table = conn.prepare("SELECT id FROM rules LIMIT 0").is_ok();
    if has_table {
        return Ok(());
    }
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS rules (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            enabled INTEGER NOT NULL DEFAULT 1,
            trigger_event TEXT NOT NULL,
            trigger_filter TEXT,
            action_type TEXT NOT NULL,
            action_config TEXT NOT NULL,
            priority INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL
        );",
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::CategoryType;
    use tempfile::TempDir;

    fn setup() -> (TempDir, ProjectDb) {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join(".mkrk");
        let db = ProjectDb::create(&db_path).unwrap();
        (dir, db)
    }

    #[test]
    fn create_and_open() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join(".mkrk");
        ProjectDb::create(&db_path).unwrap();
        ProjectDb::open(&db_path).unwrap();
    }

    #[test]
    fn category_crud() {
        let (_dir, db) = setup();
        let cat = Category {
            id: None,
            name: "evidence".to_string(),
            pattern: "evidence/**".to_string(),
            category_type: CategoryType::Files,
            description: Some("Evidence files".to_string()),
        };
        let cat_id = db.insert_category(&cat).unwrap();
        db.insert_category_policy(cat_id, &ProtectionLevel::Immutable)
            .unwrap();
        let cats = db.list_categories().unwrap();
        assert_eq!(cats.len(), 1);
        assert_eq!(cats[0].pattern, "evidence/**");
        assert_eq!(cats[0].category_type, CategoryType::Files);

        let policy = db.get_policy_for_category(cat_id).unwrap();
        assert_eq!(policy, Some(ProtectionLevel::Immutable));
    }

    #[test]
    fn category_type_roundtrip_through_db() {
        let (_dir, db) = setup();
        for ct in &[
            CategoryType::Files,
            CategoryType::Tools,
            CategoryType::Inbox,
        ] {
            let cat = Category {
                id: None,
                name: ct.to_string(),
                pattern: format!("{}/**", ct),
                category_type: *ct,
                description: None,
            };
            db.insert_category(&cat).unwrap();
        }
        let cats = db.list_categories().unwrap();
        assert_eq!(cats.len(), 3);
        assert_eq!(cats[0].category_type, CategoryType::Files);
        assert_eq!(cats[1].category_type, CategoryType::Tools);
        assert_eq!(cats[2].category_type, CategoryType::Inbox);
    }

    #[test]
    fn file_crud() {
        let (_dir, db) = setup();
        let file = TrackedFile {
            id: None,
            name: "test.pdf".to_string(),
            path: "evidence/test.pdf".to_string(),
            sha256: Some("abc123".to_string()),
            mime_type: Some("application/pdf".to_string()),
            size: Some(1024),
            ingested_at: "2025-01-01T00:00:00Z".to_string(),
            provenance: None,
            immutable: true,
        };
        let id = db.insert_file(&file).unwrap();
        assert!(id > 0);

        let found = db.get_file_by_name("test.pdf").unwrap().unwrap();
        assert_eq!(found.path, "evidence/test.pdf");
        assert!(found.immutable);

        let found_by_path = db.get_file_by_path("evidence/test.pdf").unwrap().unwrap();
        assert_eq!(found_by_path.name, "test.pdf");

        let files = db.list_files(None).unwrap();
        assert_eq!(files.len(), 1);

        let filtered = db.list_files(Some("evidence/")).unwrap();
        assert_eq!(filtered.len(), 1);

        let empty = db.list_files(Some("notes/")).unwrap();
        assert!(empty.is_empty());
    }

    #[test]
    fn tag_crud() {
        let (_dir, db) = setup();
        let file = TrackedFile {
            id: None,
            name: "recording.wav".to_string(),
            path: "evidence/recording.wav".to_string(),
            sha256: Some("def456".to_string()),
            mime_type: Some("audio/wav".to_string()),
            size: Some(2048),
            ingested_at: "2025-01-01T00:00:00Z".to_string(),
            provenance: None,
            immutable: false,
        };
        let file_id = db.insert_file(&file).unwrap();

        db.insert_tag(file_id, "speech", "def456").unwrap();
        db.insert_tag(file_id, "rf", "def456").unwrap();

        let tags = db.get_tags(file_id).unwrap();
        assert_eq!(tags, vec!["rf", "speech"]);

        db.remove_tag(file_id, "rf").unwrap();
        let tags = db.get_tags(file_id).unwrap();
        assert_eq!(tags, vec!["speech"]);

        let by_tag = db.list_files_by_tag("speech").unwrap();
        assert_eq!(by_tag.len(), 1);
        assert_eq!(by_tag[0].name, "recording.wav");
    }

    #[test]
    fn tag_stores_and_retrieves_hash() {
        let (_dir, db) = setup();
        let file = TrackedFile {
            id: None,
            name: "doc.pdf".to_string(),
            path: "evidence/doc.pdf".to_string(),
            sha256: Some("abc123".to_string()),
            mime_type: Some("application/pdf".to_string()),
            size: Some(1024),
            ingested_at: "2025-01-01T00:00:00Z".to_string(),
            provenance: None,
            immutable: false,
        };
        let file_id = db.insert_file(&file).unwrap();

        db.insert_tag(file_id, "classified", "sha256_hash_value")
            .unwrap();

        let hash = db.get_file_tag_hash(file_id, "classified").unwrap();
        assert_eq!(hash, Some("sha256_hash_value".to_string()));

        let all = db.list_all_tags().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].file_hash, Some("sha256_hash_value".to_string()));
    }

    #[test]
    fn retagging_updates_hash() {
        let (_dir, db) = setup();
        let file = TrackedFile {
            id: None,
            name: "doc.pdf".to_string(),
            path: "evidence/doc.pdf".to_string(),
            sha256: Some("abc123".to_string()),
            mime_type: Some("application/pdf".to_string()),
            size: Some(1024),
            ingested_at: "2025-01-01T00:00:00Z".to_string(),
            provenance: None,
            immutable: false,
        };
        let file_id = db.insert_file(&file).unwrap();

        db.insert_tag(file_id, "classified", "old_hash").unwrap();
        assert_eq!(
            db.get_file_tag_hash(file_id, "classified").unwrap(),
            Some("old_hash".to_string())
        );

        db.insert_tag(file_id, "classified", "new_hash").unwrap();
        assert_eq!(
            db.get_file_tag_hash(file_id, "classified").unwrap(),
            Some("new_hash".to_string())
        );

        let tags = db.get_tags(file_id).unwrap();
        assert_eq!(tags.len(), 1, "re-tagging should not create duplicates");
    }

    #[test]
    fn get_file_tag_hash_returns_none_for_missing() {
        let (_dir, db) = setup();
        let hash = db.get_file_tag_hash(999, "nonexistent").unwrap();
        assert_eq!(hash, None);
    }

    #[test]
    fn migrate_adds_file_hash_column() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join(".mkrk");

        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
            .unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS categories (
                id INTEGER PRIMARY KEY,
                pattern TEXT NOT NULL UNIQUE,
                category_type TEXT NOT NULL DEFAULT 'files',
                description TEXT
            );
            CREATE TABLE IF NOT EXISTS category_policy (
                id INTEGER PRIMARY KEY,
                category_id INTEGER NOT NULL REFERENCES categories(id),
                protection_level TEXT NOT NULL DEFAULT 'editable',
                UNIQUE(category_id)
            );
            CREATE TABLE IF NOT EXISTS files (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                path TEXT NOT NULL UNIQUE,
                sha256 TEXT,
                mime_type TEXT,
                size INTEGER,
                ingested_at TEXT NOT NULL,
                provenance TEXT,
                immutable INTEGER DEFAULT 0
            );
            CREATE TABLE IF NOT EXISTS file_tags (
                file_id INTEGER REFERENCES files(id),
                tag TEXT NOT NULL,
                PRIMARY KEY (file_id, tag)
            );
            CREATE TABLE IF NOT EXISTS tool_config (
                id INTEGER PRIMARY KEY,
                scope TEXT,
                action TEXT NOT NULL,
                file_type TEXT NOT NULL,
                command TEXT NOT NULL,
                env TEXT,
                quiet INTEGER NOT NULL DEFAULT 1
            );
            CREATE TABLE IF NOT EXISTS tag_tool_config (
                id INTEGER PRIMARY KEY,
                tag TEXT NOT NULL,
                action TEXT NOT NULL,
                file_type TEXT NOT NULL,
                command TEXT NOT NULL,
                env TEXT,
                quiet INTEGER NOT NULL DEFAULT 1,
                UNIQUE(tag, action, file_type)
            );
            CREATE TABLE IF NOT EXISTS audit_log (
                id INTEGER PRIMARY KEY,
                timestamp TEXT NOT NULL,
                operation TEXT NOT NULL,
                file_id INTEGER REFERENCES files(id),
                user TEXT,
                detail TEXT
            );",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO files (name, path, ingested_at) VALUES (?1, ?2, ?3)",
            rusqlite::params!["test.pdf", "test.pdf", "2025-01-01T00:00:00Z"],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO file_tags (file_id, tag) VALUES (1, 'old-tag')",
            [],
        )
        .unwrap();
        drop(conn);

        let db = ProjectDb::open(&db_path).unwrap();

        let tags = db.list_all_tags().unwrap();
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].tag, "old-tag");
        assert_eq!(tags[0].file_hash, None);

        db.insert_tag(1, "new-tag", "somehash").unwrap();
        let hash = db.get_file_tag_hash(1, "new-tag").unwrap();
        assert_eq!(hash, Some("somehash".to_string()));
    }

    #[test]
    fn match_category_most_specific() {
        let (_dir, db) = setup();
        let id1 = db
            .insert_category(&Category {
                id: None,
                name: "evidence".to_string(),
                pattern: "evidence/**".to_string(),
                category_type: CategoryType::Files,
                description: None,
            })
            .unwrap();
        db.insert_category_policy(id1, &ProtectionLevel::Immutable)
            .unwrap();

        let id2 = db
            .insert_category(&Category {
                id: None,
                name: "evidence/financial".to_string(),
                pattern: "evidence/financial/**".to_string(),
                category_type: CategoryType::Files,
                description: None,
            })
            .unwrap();
        db.insert_category_policy(id2, &ProtectionLevel::Protected)
            .unwrap();

        // Parent's stricter policy (immutable) wins over child's (protected)
        let protection = db
            .resolve_protection("evidence/financial/receipt.pdf")
            .unwrap();
        assert_eq!(protection, ProtectionLevel::Immutable);

        let protection = db.resolve_protection("evidence/photo.jpg").unwrap();
        assert_eq!(protection, ProtectionLevel::Immutable);
    }

    #[test]
    fn resolve_protection_child_can_tighten() {
        let (_dir, db) = setup();
        let id1 = db
            .insert_category(&Category {
                id: None,
                name: "notes".to_string(),
                pattern: "notes/**".to_string(),
                category_type: CategoryType::Files,
                description: None,
            })
            .unwrap();
        db.insert_category_policy(id1, &ProtectionLevel::Editable)
            .unwrap();

        let id2 = db
            .insert_category(&Category {
                id: None,
                name: "notes/confidential".to_string(),
                pattern: "notes/confidential/**".to_string(),
                category_type: CategoryType::Files,
                description: None,
            })
            .unwrap();
        db.insert_category_policy(id2, &ProtectionLevel::Protected)
            .unwrap();

        // Child tightens from editable to protected
        let protection = db.resolve_protection("notes/confidential/doc.md").unwrap();
        assert_eq!(protection, ProtectionLevel::Protected);

        // Parent only → editable
        let protection = db.resolve_protection("notes/todo.md").unwrap();
        assert_eq!(protection, ProtectionLevel::Editable);
    }

    #[test]
    fn resolve_protection_multiple_overlapping() {
        let (_dir, db) = setup();
        let id1 = db
            .insert_category(&Category {
                id: None,
                name: "evidence".to_string(),
                pattern: "evidence/**".to_string(),
                category_type: CategoryType::Files,
                description: None,
            })
            .unwrap();
        db.insert_category_policy(id1, &ProtectionLevel::Protected)
            .unwrap();

        let id2 = db
            .insert_category(&Category {
                id: None,
                name: "evidence/financial".to_string(),
                pattern: "evidence/financial/**".to_string(),
                category_type: CategoryType::Files,
                description: None,
            })
            .unwrap();
        db.insert_category_policy(id2, &ProtectionLevel::Editable)
            .unwrap();

        let id3 = db
            .insert_category(&Category {
                id: None,
                name: "evidence/financial/tax".to_string(),
                pattern: "evidence/financial/tax/**".to_string(),
                category_type: CategoryType::Files,
                description: None,
            })
            .unwrap();
        db.insert_category_policy(id3, &ProtectionLevel::Immutable)
            .unwrap();

        // Three overlapping: protected + editable + immutable → immutable wins
        let protection = db
            .resolve_protection("evidence/financial/tax/return.pdf")
            .unwrap();
        assert_eq!(protection, ProtectionLevel::Immutable);

        // Two overlapping: protected + editable → protected wins
        let protection = db
            .resolve_protection("evidence/financial/invoice.pdf")
            .unwrap();
        assert_eq!(protection, ProtectionLevel::Protected);
    }

    #[test]
    fn audit_log() {
        let (_dir, db) = setup();
        let file = TrackedFile {
            id: None,
            name: "test.pdf".to_string(),
            path: "test.pdf".to_string(),
            sha256: None,
            mime_type: None,
            size: None,
            ingested_at: "2025-01-01T00:00:00Z".to_string(),
            provenance: None,
            immutable: false,
        };
        let file_id = db.insert_file(&file).unwrap();
        db.insert_audit(
            "ingest",
            Some(file_id),
            Some("user"),
            Some("{\"source\": \"file\"}"),
        )
        .unwrap();
        db.insert_audit("verify", None, Some("user"), None).unwrap();
        let last = db.last_verify_time().unwrap();
        assert!(last.is_some());
    }

    #[test]
    fn counts() {
        let (_dir, db) = setup();
        assert_eq!(db.file_count().unwrap(), 0);
        assert_eq!(db.category_count().unwrap(), 0);
        assert_eq!(db.tag_count().unwrap(), 0);
    }

    #[test]
    fn category_policy_crud() {
        let (_dir, db) = setup();
        let cat_id = db
            .insert_category(&Category {
                id: None,
                name: "docs".to_string(),
                pattern: "docs/**".to_string(),
                category_type: CategoryType::Files,
                description: None,
            })
            .unwrap();

        assert_eq!(db.get_policy_for_category(cat_id).unwrap(), None);

        db.insert_category_policy(cat_id, &ProtectionLevel::Protected)
            .unwrap();
        assert_eq!(
            db.get_policy_for_category(cat_id).unwrap(),
            Some(ProtectionLevel::Protected)
        );

        db.insert_category_policy(cat_id, &ProtectionLevel::Immutable)
            .unwrap();
        assert_eq!(
            db.get_policy_for_category(cat_id).unwrap(),
            Some(ProtectionLevel::Immutable)
        );
    }

    #[test]
    fn resolve_protection_with_policy() {
        let (_dir, db) = setup();
        let cat_id = db
            .insert_category(&Category {
                id: None,
                name: "evidence".to_string(),
                pattern: "evidence/**".to_string(),
                category_type: CategoryType::Files,
                description: None,
            })
            .unwrap();
        db.insert_category_policy(cat_id, &ProtectionLevel::Immutable)
            .unwrap();

        assert_eq!(
            db.resolve_protection("evidence/doc.pdf").unwrap(),
            ProtectionLevel::Immutable
        );
    }

    #[test]
    fn resolve_protection_no_match_defaults_editable() {
        let (_dir, db) = setup();
        assert_eq!(
            db.resolve_protection("random/file.txt").unwrap(),
            ProtectionLevel::Editable
        );
    }

    #[test]
    fn resolve_protection_category_without_policy_defaults_editable() {
        let (_dir, db) = setup();
        db.insert_category(&Category {
            id: None,
            name: "notes".to_string(),
            pattern: "notes/**".to_string(),
            category_type: CategoryType::Files,
            description: None,
        })
        .unwrap();

        assert_eq!(
            db.resolve_protection("notes/todo.md").unwrap(),
            ProtectionLevel::Editable
        );
    }

    #[test]
    fn migrate_adds_category_type_column() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join(".mkrk");

        let old_schema = "
            CREATE TABLE IF NOT EXISTS categories (
                id INTEGER PRIMARY KEY,
                pattern TEXT NOT NULL UNIQUE,
                protection_level TEXT NOT NULL,
                description TEXT
            );
        ";
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
            .unwrap();
        conn.execute_batch(old_schema).unwrap();
        conn.execute(
            "INSERT INTO categories (pattern, protection_level, description) VALUES (?1, ?2, ?3)",
            rusqlite::params!["evidence/**", "immutable", "Evidence"],
        )
        .unwrap();
        drop(conn);

        let db = ProjectDb::open(&db_path).unwrap();
        let cats = db.list_categories().unwrap();
        assert_eq!(cats.len(), 1);
        assert_eq!(cats[0].category_type, CategoryType::Files);
    }

    #[test]
    fn policy_migration_from_old_schema() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join(".mkrk");

        let old_schema = "
            CREATE TABLE IF NOT EXISTS categories (
                id INTEGER PRIMARY KEY,
                pattern TEXT NOT NULL UNIQUE,
                category_type TEXT NOT NULL DEFAULT 'files',
                protection_level TEXT NOT NULL,
                description TEXT
            );
        ";
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
            .unwrap();
        conn.execute_batch(old_schema).unwrap();
        conn.execute(
            "INSERT INTO categories (pattern, category_type, protection_level, description) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params!["evidence/**", "files", "immutable", "Evidence"],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO categories (pattern, category_type, protection_level, description) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params!["notes/**", "files", "protected", "Notes"],
        )
        .unwrap();
        drop(conn);

        let db = ProjectDb::open(&db_path).unwrap();

        let cats = db.list_categories().unwrap();
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
            db.get_policy_for_category(ev_id).unwrap(),
            Some(ProtectionLevel::Immutable)
        );
        assert_eq!(
            db.get_policy_for_category(notes_id).unwrap(),
            Some(ProtectionLevel::Protected)
        );
    }

    fn make_rule(name: &str, event: TriggerEvent, action_type: ActionType) -> Rule {
        Rule {
            id: None,
            name: name.to_string(),
            enabled: true,
            trigger_event: event,
            trigger_filter: TriggerFilter::default(),
            action_type,
            action_config: ActionConfig {
                tool: Some("ocr".to_string()),
                tag: None,
            },
            priority: 0,
            created_at: "2025-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn rule_crud() {
        let (_dir, db) = setup();
        let rule = make_rule("ocr-pdfs", TriggerEvent::Ingest, ActionType::RunTool);
        let id = db.insert_rule(&rule).unwrap();
        assert!(id > 0);

        let rules = db.list_rules().unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].name, "ocr-pdfs");
        assert!(rules[0].enabled);
        assert_eq!(rules[0].trigger_event, TriggerEvent::Ingest);
        assert_eq!(rules[0].action_type, ActionType::RunTool);
        assert_eq!(rules[0].action_config.tool.as_deref(), Some("ocr"));

        let found = db.get_rule_by_name("ocr-pdfs").unwrap();
        assert!(found.is_some());

        let missing = db.get_rule_by_name("nope").unwrap();
        assert!(missing.is_none());

        let removed = db.remove_rule("ocr-pdfs").unwrap();
        assert_eq!(removed, 1);
        assert!(db.list_rules().unwrap().is_empty());
    }

    #[test]
    fn rule_enable_disable() {
        let (_dir, db) = setup();
        let rule = make_rule("tag-review", TriggerEvent::Ingest, ActionType::AddTag);
        db.insert_rule(&rule).unwrap();

        db.set_rule_enabled("tag-review", false).unwrap();
        let r = db.get_rule_by_name("tag-review").unwrap().unwrap();
        assert!(!r.enabled);

        db.set_rule_enabled("tag-review", true).unwrap();
        let r = db.get_rule_by_name("tag-review").unwrap().unwrap();
        assert!(r.enabled);
    }

    #[test]
    fn get_matching_rules_filters_by_event_and_enabled() {
        let (_dir, db) = setup();
        let r1 = make_rule("ingest-rule", TriggerEvent::Ingest, ActionType::RunTool);
        let r2 = make_rule("tag-rule", TriggerEvent::Tag, ActionType::RunTool);
        db.insert_rule(&r1).unwrap();
        db.insert_rule(&r2).unwrap();

        let ingest_rules = db.get_matching_rules(TriggerEvent::Ingest).unwrap();
        assert_eq!(ingest_rules.len(), 1);
        assert_eq!(ingest_rules[0].name, "ingest-rule");

        let tag_rules = db.get_matching_rules(TriggerEvent::Tag).unwrap();
        assert_eq!(tag_rules.len(), 1);
        assert_eq!(tag_rules[0].name, "tag-rule");

        db.set_rule_enabled("ingest-rule", false).unwrap();
        assert!(db
            .get_matching_rules(TriggerEvent::Ingest)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn rule_with_filter_roundtrip() {
        let (_dir, db) = setup();
        let mut rule = make_rule("filtered", TriggerEvent::Ingest, ActionType::RunTool);
        rule.trigger_filter = TriggerFilter {
            category: Some("evidence".to_string()),
            mime_type: Some("application/pdf".to_string()),
            ..Default::default()
        };
        db.insert_rule(&rule).unwrap();

        let found = db.get_rule_by_name("filtered").unwrap().unwrap();
        assert_eq!(found.trigger_filter.category.as_deref(), Some("evidence"));
        assert_eq!(
            found.trigger_filter.mime_type.as_deref(),
            Some("application/pdf"),
        );
        assert!(found.trigger_filter.tag_name.is_none());
        assert!(found.trigger_filter.file_type.is_none());
    }

    #[test]
    fn rule_priority_ordering() {
        let (_dir, db) = setup();
        let mut r1 = make_rule("low-priority", TriggerEvent::Ingest, ActionType::RunTool);
        r1.priority = 10;
        let mut r2 = make_rule("high-priority", TriggerEvent::Ingest, ActionType::RunTool);
        r2.priority = 1;
        db.insert_rule(&r1).unwrap();
        db.insert_rule(&r2).unwrap();

        let rules = db.get_matching_rules(TriggerEvent::Ingest).unwrap();
        assert_eq!(rules[0].name, "high-priority");
        assert_eq!(rules[1].name, "low-priority");
    }

    #[test]
    fn rule_duplicate_name_rejected() {
        let (_dir, db) = setup();
        let r = make_rule("dup", TriggerEvent::Ingest, ActionType::RunTool);
        db.insert_rule(&r).unwrap();
        assert!(db.insert_rule(&r).is_err());
    }

    #[test]
    fn rule_remove_nonexistent_returns_zero() {
        let (_dir, db) = setup();
        let count = db.remove_rule("ghost").unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn rule_set_enabled_nonexistent_returns_zero() {
        let (_dir, db) = setup();
        let count = db.set_rule_enabled("ghost", true).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn rule_list_empty() {
        let (_dir, db) = setup();
        let rules = db.list_rules().unwrap();
        assert!(rules.is_empty());
    }

    #[test]
    fn rule_with_empty_filter() {
        let (_dir, db) = setup();
        let r = make_rule("empty-filter", TriggerEvent::Ingest, ActionType::AddTag);
        db.insert_rule(&r).unwrap();

        let found = db.get_rule_by_name("empty-filter").unwrap().unwrap();
        assert!(found.trigger_filter.is_empty());
    }

    #[test]
    fn get_matching_rules_wrong_event_returns_empty() {
        let (_dir, db) = setup();
        let r = make_rule("ingest-only", TriggerEvent::Ingest, ActionType::RunTool);
        db.insert_rule(&r).unwrap();

        let rules = db.get_matching_rules(TriggerEvent::Tag).unwrap();
        assert!(rules.is_empty());
    }
}
