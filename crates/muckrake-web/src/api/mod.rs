mod config;
mod entities;
mod files;
mod relationships;
mod session;
mod workspaces;

use axum::Router;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .nest("/config", config::router())
        .nest("/entities", entities::router())
        .nest("/files", files::router())
        .nest("/relationships", relationships::router())
        .nest("/session", session::router())
        .nest("/workspaces", workspaces::router())
}
