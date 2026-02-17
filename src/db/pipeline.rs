use anyhow::Result;
use rusqlite::Connection;
use sea_query::{Asterisk, Expr, ExprTrait, Func, Order, Query, SqliteQueryBuilder};
use sea_query_rusqlite::RusqliteBinder;

use crate::models::{AttachmentScope, Pipeline, PipelineAttachment, Sign};

use super::iden::{DefaultPipelines, PipelineAttachments, Pipelines, Signs};

pub fn insert_pipeline(conn: &Connection, pipeline: &Pipeline) -> Result<i64> {
    let states_json = serde_json::to_string(&pipeline.states)?;
    let transitions_json = serde_json::to_string(&pipeline.transitions)?;

    let (sql, values) = Query::insert()
        .into_table(Pipelines::Table)
        .columns([Pipelines::Name, Pipelines::States, Pipelines::Transitions])
        .values_panic([
            pipeline.name.as_str().into(),
            states_json.into(),
            transitions_json.into(),
        ])
        .build_rusqlite(SqliteQueryBuilder);
    conn.execute(&sql, &*values.as_params())?;
    Ok(conn.last_insert_rowid())
}

pub fn get_pipeline_by_name(conn: &Connection, name: &str) -> Result<Option<Pipeline>> {
    let (sql, values) = Query::select()
        .columns(PIPELINE_COLUMNS)
        .from(Pipelines::Table)
        .and_where(Expr::col(Pipelines::Name).eq(name))
        .build_rusqlite(SqliteQueryBuilder);
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query_map(&*values.as_params(), row_to_pipeline)?;
    match rows.next() {
        Some(row) => Ok(Some(row?)),
        None => Ok(None),
    }
}

