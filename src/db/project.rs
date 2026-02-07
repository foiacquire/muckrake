use std::path::Path;

use anyhow::{Context, Result};
use chrono::Utc;
use rusqlite::{params, Connection};

use crate::models::{Category, FileTag, ProtectionLevel, TrackedFile};

use super::schema::PROJECT_SCHEMA;

pub struct ProjectDb {
    conn: Connection,
}

impl ProjectDb {
    pub fn create(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)
            .with_context(|| format!("failed to create project db at {}", path.display()))?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        conn.execute_batch(PROJECT_SCHEMA)?;
        Ok(Self { conn })
    }

    pub fn open(path: &Path) -> Result<Self> {
        if !path.exists() {
            anyhow::bail!("project database not found: {}", path.display());
        }
        let conn = Connection::open(path)
            .with_context(|| format!("failed to open project db at {}", path.display()))?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        Ok(Self { conn })
    }

    pub fn insert_category(&self, cat: &Category) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO categories (pattern, protection_level, description) VALUES (?1, ?2, ?3)",
            params![
                cat.pattern,
                cat.protection_level.to_string(),
                cat.description
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn list_categories(&self) -> Result<Vec<Category>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, pattern, protection_level, description FROM categories")?;
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

    pub fn match_category(&self, rel_path: &str) -> Result<Option<Category>> {
        let categories = self.list_categories()?;
        let mut best: Option<&Category> = None;
        let mut best_specificity = 0usize;

        for cat in &categories {
            if cat.matches(rel_path) {
                let specificity = cat.pattern.len();
                if specificity > best_specificity {
                    best = Some(cat);
                    best_specificity = specificity;
                }
            }
        }

        Ok(best.cloned())
    }

    pub fn insert_file(&self, file: &TrackedFile) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO files (name, path, sha256, mime_type, size, ingested_at, provenance, immutable)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                file.name,
                file.path,
                file.sha256,
                file.mime_type,
                file.size,
                file.ingested_at,
                file.provenance,
                i32::from(file.immutable),
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn get_file_by_name(&self, name: &str) -> Result<Option<TrackedFile>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, path, sha256, mime_type, size, ingested_at, provenance, immutable
             FROM files WHERE name = ?1",
        )?;
        let mut rows = stmt.query_map(params![name], row_to_file)?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    pub fn get_file_by_path(&self, path: &str) -> Result<Option<TrackedFile>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, path, sha256, mime_type, size, ingested_at, provenance, immutable
             FROM files WHERE path = ?1",
        )?;
        let mut rows = stmt.query_map(params![path], row_to_file)?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    pub fn list_files(&self, path_prefix: Option<&str>) -> Result<Vec<TrackedFile>> {
        if let Some(prefix) = path_prefix {
            let pattern = format!("{prefix}%");
            let mut stmt = self.conn.prepare(
                "SELECT id, name, path, sha256, mime_type, size, ingested_at, provenance, immutable
                 FROM files WHERE path LIKE ?1 ORDER BY path",
            )?;
            let rows = stmt.query_map(params![pattern], row_to_file)?;
            rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
        } else {
            let mut stmt = self.conn.prepare(
                "SELECT id, name, path, sha256, mime_type, size, ingested_at, provenance, immutable
                 FROM files ORDER BY path",
            )?;
            let rows = stmt.query_map([], row_to_file)?;
            rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
        }
    }

    pub fn list_files_by_tag(&self, tag: &str) -> Result<Vec<TrackedFile>> {
        let mut stmt = self.conn.prepare(
            "SELECT f.id, f.name, f.path, f.sha256, f.mime_type, f.size, f.ingested_at, f.provenance, f.immutable
             FROM files f
             JOIN file_tags ft ON f.id = ft.file_id
             WHERE ft.tag = ?1
             ORDER BY f.path",
        )?;
        let rows = stmt.query_map(params![tag], row_to_file)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn update_file_path(&self, file_id: i64, new_path: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE files SET path = ?1 WHERE id = ?2",
            params![new_path, file_id],
        )?;
        Ok(())
    }

    pub fn update_file_immutable(&self, file_id: i64, immutable: bool) -> Result<()> {
        self.conn.execute(
            "UPDATE files SET immutable = ?1 WHERE id = ?2",
            params![i32::from(immutable), file_id],
        )?;
        Ok(())
    }

    pub fn insert_tag(&self, file_id: i64, tag: &str) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO file_tags (file_id, tag) VALUES (?1, ?2)",
            params![file_id, tag],
        )?;
        Ok(())
    }

    pub fn remove_tag(&self, file_id: i64, tag: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM file_tags WHERE file_id = ?1 AND tag = ?2",
            params![file_id, tag],
        )?;
        Ok(())
    }

    pub fn get_tags(&self, file_id: i64) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT tag FROM file_tags WHERE file_id = ?1 ORDER BY tag")?;
        let rows = stmt.query_map(params![file_id], |row| row.get(0))?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn list_all_tags(&self) -> Result<Vec<FileTag>> {
        let mut stmt = self
            .conn
            .prepare("SELECT file_id, tag FROM file_tags ORDER BY tag")?;
        let rows = stmt.query_map([], |row| {
            Ok(FileTag {
                file_id: row.get(0)?,
                tag: row.get(1)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn insert_audit(
        &self,
        operation: &str,
        file_id: Option<i64>,
        user: Option<&str>,
        detail: Option<&str>,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO audit_log (timestamp, operation, file_id, user, detail) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![now, operation, file_id, user, detail],
        )?;
        Ok(())
    }

    pub fn get_tool_config(
        &self,
        scope: Option<&str>,
        action: &str,
        file_type: &str,
    ) -> Result<Option<ToolConfigRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, scope, action, file_type, command, env FROM tool_config
             WHERE (scope = ?1 OR scope IS NULL) AND action = ?2 AND (file_type = ?3 OR file_type = '*')
             ORDER BY
                CASE WHEN scope IS NOT NULL THEN 0 ELSE 1 END,
                CASE WHEN file_type = '*' THEN 1 ELSE 0 END
             LIMIT 1",
        )?;
        let mut rows = stmt.query_map(params![scope, action, file_type], |row| {
            Ok(ToolConfigRow {
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
    ) -> Result<Vec<TagToolConfigRow>> {
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
            Ok(TagToolConfigRow {
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

    pub fn file_count(&self) -> Result<i64> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0))?)
    }

    pub fn category_count(&self) -> Result<i64> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM categories", [], |row| row.get(0))?)
    }

    pub fn tag_count(&self) -> Result<i64> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(DISTINCT tag) FROM file_tags", [], |row| {
                row.get(0)
            })?)
    }

    pub fn last_verify_time(&self) -> Result<Option<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT timestamp FROM audit_log WHERE operation = 'verify' ORDER BY timestamp DESC LIMIT 1",
        )?;
        let mut rows = stmt.query_map([], |row| row.get(0))?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }
}

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
}

