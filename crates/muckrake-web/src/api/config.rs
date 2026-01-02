use axum::{Json, Router, extract::State, response::IntoResponse, routing::get};
use serde::Serialize;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(get_config))
}

#[derive(Debug, Serialize)]
pub struct ConfigResponse {
    pub allow_remote_workspaces: bool,
}

async fn get_config(State(state): State<AppState>) -> impl IntoResponse {
    Json(ConfigResponse {
        allow_remote_workspaces: state.config.allow_remote_workspaces,
    })
}
