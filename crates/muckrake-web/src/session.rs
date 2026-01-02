use axum::{
    extract::FromRequestParts,
    http::{StatusCode, request::Parts},
};
use axum_extra::extract::CookieJar;
use uuid::Uuid;

use crate::state::{AppState, SessionId};

const SESSION_COOKIE_NAME: &str = "muckrake_session";

/// Extractor that provides the session ID from cookies
///
/// Creates a new session and project if one doesn't exist
pub struct SessionToken(pub SessionId);

impl FromRequestParts<AppState> for SessionToken {
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let jar = CookieJar::from_request_parts(parts, state)
            .await
            .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Failed to read cookies"))?;

        let session_id = if let Some(cookie) = jar.get(SESSION_COOKIE_NAME) {
            cookie
                .value()
                .parse::<Uuid>()
                .unwrap_or_else(|_| Uuid::new_v4())
        } else {
            Uuid::new_v4()
        };

        // Ensure session exists and has a workspace and project
        {
            let mut manager = state.manager.write().await;

            // Get or create session and check what's missing
            let (needs_workspace, needs_project) = {
                let session = manager.get_or_create_session(session_id);
                (session.workspace_id.is_none(), session.project_id.is_none())
            };

            // Auto-create a workspace if session doesn't have one
            if needs_workspace {
                manager.create_workspace(session_id, "Untitled Workspace".to_string());
            }

            // Auto-create a project if session doesn't have one
            if needs_project {
                let _ = manager
                    .create_project(session_id, "Untitled Investigation".to_string())
                    .await;
            }
        }

        Ok(SessionToken(session_id))
    }
}

/// Cookie to set on response for new sessions
pub fn session_cookie(session_id: SessionId) -> axum_extra::extract::cookie::Cookie<'static> {
    axum_extra::extract::cookie::Cookie::build((SESSION_COOKIE_NAME, session_id.to_string()))
        .path("/")
        .http_only(true)
        .same_site(axum_extra::extract::cookie::SameSite::Strict)
        .build()
}