#[derive(Debug, Clone)]
pub struct TagToolConfigRow {
    pub id: i64,
    pub tag: String,
    pub action: String,
    pub file_type: String,
    pub command: String,
    pub env: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
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
            pattern: "evidence/**".to_string(),
            protection_level: ProtectionLevel::Immutable,
            description: Some("Evidence files".to_string()),
        };
        db.insert_category(&cat).unwrap();
        let cats = db.list_categories().unwrap();
        assert_eq!(cats.len(), 1);
        assert_eq!(cats[0].pattern, "evidence/**");
        assert_eq!(cats[0].protection_level, ProtectionLevel::Immutable);
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

        db.insert_tag(file_id, "speech").unwrap();
        db.insert_tag(file_id, "rf").unwrap();

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
    fn match_category_most_specific() {
        let (_dir, db) = setup();
        db.insert_category(&Category {
            id: None,
            pattern: "evidence/**".to_string(),
            protection_level: ProtectionLevel::Immutable,
            description: None,
        })
        .unwrap();
        db.insert_category(&Category {
            id: None,
            pattern: "evidence/financial/**".to_string(),
            protection_level: ProtectionLevel::Protected,
            description: None,
        })
        .unwrap();

        let cat = db
            .match_category("evidence/financial/receipt.pdf")
            .unwrap()
            .unwrap();
        assert_eq!(cat.protection_level, ProtectionLevel::Protected);

        let cat = db.match_category("evidence/photo.jpg").unwrap().unwrap();
        assert_eq!(cat.protection_level, ProtectionLevel::Immutable);
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
}
