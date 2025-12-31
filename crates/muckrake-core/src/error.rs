use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Entity not found: {0}")]
    EntityNotFound(uuid::Uuid),

    #[error("Relationship not found: {0}")]
    RelationshipNotFound(uuid::Uuid),

    #[error("Invalid entity type: {0}")]
    InvalidEntityType(String),

    #[error("Invalid relationship type: {0}")]
    InvalidRelationshipType(String),

    #[error("Duplicate alias: {alias} already exists for entity {entity_id}")]
    DuplicateAlias { entity_id: uuid::Uuid, alias: String },

    #[error("Self-referential relationship not allowed")]
    SelfReference,

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
