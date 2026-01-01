use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::get,
};
use muckrake_core::{Entity, EntityData, EntityType};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_entities).post(create_entity))
        .route("/search", get(search_entities))
        .route("/{id}", get(get_entity).put(update_entity).delete(delete_entity))
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    #[serde(rename = "type")]
    entity_type: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    q: String,
}

#[derive(Debug, Serialize)]
pub struct EntityResponse {
    pub id: Uuid,
    pub canonical_name: String,
    #[serde(flatten)]
    pub data: EntityData,
    pub confidence: Option<f64>,
    pub created_at: String,
    pub updated_at: String,
}

impl From<Entity> for EntityResponse {
    fn from(e: Entity) -> Self {
        Self {
            id: e.id,
            canonical_name: e.canonical_name,
            data: e.data,
            confidence: e.confidence,
            created_at: e.created_at.to_rfc3339(),
            updated_at: e.updated_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateEntityRequest {
    pub canonical_name: String,
    #[serde(flatten)]
    pub data: EntityData,
    pub confidence: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateEntityRequest {
    pub canonical_name: Option<String>,
    #[serde(flatten)]
    pub data: Option<EntityData>,
    pub confidence: Option<f64>,
}

async fn list_entities(
    State(state): State<AppState>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Vec<EntityResponse>>, (StatusCode, String)> {
    let entity_type = query
        .entity_type
        .map(|t| t.parse::<EntityType>())
        .transpose()
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

    let storage = state.storage.read().await;
    let entities = storage
        .list_entities(entity_type)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(entities.into_iter().map(EntityResponse::from).collect()))
}

async fn search_entities(
    State(state): State<AppState>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<EntityResponse>>, (StatusCode, String)> {
    let storage = state.storage.read().await;
    let entities = storage
        .search_entities(&query.q)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(entities.into_iter().map(EntityResponse::from).collect()))
}

async fn get_entity(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<EntityResponse>, (StatusCode, String)> {
    let storage = state.storage.read().await;
    let entity = storage
        .get_entity(id)
        .await
        .map_err(|e| match e {
            muckrake_core::Error::EntityNotFound(_) => {
                (StatusCode::NOT_FOUND, "Entity not found".to_string())
            }
            _ => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
        })?;

    Ok(Json(EntityResponse::from(entity)))
}

async fn create_entity(
    State(state): State<AppState>,
    Json(req): Json<CreateEntityRequest>,
) -> Result<(StatusCode, Json<EntityResponse>), (StatusCode, String)> {
    let mut entity = Entity::new(req.canonical_name, req.data);
    if let Some(confidence) = req.confidence {
        entity = entity.with_confidence(confidence);
    }

    let storage = state.storage.read().await;
    storage
        .insert_entity(&entity)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok((StatusCode::CREATED, Json(EntityResponse::from(entity))))
}

async fn update_entity(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateEntityRequest>,
) -> Result<Json<EntityResponse>, (StatusCode, String)> {
    let storage = state.storage.read().await;

    let mut entity = storage.get_entity(id).await.map_err(|e| match e {
        muckrake_core::Error::EntityNotFound(_) => {
            (StatusCode::NOT_FOUND, "Entity not found".to_string())
        }
        _ => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    })?;

    if let Some(name) = req.canonical_name {
        entity.canonical_name = name;
    }
    if let Some(data) = req.data {
        entity.data = data;
    }
    if let Some(confidence) = req.confidence {
        entity.confidence = Some(confidence);
    }

    storage
        .update_entity(&entity)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(EntityResponse::from(entity)))
}

async fn delete_entity(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, String)> {
    let storage = state.storage.read().await;
    storage.delete_entity(id).await.map_err(|e| match e {
        muckrake_core::Error::EntityNotFound(_) => {
            (StatusCode::NOT_FOUND, "Entity not found".to_string())
        }
        _ => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    })?;

    Ok(StatusCode::NO_CONTENT)
}
