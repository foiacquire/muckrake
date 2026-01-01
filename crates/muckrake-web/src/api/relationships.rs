use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
};
use muckrake_core::{Relationship, RelationshipData, RelationType};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", post(create_relationship))
        .route("/{id}", get(get_relationship).delete(delete_relationship))
        .route("/entity/{entity_id}", get(get_entity_relationships))
}

#[derive(Debug, Serialize)]
pub struct RelationshipResponse {
    pub id: Uuid,
    pub source_id: Uuid,
    pub target_id: Uuid,
    pub relation_type: RelationType,
    pub confidence: Option<f64>,
    #[serde(flatten)]
    pub data: RelationshipData,
    pub created_at: String,
}

impl From<Relationship> for RelationshipResponse {
    fn from(r: Relationship) -> Self {
        Self {
            id: r.id,
            source_id: r.source_id,
            target_id: r.target_id,
            relation_type: r.relation_type,
            confidence: r.confidence,
            data: r.data,
            created_at: r.created_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateRelationshipRequest {
    pub source_id: Uuid,
    pub target_id: Uuid,
    pub relation_type: RelationType,
    pub confidence: Option<f64>,
    #[serde(flatten)]
    pub data: Option<RelationshipData>,
}

async fn get_relationship(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<RelationshipResponse>, (StatusCode, String)> {
    let storage = state.storage.read().await;
    let rel = storage.get_relationship(id).await.map_err(|e| match e {
        muckrake_core::Error::RelationshipNotFound(_) => {
            (StatusCode::NOT_FOUND, "Relationship not found".to_string())
        }
        _ => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    })?;

    Ok(Json(RelationshipResponse::from(rel)))
}

async fn get_entity_relationships(
    State(state): State<AppState>,
    Path(entity_id): Path<Uuid>,
) -> Result<Json<Vec<RelationshipResponse>>, (StatusCode, String)> {
    let storage = state.storage.read().await;
    let rels = storage
        .get_entity_relationships(entity_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(rels.into_iter().map(RelationshipResponse::from).collect()))
}

async fn create_relationship(
    State(state): State<AppState>,
    Json(req): Json<CreateRelationshipRequest>,
) -> Result<(StatusCode, Json<RelationshipResponse>), (StatusCode, String)> {
    let mut rel = Relationship::new(req.source_id, req.target_id, req.relation_type)
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

    if let Some(confidence) = req.confidence {
        rel = rel.with_confidence(confidence);
    }
    if let Some(data) = req.data {
        rel = rel.with_data(data);
    }

    let storage = state.storage.read().await;
    storage
        .insert_relationship(&rel)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok((StatusCode::CREATED, Json(RelationshipResponse::from(rel))))
}

async fn delete_relationship(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, String)> {
    let storage = state.storage.read().await;
    storage.delete_relationship(id).await.map_err(|e| match e {
        muckrake_core::Error::RelationshipNotFound(_) => {
            (StatusCode::NOT_FOUND, "Relationship not found".to_string())
        }
        _ => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    })?;

    Ok(StatusCode::NO_CONTENT)
}
