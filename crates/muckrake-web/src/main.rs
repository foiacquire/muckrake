mod api;
mod config;
mod session;
mod state;
mod workspace;

use std::net::SocketAddr;

use axum::Router;
use tower_http::{
    cors::CorsLayer,
    services::{ServeDir, ServeFile},
    trace::TraceLayer,
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::state::AppState;

/// Port 1972 - the year of Watergate
const PORT: u16 = 1972;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "muckrake_web=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Multi-session support: each session gets its own project
    let state = AppState::new();

    let static_dir = std::env::var("MUCKRAKE_STATIC")
        .unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/static").to_string());

    let serve_dir = ServeDir::new(&static_dir)
        .not_found_service(ServeFile::new(format!("{static_dir}/index.html")));

    let app = Router::new()
        .nest("/api", api::router())
        .fallback_service(serve_dir)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], PORT));
    tracing::info!("Starting muckrake on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
