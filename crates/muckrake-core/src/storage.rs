use chrono::{DateTime, Utc};
use sqlx::{sqlite::SqlitePoolOptions, Pool, Sqlite};
use uuid::Uuid;

use crate::{
    entity::{Entity, EntityAlias, EntityType},
    relationship::{Evidence, Relationship},
    source::{ImportLog, Source, SourceType},
    Error, Result,
};

const SCHEMA_VERSION: i32 = 2;

const INIT_SQL: &str = r#"
-- Project metadata
CREATE TABLE IF NOT EXISTS project_meta (
    key TEXT PRIMARY KEY,
    value TEXT
);

-- Entities: nodes in the graph
CREATE TABLE IF NOT EXISTS entities (
    id TEXT PRIMARY KEY,
    entity_type TEXT NOT NULL,
    canonical_name TEXT NOT NULL,
    data TEXT NOT NULL,
    confidence REAL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_entities_type ON entities(entity_type);
CREATE INDEX IF NOT EXISTS idx_entities_name ON entities(canonical_name);

-- Entity aliases for deduplication
CREATE TABLE IF NOT EXISTS entity_aliases (
    id TEXT PRIMARY KEY,
    entity_id TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    alias TEXT NOT NULL,
    alias_type TEXT,
    source_id TEXT REFERENCES sources(id)
);

CREATE INDEX IF NOT EXISTS idx_aliases_entity ON entity_aliases(entity_id);
CREATE INDEX IF NOT EXISTS idx_aliases_alias ON entity_aliases(alias);
CREATE UNIQUE INDEX IF NOT EXISTS idx_aliases_unique ON entity_aliases(entity_id, alias);

-- Entity attributes with temporal validity
CREATE TABLE IF NOT EXISTS entity_attributes (
    id TEXT PRIMARY KEY,
    entity_id TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    key TEXT NOT NULL,
    value TEXT,
    valid_from TEXT,
    valid_to TEXT,
    source_id TEXT REFERENCES sources(id),
    confidence REAL DEFAULT 1.0,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_attr_entity ON entity_attributes(entity_id);
CREATE INDEX IF NOT EXISTS idx_attr_key ON entity_attributes(key);
CREATE INDEX IF NOT EXISTS idx_attr_valid ON entity_attributes(valid_from, valid_to);

-- Relationships: edges in the graph
CREATE TABLE IF NOT EXISTS relationships (
    id TEXT PRIMARY KEY,
    source_id TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    target_id TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    relation_type TEXT NOT NULL,
    valid_from TEXT,
    valid_to TEXT,
    confidence REAL,
    data TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_rel_source ON relationships(source_id);
CREATE INDEX IF NOT EXISTS idx_rel_target ON relationships(target_id);
CREATE INDEX IF NOT EXISTS idx_rel_type ON relationships(relation_type);
CREATE INDEX IF NOT EXISTS idx_rel_valid ON relationships(valid_from, valid_to);

-- Sources: documents, URLs, etc.
CREATE TABLE IF NOT EXISTS sources (
    id TEXT PRIMARY KEY,
    source_type TEXT NOT NULL,
    title TEXT,
    uri TEXT,
    content_hash TEXT,
    metadata TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_sources_hash ON sources(content_hash);
CREATE INDEX IF NOT EXISTS idx_sources_uri ON sources(uri);

-- Evidence: links between entities/relationships and sources
CREATE TABLE IF NOT EXISTS evidence (
    id TEXT PRIMARY KEY,
    source_id TEXT REFERENCES sources(id),
    entity_id TEXT REFERENCES entities(id) ON DELETE CASCADE,
    relationship_id TEXT REFERENCES relationships(id) ON DELETE CASCADE,
    excerpt TEXT,
    page_number INTEGER,
    location TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    CHECK (entity_id IS NOT NULL OR relationship_id IS NOT NULL)
);

CREATE INDEX IF NOT EXISTS idx_evidence_source ON evidence(source_id);
CREATE INDEX IF NOT EXISTS idx_evidence_entity ON evidence(entity_id);
CREATE INDEX IF NOT EXISTS idx_evidence_relationship ON evidence(relationship_id);

-- Import log for tracking ingested files
CREATE TABLE IF NOT EXISTS import_log (
    id TEXT PRIMARY KEY,
    source_uri TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    imported_at TEXT NOT NULL,
    entity_count INTEGER DEFAULT 0,
    relationship_count INTEGER DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_import_hash ON import_log(content_hash);
CREATE INDEX IF NOT EXISTS idx_import_uri ON import_log(source_uri);

-- Full-text search for entities
CREATE VIRTUAL TABLE IF NOT EXISTS entities_fts USING fts5(
    canonical_name,
    content='entities',
    content_rowid='rowid'
);

CREATE TRIGGER IF NOT EXISTS entities_ai AFTER INSERT ON entities BEGIN
    INSERT INTO entities_fts(rowid, canonical_name) VALUES (NEW.rowid, NEW.canonical_name);
END;

CREATE TRIGGER IF NOT EXISTS entities_ad AFTER DELETE ON entities BEGIN
    INSERT INTO entities_fts(entities_fts, rowid, canonical_name) VALUES('delete', OLD.rowid, OLD.canonical_name);
END;

CREATE TRIGGER IF NOT EXISTS entities_au AFTER UPDATE ON entities BEGIN
    INSERT INTO entities_fts(entities_fts, rowid, canonical_name) VALUES('delete', OLD.rowid, OLD.canonical_name);
    INSERT INTO entities_fts(rowid, canonical_name) VALUES (NEW.rowid, NEW.canonical_name);
END;

-- Full-text search for aliases
CREATE VIRTUAL TABLE IF NOT EXISTS aliases_fts USING fts5(
    alias,
    content='entity_aliases',
    content_rowid='rowid'
);

CREATE TRIGGER IF NOT EXISTS aliases_ai AFTER INSERT ON entity_aliases BEGIN
    INSERT INTO aliases_fts(rowid, alias) VALUES (NEW.rowid, NEW.alias);
END;

CREATE TRIGGER IF NOT EXISTS aliases_ad AFTER DELETE ON entity_aliases BEGIN
    INSERT INTO aliases_fts(aliases_fts, rowid, alias) VALUES('delete', OLD.rowid, OLD.alias);
END;

-- Full-text search for source content
CREATE VIRTUAL TABLE IF NOT EXISTS source_content_fts USING fts5(
    content
);
"#;

pub struct Storage {
    pool: Pool<Sqlite>,
}

impl Storage {
    pub async fn open(path: &str) -> Result<Self> {
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&format!("sqlite:{path}?mode=rwc"))
            .await?;

        sqlx::raw_sql(INIT_SQL).execute(&pool).await?;

        let storage = Self { pool };
        storage.init_project_meta().await?;
        Ok(storage)
    }

    pub async fn open_memory() -> Result<Self> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await?;

        sqlx::raw_sql(INIT_SQL).execute(&pool).await?;

        let storage = Self { pool };
        storage.init_project_meta().await?;
        Ok(storage)
    }

    async fn init_project_meta(&self) -> Result<()> {
        sqlx::query(
            r#"
            INSERT OR IGNORE INTO project_meta (key, value) VALUES ('schema_version', ?)
            "#,
        )
        .bind(SCHEMA_VERSION.to_string())
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_schema_version(&self) -> Result<i32> {
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT value FROM project_meta WHERE key = 'schema_version'",
        )
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some((v,)) => Ok(v.parse().unwrap_or(1)),
            None => Ok(1),
        }
    }

    // Entity operations

    pub async fn insert_entity(&self, entity: &Entity) -> Result<()> {
        let data_json = serde_json::to_string(&entity.data)?;

        sqlx::query(
            r#"
            INSERT INTO entities (id, entity_type, canonical_name, data, confidence, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(entity.id.to_string())
        .bind(entity.entity_type().as_str())
        .bind(&entity.canonical_name)
        .bind(data_json)
        .bind(entity.confidence)
        .bind(entity.created_at.to_rfc3339())
        .bind(entity.updated_at.to_rfc3339())
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_entity(&self, id: Uuid) -> Result<Entity> {
        let row: (String, String, String, String, Option<f64>, String, String) = sqlx::query_as(
            r#"
            SELECT id, entity_type, canonical_name, data, confidence, created_at, updated_at
            FROM entities WHERE id = ?
            "#,
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await?
        .ok_or(Error::EntityNotFound(id))?;

        parse_entity_row(row)
    }

    pub async fn update_entity(&self, entity: &Entity) -> Result<()> {
        let data_json = serde_json::to_string(&entity.data)?;

        let result = sqlx::query(
            r#"
            UPDATE entities
            SET canonical_name = ?, data = ?, confidence = ?, updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(&entity.canonical_name)
        .bind(data_json)
        .bind(entity.confidence)
        .bind(Utc::now().to_rfc3339())
        .bind(entity.id.to_string())
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(Error::EntityNotFound(entity.id));
        }

        Ok(())
    }

    pub async fn delete_entity(&self, id: Uuid) -> Result<()> {
        let result = sqlx::query("DELETE FROM entities WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;

        if result.rows_affected() == 0 {
            return Err(Error::EntityNotFound(id));
        }

        Ok(())
    }

    pub async fn list_entities(&self, entity_type: Option<EntityType>) -> Result<Vec<Entity>> {
        let rows: Vec<(String, String, String, String, Option<f64>, String, String)> =
            match entity_type {
                Some(t) => {
                    sqlx::query_as(
                        r#"
                    SELECT id, entity_type, canonical_name, data, confidence, created_at, updated_at
                    FROM entities WHERE entity_type = ? ORDER BY canonical_name
                    "#,
                    )
                    .bind(t.as_str())
                    .fetch_all(&self.pool)
                    .await?
                }
                None => {
                    sqlx::query_as(
                        r#"
                    SELECT id, entity_type, canonical_name, data, confidence, created_at, updated_at
                    FROM entities ORDER BY canonical_name
                    "#,
                    )
                    .fetch_all(&self.pool)
                    .await?
                }
            };

        rows.into_iter().map(parse_entity_row).collect()
    }

    pub async fn search_entities(&self, query: &str) -> Result<Vec<Entity>> {
        let rows: Vec<(String, String, String, String, Option<f64>, String, String)> =
            sqlx::query_as(
                r#"
            SELECT e.id, e.entity_type, e.canonical_name, e.data, e.confidence, e.created_at, e.updated_at
            FROM entities e
            JOIN entities_fts ON entities_fts.rowid = e.rowid
            WHERE entities_fts MATCH ?
            ORDER BY rank
            "#,
            )
            .bind(query)
            .fetch_all(&self.pool)
            .await?;

        rows.into_iter().map(parse_entity_row).collect()
    }

    // Alias operations

    pub async fn insert_alias(&self, alias: &EntityAlias) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO entity_aliases (id, entity_id, alias, source)
            VALUES (?, ?, ?, ?)
            "#,
        )
        .bind(alias.id.to_string())
        .bind(alias.entity_id.to_string())
        .bind(&alias.alias)
        .bind(&alias.source)
        .execute(&self.pool)
        .await
        .map_err(|e| {
            if let sqlx::Error::Database(ref db_err) = e {
                if db_err.is_unique_violation() {
                    return Error::DuplicateAlias {
                        entity_id: alias.entity_id,
                        alias: alias.alias.clone(),
                    };
                }
            }
            Error::Database(e)
        })?;

        Ok(())
    }

    pub async fn get_aliases(&self, entity_id: Uuid) -> Result<Vec<EntityAlias>> {
        let rows: Vec<(String, String, String, Option<String>)> = sqlx::query_as(
            r#"
            SELECT id, entity_id, alias, source
            FROM entity_aliases WHERE entity_id = ?
            "#,
        )
        .bind(entity_id.to_string())
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(parse_alias_row).collect()
    }

    pub async fn find_by_alias(&self, alias: &str) -> Result<Vec<Entity>> {
        let rows: Vec<(String, String, String, String, Option<f64>, String, String)> =
            sqlx::query_as(
                r#"
            SELECT e.id, e.entity_type, e.canonical_name, e.data, e.confidence, e.created_at, e.updated_at
            FROM entities e
            JOIN entity_aliases a ON a.entity_id = e.id
            WHERE a.alias = ?
            "#,
            )
            .bind(alias)
            .fetch_all(&self.pool)
            .await?;

        rows.into_iter().map(parse_entity_row).collect()
    }

    // Relationship operations

    pub async fn insert_relationship(&self, rel: &Relationship) -> Result<()> {
        let data_json = serde_json::to_string(&rel.data)?;

        sqlx::query(
            r#"
            INSERT INTO relationships (id, source_id, target_id, relation_type, valid_from, valid_to, confidence, data, created_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(rel.id.to_string())
        .bind(rel.source_id.to_string())
        .bind(rel.target_id.to_string())
        .bind(rel.relation_type.as_str())
        .bind(rel.valid_from.map(|d| d.to_rfc3339()))
        .bind(rel.valid_to.map(|d| d.to_rfc3339()))
        .bind(rel.confidence)
        .bind(data_json)
        .bind(rel.created_at.to_rfc3339())
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_relationship(&self, id: Uuid) -> Result<Relationship> {
        let row: RelationshipRow = sqlx::query_as(
            r#"
            SELECT id, source_id, target_id, relation_type, valid_from, valid_to, confidence, data, created_at
            FROM relationships WHERE id = ?
            "#,
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await?
        .ok_or(Error::RelationshipNotFound(id))?;

        parse_relationship_row(row)
    }

    pub async fn get_entity_relationships(&self, entity_id: Uuid) -> Result<Vec<Relationship>> {
        let rows: Vec<RelationshipRow> = sqlx::query_as(
            r#"
            SELECT id, source_id, target_id, relation_type, valid_from, valid_to, confidence, data, created_at
            FROM relationships
            WHERE source_id = ? OR target_id = ?
            ORDER BY created_at DESC
            "#,
        )
        .bind(entity_id.to_string())
        .bind(entity_id.to_string())
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(parse_relationship_row).collect()
    }

    pub async fn get_relationships_at(&self, entity_id: Uuid, at: DateTime<Utc>) -> Result<Vec<Relationship>> {
        let at_str = at.to_rfc3339();
        let rows: Vec<RelationshipRow> = sqlx::query_as(
            r#"
            SELECT id, source_id, target_id, relation_type, valid_from, valid_to, confidence, data, created_at
            FROM relationships
            WHERE (source_id = ? OR target_id = ?)
              AND (valid_from IS NULL OR valid_from <= ?)
              AND (valid_to IS NULL OR valid_to > ?)
            ORDER BY created_at DESC
            "#,
        )
        .bind(entity_id.to_string())
        .bind(entity_id.to_string())
        .bind(&at_str)
        .bind(&at_str)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(parse_relationship_row).collect()
    }

    pub async fn delete_relationship(&self, id: Uuid) -> Result<()> {
        let result = sqlx::query("DELETE FROM relationships WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;

        if result.rows_affected() == 0 {
            return Err(Error::RelationshipNotFound(id));
        }

        Ok(())
    }

    pub async fn list_relationships(&self) -> Result<Vec<Relationship>> {
        let rows: Vec<RelationshipRow> = sqlx::query_as(
            r#"
            SELECT id, source_id, target_id, relation_type, valid_from, valid_to, confidence, data, created_at
            FROM relationships
            ORDER BY created_at DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(parse_relationship_row).collect()
    }

    // Evidence operations

    pub async fn insert_evidence(&self, evidence: &Evidence) -> Result<()> {
        let location_json = evidence.location.as_ref().map(|l| serde_json::to_string(l).ok()).flatten();

        sqlx::query(
            r#"
            INSERT INTO evidence (id, source_id, entity_id, relationship_id, excerpt, page_number, location, created_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(evidence.id.to_string())
        .bind(evidence.source_id.map(|id| id.to_string()))
        .bind(evidence.entity_id.map(|id| id.to_string()))
        .bind(evidence.relationship_id.map(|id| id.to_string()))
        .bind(&evidence.excerpt)
        .bind(evidence.page_number.map(|n| i64::from(n)))
        .bind(location_json)
        .bind(evidence.created_at.to_rfc3339())
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_entity_evidence(&self, entity_id: Uuid) -> Result<Vec<Evidence>> {
        let rows: Vec<EvidenceRow> = sqlx::query_as(
            r#"
            SELECT id, source_id, entity_id, relationship_id, excerpt, page_number, location, created_at
            FROM evidence WHERE entity_id = ?
            "#,
        )
        .bind(entity_id.to_string())
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(parse_evidence_row).collect()
    }

    pub async fn get_relationship_evidence(&self, relationship_id: Uuid) -> Result<Vec<Evidence>> {
        let rows: Vec<EvidenceRow> = sqlx::query_as(
            r#"
            SELECT id, source_id, entity_id, relationship_id, excerpt, page_number, location, created_at
            FROM evidence WHERE relationship_id = ?
            "#,
        )
        .bind(relationship_id.to_string())
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(parse_evidence_row).collect()
    }

    // Source operations

    pub async fn insert_source(&self, source: &Source) -> Result<()> {
        let metadata_json = serde_json::to_string(&source.metadata)?;

        sqlx::query(
            r#"
            INSERT INTO sources (id, source_type, title, uri, content_hash, metadata, created_at)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(source.id.to_string())
        .bind(source.source_type.as_str())
        .bind(&source.title)
        .bind(&source.uri)
        .bind(&source.content_hash)
        .bind(metadata_json)
        .bind(source.created_at.to_rfc3339())
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_source(&self, id: Uuid) -> Result<Source> {
        let row: SourceRow = sqlx::query_as(
            r#"
            SELECT id, source_type, title, uri, content_hash, metadata, created_at
            FROM sources WHERE id = ?
            "#,
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await?
        .ok_or(Error::SourceNotFound(id))?;

        parse_source_row(row)
    }

    pub async fn find_source_by_hash(&self, hash: &str) -> Result<Option<Source>> {
        let row: Option<SourceRow> = sqlx::query_as(
            r#"
            SELECT id, source_type, title, uri, content_hash, metadata, created_at
            FROM sources WHERE content_hash = ?
            "#,
        )
        .bind(hash)
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some(r) => Ok(Some(parse_source_row(r)?)),
            None => Ok(None),
        }
    }

    pub async fn list_sources(&self, source_type: Option<SourceType>) -> Result<Vec<Source>> {
        let rows: Vec<SourceRow> = match source_type {
            Some(t) => {
                sqlx::query_as(
                    r#"
                    SELECT id, source_type, title, uri, content_hash, metadata, created_at
                    FROM sources WHERE source_type = ? ORDER BY created_at DESC
                    "#,
                )
                .bind(t.as_str())
                .fetch_all(&self.pool)
                .await?
            }
            None => {
                sqlx::query_as(
                    r#"
                    SELECT id, source_type, title, uri, content_hash, metadata, created_at
                    FROM sources ORDER BY created_at DESC
                    "#,
                )
                .fetch_all(&self.pool)
                .await?
            }
        };

        rows.into_iter().map(parse_source_row).collect()
    }

    pub async fn delete_source(&self, id: Uuid) -> Result<()> {
        let result = sqlx::query("DELETE FROM sources WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;

        if result.rows_affected() == 0 {
            return Err(Error::SourceNotFound(id));
        }

        Ok(())
    }

    // Import log operations

    pub async fn insert_import_log(&self, log: &ImportLog) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO import_log (id, source_uri, content_hash, imported_at, entity_count, relationship_count)
            VALUES (?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(log.id.to_string())
        .bind(&log.source_uri)
        .bind(&log.content_hash)
        .bind(log.imported_at.to_rfc3339())
        .bind(i64::from(log.entity_count))
        .bind(i64::from(log.relationship_count))
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn find_import_by_hash(&self, hash: &str) -> Result<Option<ImportLog>> {
        let row: Option<ImportLogRow> = sqlx::query_as(
            r#"
            SELECT id, source_uri, content_hash, imported_at, entity_count, relationship_count
            FROM import_log WHERE content_hash = ?
            "#,
        )
        .bind(hash)
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some(r) => Ok(Some(parse_import_log_row(r)?)),
            None => Ok(None),
        }
    }

    pub async fn find_import_by_uri(&self, uri: &str) -> Result<Option<ImportLog>> {
        let row: Option<ImportLogRow> = sqlx::query_as(
            r#"
            SELECT id, source_uri, content_hash, imported_at, entity_count, relationship_count
            FROM import_log WHERE source_uri = ?
            ORDER BY imported_at DESC LIMIT 1
            "#,
        )
        .bind(uri)
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some(r) => Ok(Some(parse_import_log_row(r)?)),
            None => Ok(None),
        }
    }

    /// Save the database to a file. Works for both in-memory and file-based databases.
    pub async fn save_to(&self, path: &str) -> Result<()> {
        // Remove existing file if present to avoid "database already exists" error
        if std::path::Path::new(path).exists() {
            std::fs::remove_file(path)?;
        }

        // VACUUM INTO creates a clean copy of the database
        sqlx::query(&format!("VACUUM INTO '{}'", path.replace('\'', "''")))
            .execute(&self.pool)
            .await?;

        Ok(())
    }
}

fn parse_entity_row(
    row: (String, String, String, String, Option<f64>, String, String),
) -> Result<Entity> {
    let (id, _entity_type, canonical_name, data_json, confidence, created_at, updated_at) = row;

    Ok(Entity {
        id: id.parse().map_err(|_| Error::EntityNotFound(Uuid::nil()))?,
        canonical_name,
        data: serde_json::from_str(&data_json)?,
        confidence,
        created_at: DateTime::parse_from_rfc3339(&created_at)
            .map_err(|_| Error::EntityNotFound(Uuid::nil()))?
            .with_timezone(&Utc),
        updated_at: DateTime::parse_from_rfc3339(&updated_at)
            .map_err(|_| Error::EntityNotFound(Uuid::nil()))?
            .with_timezone(&Utc),
    })
}

fn parse_alias_row(row: (String, String, String, Option<String>)) -> Result<EntityAlias> {
    let (id, entity_id, alias, source) = row;

    Ok(EntityAlias {
        id: id.parse().map_err(|_| Error::EntityNotFound(Uuid::nil()))?,
        entity_id: entity_id
            .parse()
            .map_err(|_| Error::EntityNotFound(Uuid::nil()))?,
        alias,
        source,
    })
}

type RelationshipRow = (
    String,         // id
    String,         // source_id
    String,         // target_id
    String,         // relation_type
    Option<String>, // valid_from
    Option<String>, // valid_to
    Option<f64>,    // confidence
    String,         // data
    String,         // created_at
);

fn parse_relationship_row(row: RelationshipRow) -> Result<Relationship> {
    let (id, source_id, target_id, relation_type, valid_from, valid_to, confidence, data_json, created_at) = row;

    let parse_datetime = |s: String| -> Option<DateTime<Utc>> {
        DateTime::parse_from_rfc3339(&s).ok().map(|d| d.with_timezone(&Utc))
    };

    Ok(Relationship {
        id: id
            .parse()
            .map_err(|_| Error::RelationshipNotFound(Uuid::nil()))?,
        source_id: source_id
            .parse()
            .map_err(|_| Error::RelationshipNotFound(Uuid::nil()))?,
        target_id: target_id
            .parse()
            .map_err(|_| Error::RelationshipNotFound(Uuid::nil()))?,
        relation_type: relation_type.parse()?,
        valid_from: valid_from.and_then(parse_datetime),
        valid_to: valid_to.and_then(parse_datetime),
        confidence,
        data: serde_json::from_str(&data_json).unwrap_or_default(),
        created_at: DateTime::parse_from_rfc3339(&created_at)
            .map_err(|_| Error::RelationshipNotFound(Uuid::nil()))?
            .with_timezone(&Utc),
    })
}

type EvidenceRow = (
    String,         // id
    Option<String>, // source_id
    Option<String>, // entity_id
    Option<String>, // relationship_id
    Option<String>, // excerpt
    Option<i64>,    // page_number
    Option<String>, // location
    String,         // created_at
);

fn parse_evidence_row(row: EvidenceRow) -> Result<Evidence> {
    let (id, source_id, entity_id, relationship_id, excerpt, page_number, location_json, created_at) = row;

    Ok(Evidence {
        id: id.parse().map_err(|_| Error::EntityNotFound(Uuid::nil()))?,
        source_id: source_id.and_then(|s| s.parse().ok()),
        entity_id: entity_id.and_then(|s| s.parse().ok()),
        relationship_id: relationship_id.and_then(|s| s.parse().ok()),
        excerpt,
        page_number: page_number.and_then(|n| u32::try_from(n).ok()),
        location: location_json.and_then(|s| serde_json::from_str(&s).ok()),
        created_at: DateTime::parse_from_rfc3339(&created_at)
            .map_err(|_| Error::EntityNotFound(Uuid::nil()))?
            .with_timezone(&Utc),
    })
}

type SourceRow = (
    String,         // id
    String,         // source_type
    Option<String>, // title
    Option<String>, // uri
    Option<String>, // content_hash
    String,         // metadata
    String,         // created_at
);

fn parse_source_row(row: SourceRow) -> Result<Source> {
    let (id, source_type, title, uri, content_hash, metadata_json, created_at) = row;

    Ok(Source {
        id: id.parse().map_err(|_| Error::SourceNotFound(Uuid::nil()))?,
        source_type: source_type.parse()?,
        title,
        uri,
        content_hash,
        metadata: serde_json::from_str(&metadata_json).unwrap_or_default(),
        created_at: DateTime::parse_from_rfc3339(&created_at)
            .map_err(|_| Error::SourceNotFound(Uuid::nil()))?
            .with_timezone(&Utc),
    })
}

type ImportLogRow = (
    String, // id
    String, // source_uri
    String, // content_hash
    String, // imported_at
    i64,    // entity_count
    i64,    // relationship_count
);

fn parse_import_log_row(row: ImportLogRow) -> Result<ImportLog> {
    let (id, source_uri, content_hash, imported_at, entity_count, relationship_count) = row;

    Ok(ImportLog {
        id: id.parse().map_err(|_| Error::EntityNotFound(Uuid::nil()))?,
        source_uri,
        content_hash,
        imported_at: DateTime::parse_from_rfc3339(&imported_at)
            .map_err(|_| Error::EntityNotFound(Uuid::nil()))?
            .with_timezone(&Utc),
        entity_count: u32::try_from(entity_count).unwrap_or(0),
        relationship_count: u32::try_from(relationship_count).unwrap_or(0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::{PersonData, EntityData};
    use crate::relationship::RelationType;

    #[tokio::test]
    async fn test_entity_crud() {
        let storage = Storage::open_memory().await.unwrap();

        let entity = Entity::new(
            "John Doe".to_string(),
            EntityData::Person(PersonData {
                date_of_birth: None,
                date_of_death: None,
                nationalities: vec!["US".to_string()],
                roles: vec!["CEO".to_string()],
            }),
        );

        storage.insert_entity(&entity).await.unwrap();

        let retrieved = storage.get_entity(entity.id).await.unwrap();
        assert_eq!(retrieved.canonical_name, "John Doe");

        let entities = storage.list_entities(Some(EntityType::Person)).await.unwrap();
        assert_eq!(entities.len(), 1);

        storage.delete_entity(entity.id).await.unwrap();

        let result = storage.get_entity(entity.id).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_relationships() {
        let storage = Storage::open_memory().await.unwrap();

        let person = Entity::new(
            "Jane Doe".to_string(),
            EntityData::Person(PersonData {
                date_of_birth: None,
                date_of_death: None,
                nationalities: vec![],
                roles: vec![],
            }),
        );

        let org = Entity::new(
            "Acme Corp".to_string(),
            EntityData::Organization(crate::entity::OrganizationData {
                org_type: crate::entity::OrganizationType::Corporation,
                jurisdiction: Some("US".to_string()),
                registration_number: None,
                founded_date: None,
                dissolved_date: None,
            }),
        );

        storage.insert_entity(&person).await.unwrap();
        storage.insert_entity(&org).await.unwrap();

        let rel = Relationship::new(person.id, org.id, RelationType::EmployedBy).unwrap();
        storage.insert_relationship(&rel).await.unwrap();

        let rels = storage.get_entity_relationships(person.id).await.unwrap();
        assert_eq!(rels.len(), 1);
        assert_eq!(rels[0].relation_type, RelationType::EmployedBy);
    }
}
