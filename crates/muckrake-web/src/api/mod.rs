mod entities;
mod relationships;

use axum::Router;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .nest("/entities", entities::router())
        .nest("/relationships", relationships::router())
}
