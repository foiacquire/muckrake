use serde::{Deserialize, Serialize};

/// Server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Allow storing workspaces on the server
    /// When false (default), workspaces are only stored client-side
    #[serde(default)]
    pub allow_remote_workspaces: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            allow_remote_workspaces: false,
        }
    }
}

impl ServerConfig {
    pub fn from_env() -> Self {
        Self {
            allow_remote_workspaces: std::env::var("MUCKRAKE_ALLOW_REMOTE_WORKSPACES")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(false),
        }
    }
}
