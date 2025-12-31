use chrono::{DateTime, Utc};
use sqlx::{sqlite::SqlitePoolOptions, Pool, Sqlite};
use uuid::Uuid;

use crate::{
    entity::{Entity, EntityAlias, EntityType},
    relationship::{Evidence, Relationship},
    Error, Result,
};

const INIT_SQL: &str = r#"
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

CREATE TABLE IF NOT EXISTS entity_aliases (
    id TEXT PRIMARY KEY,
    entity_id TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    alias TEXT NOT NULL,
    source TEXT
);

CREATE INDEX IF NOT EXISTS idx_aliases_entity ON entity_aliases(entity_id);
CREATE INDEX IF NOT EXISTS idx_aliases_alias ON entity_aliases(alias);
CREATE UNIQUE INDEX IF NOT EXISTS idx_aliases_unique ON entity_aliases(entity_id, alias);

CREATE TABLE IF NOT EXISTS relationships (
    id TEXT PRIMARY KEY,
    source_id TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    target_id TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    relation_type TEXT NOT NULL,
    confidence REAL,
    data TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_rel_source ON relationships(source_id);
CREATE INDEX IF NOT EXISTS idx_rel_target ON relationships(target_id);
CREATE INDEX IF NOT EXISTS idx_rel_type ON relationships(relation_type);

CREATE TABLE IF NOT EXISTS evidence (
    id TEXT PRIMARY KEY,
    entity_id TEXT REFERENCES entities(id) ON DELETE CASCADE,
    relationship_id TEXT REFERENCES relationships(id) ON DELETE CASCADE,
    document_id TEXT NOT NULL,
    page_number INTEGER,
    text_span TEXT,
    context TEXT
);

CREATE INDEX IF NOT EXISTS idx_evidence_entity ON evidence(entity_id);
CREATE INDEX IF NOT EXISTS idx_evidence_relationship ON evidence(relationship_id);
CREATE INDEX IF NOT EXISTS idx_evidence_document ON evidence(document_id);

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

        sqlx::query(INIT_SQL).execute(&pool).await?;

        Ok(Self { pool })
    }

    pub async fn open_memory() -> Result<Self> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await?;

        sqlx::query(INIT_SQL).execute(&pool).await?;

        Ok(Self { pool })
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
            INSERT INTO relationships (id, source_id, target_id, relation_type, confidence, data, created_at)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(rel.id.to_string())
        .bind(rel.source_id.to_string())
        .bind(rel.target_id.to_string())
        .bind(rel.relation_type.as_str())
        .bind(rel.confidence)
        .bind(data_json)
        .bind(rel.created_at.to_rfc3339())
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_relationship(&self, id: Uuid) -> Result<Relationship> {
        let row: (String, String, String, String, Option<f64>, String, String) = sqlx::query_as(
            r#"
            SELECT id, source_id, target_id, relation_type, confidence, data, created_at
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
        let rows: Vec<(String, String, String, String, Option<f64>, String, String)> =
            sqlx::query_as(
                r#"
            SELECT id, source_id, target_id, relation_type, confidence, data, created_at
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

    // Evidence operations

    pub async fn insert_evidence(&self, evidence: &Evidence) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO evidence (id, entity_id, relationship_id, document_id, page_number, text_span, context)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(evidence.id.to_string())
        .bind(evidence.entity_id.map(|id| id.to_string()))
        .bind(evidence.relationship_id.map(|id| id.to_string()))
        .bind(&evidence.document_id)
        .bind(evidence.page_number.map(|n| i64::from(n)))
        .bind(&evidence.text_span)
        .bind(&evidence.context)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_entity_evidence(&self, entity_id: Uuid) -> Result<Vec<Evidence>> {
        let rows: Vec<(
            String,
            Option<String>,
            Option<String>,
            String,
            Option<i64>,
            Option<String>,
            Option<String>,
        )> = sqlx::query_as(
            r#"
            SELECT id, entity_id, relationship_id, document_id, page_number, text_span, context
            FROM evidence WHERE entity_id = ?
            "#,
        )
        .bind(entity_id.to_string())
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(parse_evidence_row).collect()
    }

    pub async fn get_relationship_evidence(&self, relationship_id: Uuid) -> Result<Vec<Evidence>> {
        let rows: Vec<(
            String,
            Option<String>,
            Option<String>,
            String,
            Option<i64>,
            Option<String>,
            Option<String>,
        )> = sqlx::query_as(
            r#"
            SELECT id, entity_id, relationship_id, document_id, page_number, text_span, context
            FROM evidence WHERE relationship_id = ?
            "#,
        )
        .bind(relationship_id.to_string())
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(parse_evidence_row).collect()
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

fn parse_relationship_row(
    row: (String, String, String, String, Option<f64>, String, String),
) -> Result<Relationship> {
    let (id, source_id, target_id, relation_type, confidence, data_json, created_at) = row;

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
        confidence,
        data: serde_json::from_str(&data_json).unwrap_or_default(),
        created_at: DateTime::parse_from_rfc3339(&created_at)
            .map_err(|_| Error::RelationshipNotFound(Uuid::nil()))?
            .with_timezone(&Utc),
    })
}

fn parse_evidence_row(
    row: (
        String,
        Option<String>,
        Option<String>,
        String,
        Option<i64>,
        Option<String>,
        Option<String>,
    ),
) -> Result<Evidence> {
    let (id, entity_id, relationship_id, document_id, page_number, text_span, context) = row;

    Ok(Evidence {
        id: id.parse().map_err(|_| Error::EntityNotFound(Uuid::nil()))?,
        entity_id: entity_id.and_then(|s| s.parse().ok()),
        relationship_id: relationship_id.and_then(|s| s.parse().ok()),
        document_id,
        page_number: page_number.and_then(|n| u32::try_from(n).ok()),
        text_span,
        context,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::{PersonData, EntityData};

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