pub fn list_pipelines(conn: &Connection) -> Result<Vec<Pipeline>> {
    let (sql, values) = Query::select()
        .columns(PIPELINE_COLUMNS)
        .from(Pipelines::Table)
        .order_by(Pipelines::Name, Order::Asc)
        .build_rusqlite(SqliteQueryBuilder);
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(&*values.as_params(), row_to_pipeline)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn remove_pipeline(conn: &Connection, name: &str) -> Result<u64> {
    let pipeline = get_pipeline_by_name(conn, name)?;
    let Some(pipeline) = pipeline else {
        return Ok(0);
    };
    let pipeline_id = pipeline.id.unwrap();

    let (sql, values) = Query::delete()
        .from_table(Signs::Table)
        .and_where(Expr::col(Signs::PipelineId).eq(pipeline_id))
        .build_rusqlite(SqliteQueryBuilder);
    conn.execute(&sql, &*values.as_params())?;

    let (sql, values) = Query::delete()
        .from_table(PipelineAttachments::Table)
        .and_where(Expr::col(PipelineAttachments::PipelineId).eq(pipeline_id))
        .build_rusqlite(SqliteQueryBuilder);
    conn.execute(&sql, &*values.as_params())?;

    let (sql, values) = Query::delete()
        .from_table(Pipelines::Table)
        .and_where(Expr::col(Pipelines::Name).eq(name))
        .build_rusqlite(SqliteQueryBuilder);
    let count = conn.execute(&sql, &*values.as_params())?;
    Ok(count as u64)
}

pub fn pipeline_count(conn: &Connection) -> Result<i64> {
    let (sql, values) = Query::select()
        .expr(Func::count(Expr::col(Asterisk)))
        .from(Pipelines::Table)
        .build_rusqlite(SqliteQueryBuilder);
    Ok(conn.query_row(&sql, &*values.as_params(), |row| row.get(0))?)
}

pub fn attach_pipeline(
    conn: &Connection,
    pipeline_id: i64,
    scope_type: AttachmentScope,
    scope_value: &str,
) -> Result<i64> {
    let (sql, values) = Query::insert()
        .into_table(PipelineAttachments::Table)
        .columns([
            PipelineAttachments::PipelineId,
            PipelineAttachments::ScopeType,
            PipelineAttachments::ScopeValue,
        ])
        .values_panic([
            pipeline_id.into(),
            scope_type.to_string().into(),
            scope_value.into(),
        ])
        .build_rusqlite(SqliteQueryBuilder);
    conn.execute(&sql, &*values.as_params())?;
    Ok(conn.last_insert_rowid())
}

pub fn detach_pipeline(
    conn: &Connection,
    pipeline_id: i64,
    scope_type: AttachmentScope,
    scope_value: &str,
) -> Result<u64> {
    let (sql, values) = Query::delete()
        .from_table(PipelineAttachments::Table)
        .and_where(Expr::col(PipelineAttachments::PipelineId).eq(pipeline_id))
        .and_where(Expr::col(PipelineAttachments::ScopeType).eq(scope_type.to_string()))
        .and_where(Expr::col(PipelineAttachments::ScopeValue).eq(scope_value))
        .build_rusqlite(SqliteQueryBuilder);
    let count = conn.execute(&sql, &*values.as_params())?;
    Ok(count as u64)
}

pub fn list_attachments_for_pipeline(
    conn: &Connection,
    pipeline_id: i64,
) -> Result<Vec<PipelineAttachment>> {
    let (sql, values) = Query::select()
        .columns(ATTACHMENT_COLUMNS)
        .from(PipelineAttachments::Table)
        .and_where(Expr::col(PipelineAttachments::PipelineId).eq(pipeline_id))
        .order_by(PipelineAttachments::ScopeType, Order::Asc)
        .order_by(PipelineAttachments::ScopeValue, Order::Asc)
        .build_rusqlite(SqliteQueryBuilder);
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(&*values.as_params(), row_to_attachment)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn get_pipelines_for_file(
    conn: &Connection,
    file_id: i64,
    rel_path: &str,
    categories: &[crate::models::Category],
    tags: &[String],
) -> Result<Vec<Pipeline>> {
    let mut pipeline_ids = collect_pipeline_ids_for_scope(conn, rel_path, categories, tags)?;
    pipeline_ids.sort_unstable();
    pipeline_ids.dedup();

    let _ = file_id;

    let mut pipelines = Vec::new();
    for pid in pipeline_ids {
        let (sql, values) = Query::select()
            .columns(PIPELINE_COLUMNS)
            .from(Pipelines::Table)
            .and_where(Expr::col(Pipelines::Id).eq(pid))
            .build_rusqlite(SqliteQueryBuilder);
        let mut stmt = conn.prepare(&sql)?;
        let mut rows = stmt.query_map(&*values.as_params(), row_to_pipeline)?;
        if let Some(row) = rows.next() {
            pipelines.push(row?);
        }
    }

    Ok(pipelines)
}

fn collect_pipeline_ids_for_scope(
    conn: &Connection,
    rel_path: &str,
    categories: &[crate::models::Category],
    tags: &[String],
) -> Result<Vec<i64>> {
    let mut ids: Vec<i64> = Vec::new();

    for cat in categories {
        if cat.matches(rel_path).unwrap_or(false) {
            let (sql, values) = Query::select()
                .column(PipelineAttachments::PipelineId)
                .from(PipelineAttachments::Table)
                .and_where(
                    Expr::col(PipelineAttachments::ScopeType)
                        .eq(AttachmentScope::Category.to_string()),
                )
                .and_where(Expr::col(PipelineAttachments::ScopeValue).eq(cat.name.as_str()))
                .build_rusqlite(SqliteQueryBuilder);
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map(&*values.as_params(), |row| row.get::<_, i64>(0))?;
            for id in rows {
                ids.push(id?);
            }
        }
    }

    if !tags.is_empty() {
        let tag_values: Vec<sea_query::Value> = tags.iter().map(|t| t.as_str().into()).collect();
        let (sql, values) = Query::select()
            .column(PipelineAttachments::PipelineId)
            .from(PipelineAttachments::Table)
            .and_where(
                Expr::col(PipelineAttachments::ScopeType).eq(AttachmentScope::Tag.to_string()),
            )
            .and_where(Expr::col(PipelineAttachments::ScopeValue).is_in(tag_values))
            .build_rusqlite(SqliteQueryBuilder);
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(&*values.as_params(), |row| row.get::<_, i64>(0))?;
        for id in rows {
            ids.push(id?);
        }
    }

    Ok(ids)
}

pub fn insert_sign(conn: &Connection, sign: &Sign) -> Result<i64> {
    let (sql, values) = Query::insert()
        .into_table(Signs::Table)
        .columns([
            Signs::PipelineId,
            Signs::FileId,
            Signs::FileHash,
            Signs::SignName,
            Signs::Signer,
            Signs::SignedAt,
            Signs::Signature,
        ])
        .values_panic([
            sign.pipeline_id.into(),
            sign.file_id.into(),
            sign.file_hash.as_str().into(),
            sign.sign_name.as_str().into(),
            sign.signer.as_str().into(),
            sign.signed_at.as_str().into(),
            sign.signature.clone().into(),
        ])
        .build_rusqlite(SqliteQueryBuilder);
    conn.execute(&sql, &*values.as_params())?;
    Ok(conn.last_insert_rowid())
}

pub fn revoke_sign(conn: &Connection, sign_id: i64, revoked_at: &str) -> Result<u64> {
    let (sql, values) = Query::update()
        .table(Signs::Table)
        .value(Signs::RevokedAt, revoked_at)
        .and_where(Expr::col(Signs::Id).eq(sign_id))
        .and_where(Expr::col(Signs::RevokedAt).is_null())
        .build_rusqlite(SqliteQueryBuilder);
    let count = conn.execute(&sql, &*values.as_params())?;
    Ok(count as u64)
}

pub fn get_valid_signs_for_file_pipeline(
    conn: &Connection,
    file_id: i64,
    pipeline_id: i64,
    current_hash: &str,
) -> Result<Vec<Sign>> {
    let (sql, values) = Query::select()
        .columns(SIGN_COLUMNS)
        .from(Signs::Table)
        .and_where(Expr::col(Signs::FileId).eq(file_id))
        .and_where(Expr::col(Signs::PipelineId).eq(pipeline_id))
        .and_where(Expr::col(Signs::FileHash).eq(current_hash))
        .and_where(Expr::col(Signs::RevokedAt).is_null())
        .order_by(Signs::SignedAt, Order::Asc)
        .build_rusqlite(SqliteQueryBuilder);
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(&*values.as_params(), row_to_sign)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn get_signs_for_file(conn: &Connection, file_id: i64) -> Result<Vec<Sign>> {
    let (sql, values) = Query::select()
        .columns(SIGN_COLUMNS)
        .from(Signs::Table)
        .and_where(Expr::col(Signs::FileId).eq(file_id))
        .order_by(Signs::PipelineId, Order::Asc)
        .order_by(Signs::SignedAt, Order::Asc)
        .build_rusqlite(SqliteQueryBuilder);
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(&*values.as_params(), row_to_sign)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn find_sign(
    conn: &Connection,
    file_id: i64,
    pipeline_id: i64,
    sign_name: &str,
) -> Result<Option<Sign>> {
    let (sql, values) = Query::select()
        .columns(SIGN_COLUMNS)
        .from(Signs::Table)
        .and_where(Expr::col(Signs::FileId).eq(file_id))
        .and_where(Expr::col(Signs::PipelineId).eq(pipeline_id))
        .and_where(Expr::col(Signs::SignName).eq(sign_name))
        .and_where(Expr::col(Signs::RevokedAt).is_null())
        .order_by(Signs::SignedAt, Order::Desc)
        .limit(1)
        .build_rusqlite(SqliteQueryBuilder);
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query_map(&*values.as_params(), row_to_sign)?;
    match rows.next() {
        Some(row) => Ok(Some(row?)),
        None => Ok(None),
    }
}

pub fn sign_count(conn: &Connection) -> Result<i64> {
    let (sql, values) = Query::select()
        .expr(Func::count(Expr::col(Asterisk)))
        .from(Signs::Table)
        .and_where(Expr::col(Signs::RevokedAt).is_null())
        .build_rusqlite(SqliteQueryBuilder);
    Ok(conn.query_row(&sql, &*values.as_params(), |row| row.get(0))?)
}

pub fn migrate_pipeline_tables(conn: &Connection) -> Result<()> {
    let has_table = conn.prepare("SELECT id FROM pipelines LIMIT 0").is_ok();
    if has_table {
        return Ok(());
    }
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS pipelines (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            states TEXT NOT NULL,
            transitions TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS pipeline_attachments (
            id INTEGER PRIMARY KEY,
            pipeline_id INTEGER NOT NULL REFERENCES pipelines(id),
            scope_type TEXT NOT NULL,
            scope_value TEXT NOT NULL,
            UNIQUE(pipeline_id, scope_type, scope_value)
        );
        CREATE TABLE IF NOT EXISTS signs (
            id INTEGER PRIMARY KEY,
            pipeline_id INTEGER NOT NULL REFERENCES pipelines(id),
            file_id INTEGER NOT NULL REFERENCES files(id),
            file_hash TEXT NOT NULL,
            sign_name TEXT NOT NULL,
            signer TEXT NOT NULL,
            signed_at TEXT NOT NULL,
            signature TEXT,
            revoked_at TEXT
        );",
    )?;
    Ok(())
}

pub fn insert_default_pipeline(conn: &Connection, pipeline: &Pipeline) -> Result<i64> {
    let states_json = serde_json::to_string(&pipeline.states)?;
    let transitions_json = serde_json::to_string(&pipeline.transitions)?;

    let (sql, values) = Query::insert()
        .into_table(DefaultPipelines::Table)
        .columns([
            DefaultPipelines::Name,
            DefaultPipelines::States,
            DefaultPipelines::Transitions,
        ])
        .values_panic([
            pipeline.name.as_str().into(),
            states_json.into(),
            transitions_json.into(),
        ])
        .build_rusqlite(SqliteQueryBuilder);
    conn.execute(&sql, &*values.as_params())?;
    Ok(conn.last_insert_rowid())
}

pub fn list_default_pipelines(conn: &Connection) -> Result<Vec<Pipeline>> {
    let (sql, values) = Query::select()
        .columns(DEFAULT_PIPELINE_COLUMNS)
        .from(DefaultPipelines::Table)
        .order_by(DefaultPipelines::Name, Order::Asc)
        .build_rusqlite(SqliteQueryBuilder);
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(&*values.as_params(), row_to_pipeline)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn remove_default_pipeline(conn: &Connection, name: &str) -> Result<u64> {
    let (sql, values) = Query::delete()
        .from_table(DefaultPipelines::Table)
        .and_where(Expr::col(DefaultPipelines::Name).eq(name))
        .build_rusqlite(SqliteQueryBuilder);
    let count = conn.execute(&sql, &*values.as_params())?;
    Ok(count as u64)
}

pub fn migrate_default_pipelines_table(conn: &Connection) -> Result<()> {
    let has_table = conn
        .prepare("SELECT id FROM default_pipelines LIMIT 0")
        .is_ok();
    if has_table {
        return Ok(());
    }
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS default_pipelines (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            states TEXT NOT NULL,
            transitions TEXT NOT NULL
        );",
    )?;
    Ok(())
}

const PIPELINE_COLUMNS: [Pipelines; 4] = [
    Pipelines::Id,
    Pipelines::Name,
    Pipelines::States,
    Pipelines::Transitions,
];

const DEFAULT_PIPELINE_COLUMNS: [DefaultPipelines; 4] = [
    DefaultPipelines::Id,
    DefaultPipelines::Name,
    DefaultPipelines::States,
    DefaultPipelines::Transitions,
];

const ATTACHMENT_COLUMNS: [PipelineAttachments; 4] = [
    PipelineAttachments::Id,
    PipelineAttachments::PipelineId,
    PipelineAttachments::ScopeType,
    PipelineAttachments::ScopeValue,
];

const SIGN_COLUMNS: [Signs; 9] = [
    Signs::Id,
    Signs::PipelineId,
    Signs::FileId,
    Signs::FileHash,
    Signs::SignName,
    Signs::Signer,
    Signs::SignedAt,
    Signs::Signature,
    Signs::RevokedAt,
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

fn row_to_pipeline(row: &rusqlite::Row) -> rusqlite::Result<Pipeline> {
    let states_json: String = row.get(2)?;
    let transitions_json: String = row.get(3)?;

    let states: Vec<String> =
        serde_json::from_str(&states_json).map_err(|e| col_conversion_err(2, e))?;
    let transitions =
        serde_json::from_str(&transitions_json).map_err(|e| col_conversion_err(3, e))?;

    Ok(Pipeline {
        id: Some(row.get(0)?),
        name: row.get(1)?,
        states,
        transitions,
    })
}

fn row_to_attachment(row: &rusqlite::Row) -> rusqlite::Result<PipelineAttachment> {
    let scope_str: String = row.get(2)?;
    let scope_type: AttachmentScope = scope_str.parse().map_err(|e| col_conversion_err(2, e))?;

    Ok(PipelineAttachment {
        id: Some(row.get(0)?),
        pipeline_id: row.get(1)?,
        scope_type,
        scope_value: row.get(3)?,
    })
}

fn row_to_sign(row: &rusqlite::Row) -> rusqlite::Result<Sign> {
    Ok(Sign {
        id: Some(row.get(0)?),
        pipeline_id: row.get(1)?,
        file_id: row.get(2)?,
        file_hash: row.get(3)?,
        sign_name: row.get(4)?,
        signer: row.get(5)?,
        signed_at: row.get(6)?,
        signature: row.get(7)?,
        revoked_at: row.get(8)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::ProjectDb;
    use crate::models::{Category, CategoryType, TrackedFile};
    use tempfile::TempDir;

    fn setup() -> (TempDir, ProjectDb) {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join(".mkrk");
        let db = ProjectDb::create(&db_path).unwrap();
        (dir, db)
    }

    fn make_pipeline(name: &str, states: &[&str]) -> Pipeline {
        let states: Vec<String> = states.iter().map(|s| (*s).to_string()).collect();
        let transitions = Pipeline::default_transitions(&states);
        Pipeline {
            id: None,
            name: name.to_string(),
            states,
            transitions,
        }
    }

    fn make_file(name: &str, path: &str) -> TrackedFile {
        TrackedFile {
            id: None,
            name: name.to_string(),
            path: path.to_string(),
            sha256: Some("abc123".to_string()),
            mime_type: None,
            size: Some(1024),
            ingested_at: "2025-01-01T00:00:00Z".to_string(),
            provenance: None,
            immutable: false,
        }
    }

    #[test]
    fn pipeline_crud() {
        let (_dir, db) = setup();
        let pipeline = make_pipeline("legal-review", &["draft", "review", "published"]);
        let id = db.insert_pipeline(&pipeline).unwrap();
        assert!(id > 0);

        let found = db.get_pipeline_by_name("legal-review").unwrap().unwrap();
        assert_eq!(found.name, "legal-review");
        assert_eq!(found.states, vec!["draft", "review", "published"]);
        assert_eq!(found.transitions["review"], vec!["review"]);
        assert_eq!(found.transitions["published"], vec!["published"]);

        let all = db.list_pipelines().unwrap();
        assert_eq!(all.len(), 1);

        assert_eq!(db.pipeline_count().unwrap(), 1);

        let removed = db.remove_pipeline("legal-review").unwrap();
        assert_eq!(removed, 1);
        assert!(db.list_pipelines().unwrap().is_empty());
    }

    #[test]
    fn pipeline_remove_nonexistent() {
        let (_dir, db) = setup();
        assert_eq!(db.remove_pipeline("ghost").unwrap(), 0);
    }

    #[test]
    fn pipeline_duplicate_name_rejected() {
        let (_dir, db) = setup();
        let p = make_pipeline("dup", &["a", "b"]);
        db.insert_pipeline(&p).unwrap();
        assert!(db.insert_pipeline(&p).is_err());
    }

    #[test]
    fn attachment_crud() {
        let (_dir, db) = setup();
        let p = make_pipeline("review", &["draft", "done"]);
        let pid = db.insert_pipeline(&p).unwrap();

        db.attach_pipeline(pid, AttachmentScope::Category, "evidence")
            .unwrap();
        db.attach_pipeline(pid, AttachmentScope::Tag, "classified")
            .unwrap();

        let attachments = db.list_attachments_for_pipeline(pid).unwrap();
        assert_eq!(attachments.len(), 2);
        assert_eq!(attachments[0].scope_type, AttachmentScope::Category);
        assert_eq!(attachments[0].scope_value, "evidence");
        assert_eq!(attachments[1].scope_type, AttachmentScope::Tag);
        assert_eq!(attachments[1].scope_value, "classified");

        let removed = db
            .detach_pipeline(pid, AttachmentScope::Category, "evidence")
            .unwrap();
        assert_eq!(removed, 1);

        let attachments = db.list_attachments_for_pipeline(pid).unwrap();
        assert_eq!(attachments.len(), 1);
    }

    #[test]
    fn attachment_duplicate_rejected() {
        let (_dir, db) = setup();
        let p = make_pipeline("review", &["draft", "done"]);
        let pid = db.insert_pipeline(&p).unwrap();

        db.attach_pipeline(pid, AttachmentScope::Category, "evidence")
            .unwrap();
        assert!(db
            .attach_pipeline(pid, AttachmentScope::Category, "evidence")
            .is_err());
    }

    #[test]
    fn sign_crud() {
        let (_dir, db) = setup();
        let p = make_pipeline("review", &["draft", "reviewed", "published"]);
        let pid = db.insert_pipeline(&p).unwrap();
        let file = make_file("doc.pdf", "evidence/doc.pdf");
        let fid = db.insert_file(&file).unwrap();

        let sign = Sign {
            id: None,
            pipeline_id: pid,
            file_id: fid,
            file_hash: "abc123".to_string(),
            sign_name: "reviewed".to_string(),
            signer: "alice".to_string(),
            signed_at: "2025-06-01T00:00:00Z".to_string(),
            signature: None,
            revoked_at: None,
        };
        let sid = db.insert_sign(&sign).unwrap();
        assert!(sid > 0);

        let valid = db
            .get_valid_signs_for_file_pipeline(fid, pid, "abc123")
            .unwrap();
        assert_eq!(valid.len(), 1);
        assert_eq!(valid[0].sign_name, "reviewed");

        let stale = db
            .get_valid_signs_for_file_pipeline(fid, pid, "different_hash")
            .unwrap();
        assert!(stale.is_empty());

        let all = db.get_signs_for_file(fid).unwrap();
        assert_eq!(all.len(), 1);

        assert_eq!(db.sign_count().unwrap(), 1);
    }

    #[test]
    fn sign_revoke() {
        let (_dir, db) = setup();
        let p = make_pipeline("review", &["draft", "done"]);
        let pid = db.insert_pipeline(&p).unwrap();
        let file = make_file("doc.pdf", "evidence/doc.pdf");
        let fid = db.insert_file(&file).unwrap();

        let sign = Sign {
            id: None,
            pipeline_id: pid,
            file_id: fid,
            file_hash: "abc123".to_string(),
            sign_name: "done".to_string(),
            signer: "bob".to_string(),
            signed_at: "2025-06-01T00:00:00Z".to_string(),
            signature: None,
            revoked_at: None,
        };
        let sid = db.insert_sign(&sign).unwrap();

        let revoked = db.revoke_sign(sid, "2025-06-02T00:00:00Z").unwrap();
        assert_eq!(revoked, 1);

        let valid = db
            .get_valid_signs_for_file_pipeline(fid, pid, "abc123")
            .unwrap();
        assert!(valid.is_empty());

        assert_eq!(db.sign_count().unwrap(), 0);

        let all = db.get_signs_for_file(fid).unwrap();
        assert_eq!(all.len(), 1);
        assert!(all[0].revoked_at.is_some());
    }

    #[test]
    fn find_sign_returns_latest_active() {
        let (_dir, db) = setup();
        let p = make_pipeline("review", &["draft", "done"]);
        let pid = db.insert_pipeline(&p).unwrap();
        let file = make_file("doc.pdf", "evidence/doc.pdf");
        let fid = db.insert_file(&file).unwrap();

        let sign1 = Sign {
            id: None,
            pipeline_id: pid,
            file_id: fid,
            file_hash: "abc123".to_string(),
            sign_name: "done".to_string(),
            signer: "alice".to_string(),
            signed_at: "2025-06-01T00:00:00Z".to_string(),
            signature: None,
            revoked_at: None,
        };
        db.insert_sign(&sign1).unwrap();

        let found = db.find_sign(fid, pid, "done").unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().signer, "alice");

        let not_found = db.find_sign(fid, pid, "nonexistent").unwrap();
        assert!(not_found.is_none());
    }

    #[test]
    fn remove_pipeline_cascades_signs_and_attachments() {
        let (_dir, db) = setup();
        let p = make_pipeline("review", &["draft", "done"]);
        let pid = db.insert_pipeline(&p).unwrap();
        let file = make_file("doc.pdf", "evidence/doc.pdf");
        let fid = db.insert_file(&file).unwrap();

        db.attach_pipeline(pid, AttachmentScope::Category, "evidence")
            .unwrap();

        let sign = Sign {
            id: None,
            pipeline_id: pid,
            file_id: fid,
            file_hash: "abc123".to_string(),
            sign_name: "done".to_string(),
            signer: "alice".to_string(),
            signed_at: "2025-06-01T00:00:00Z".to_string(),
            signature: None,
            revoked_at: None,
        };
        db.insert_sign(&sign).unwrap();

        db.remove_pipeline("review").unwrap();
        assert_eq!(db.sign_count().unwrap(), 0);
        assert_eq!(db.pipeline_count().unwrap(), 0);
    }

    #[test]
    fn get_pipelines_for_file_by_category() {
        let (_dir, db) = setup();
        let p = make_pipeline("review", &["draft", "done"]);
        let pid = db.insert_pipeline(&p).unwrap();

        db.attach_pipeline(pid, AttachmentScope::Category, "evidence")
            .unwrap();

        let cat = Category {
            id: Some(1),
            name: "evidence".to_string(),
            pattern: "evidence/**".to_string(),
            category_type: CategoryType::Files,
            description: None,
        };

        let pipelines = db
            .get_pipelines_for_file(1, "evidence/doc.pdf", &[cat.clone()], &[])
            .unwrap();
        assert_eq!(pipelines.len(), 1);
        assert_eq!(pipelines[0].name, "review");

        let no_match = db
            .get_pipelines_for_file(1, "notes/doc.pdf", &[cat], &[])
            .unwrap();
        assert!(no_match.is_empty());
    }

    #[test]
    fn get_pipelines_for_file_by_tag() {
        let (_dir, db) = setup();
        let p = make_pipeline("classification", &["unclassified", "classified"]);
        let pid = db.insert_pipeline(&p).unwrap();

        db.attach_pipeline(pid, AttachmentScope::Tag, "secret")
            .unwrap();

        let tags = vec!["secret".to_string()];
        let pipelines = db
            .get_pipelines_for_file(1, "anything.pdf", &[], &tags)
            .unwrap();
        assert_eq!(pipelines.len(), 1);
        assert_eq!(pipelines[0].name, "classification");
    }

    #[test]
    fn get_pipelines_for_file_deduplicates() {
        let (_dir, db) = setup();
        let p = make_pipeline("review", &["draft", "done"]);
        let pid = db.insert_pipeline(&p).unwrap();

        db.attach_pipeline(pid, AttachmentScope::Category, "evidence")
            .unwrap();
        db.attach_pipeline(pid, AttachmentScope::Tag, "important")
            .unwrap();

        let cat = Category {
            id: Some(1),
            name: "evidence".to_string(),
            pattern: "evidence/**".to_string(),
            category_type: CategoryType::Files,
            description: None,
        };
        let tags = vec!["important".to_string()];

        let pipelines = db
            .get_pipelines_for_file(1, "evidence/doc.pdf", &[cat], &tags)
            .unwrap();
        assert_eq!(pipelines.len(), 1);
    }
}
