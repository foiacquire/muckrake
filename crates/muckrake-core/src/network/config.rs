use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Network operation mode - defaults to strictest security
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkMode {
    /// All traffic through Tor with Snowflake (default, strictest)
    TorSnowflake,
    /// All traffic through Tor with obfs4
    TorObfs4,
    /// All traffic through Tor direct (no pluggable transport)
    TorDirect,
    /// Direct connection (DANGEROUS - requires explicit opt-in)
    #[serde(rename = "direct_unsafe")]
    DirectUnsafe,
}

impl Default for NetworkMode {
    fn default() -> Self {
        Self::TorSnowflake
    }
}

impl NetworkMode {
    pub fn requires_tor(&self) -> bool {
        !matches!(self, Self::DirectUnsafe)
    }

    pub fn requires_pluggable_transport(&self) -> bool {
        matches!(self, Self::TorSnowflake | Self::TorObfs4)
    }

    pub fn transport_name(&self) -> Option<&'static str> {
        match self {
            Self::TorSnowflake => Some("snowflake"),
            Self::TorObfs4 => Some("obfs4"),
            _ => None,
        }
    }
}

/// Which Tor client implementation to use
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TorClient {
    /// C Tor (the original implementation)
    Tor,
    /// Arti (Rust implementation)
    Arti,
    /// Use existing Tor Browser SOCKS proxy (no process management)
    TorBrowser,
    /// Use existing system SOCKS proxy (no process management)
    ExistingProxy,
}

impl TorClient {
    pub fn binary_name(&self) -> Option<&'static str> {
        match self {
            Self::Tor => Some("tor"),
            Self::Arti => Some("arti"),
            Self::TorBrowser | Self::ExistingProxy => None,
        }
    }

    pub fn manages_process(&self) -> bool {
        matches!(self, Self::Tor | Self::Arti)
    }
}

/// Tor client preference order
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TorClientPreference {
    /// Ordered list of clients to try (first available wins)
    pub order: Vec<TorClient>,
}

impl Default for TorClientPreference {
    fn default() -> Self {
        Self {
            // Try system tor first (more mature, better tested)
            // Fall back to arti if tor isn't available
            order: vec![TorClient::Tor, TorClient::Arti],
        }
    }
}

impl TorClientPreference {
    /// Prefer Arti over C Tor
    pub fn prefer_arti() -> Self {
        Self {
            order: vec![TorClient::Arti, TorClient::Tor],
        }
    }

    /// Only use C Tor
    pub fn tor_only() -> Self {
        Self {
            order: vec![TorClient::Tor],
        }
    }

    /// Only use Arti
    pub fn arti_only() -> Self {
        Self {
            order: vec![TorClient::Arti],
        }
    }

    /// Use existing Tor Browser (port 9150)
    pub fn tor_browser() -> Self {
        Self {
            order: vec![TorClient::TorBrowser],
        }
    }

    /// Use existing proxy (user-managed)
    pub fn existing_proxy() -> Self {
        Self {
            order: vec![TorClient::ExistingProxy],
        }
    }

    /// Find the first available client from the preference list
    pub fn find_available(&self) -> Option<TorClient> {
        for client in &self.order {
            match client {
                TorClient::Tor | TorClient::Arti => {
                    if let Some(binary) = client.binary_name() {
                        if which::which(binary).is_ok() {
                            return Some(*client);
                        }
                    }
                }
                TorClient::TorBrowser | TorClient::ExistingProxy => {
                    // These don't require a binary, just assume available
                    return Some(*client);
                }
            }
        }
        None
    }
}

/// Snowflake-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnowflakeConfig {
    /// Path to snowflake-client binary
    pub client_path: PathBuf,
    /// Broker URL (domain-fronted)
    pub broker_url: String,
    /// STUN servers for WebRTC
    pub stun_servers: Vec<String>,
    /// Front domain for domain fronting
    pub front_domain: Option<String>,
    /// ICE candidate timeout
    pub ice_timeout_seconds: u32,
    /// Maximum peers
    pub max_peers: u32,
}

impl Default for SnowflakeConfig {
    fn default() -> Self {
        Self {
            client_path: PathBuf::from("snowflake-client"),
            broker_url: "https://snowflake-broker.torproject.net/".to_string(),
            stun_servers: vec![
                "stun:stun.l.google.com:19302".to_string(),
                "stun:stun.voip.blackberry.com:3478".to_string(),
                "stun:stun.altar.com.pl:3478".to_string(),
                "stun:stun.antisip.com:3478".to_string(),
                "stun:stun.bluesip.net:3478".to_string(),
            ],
            front_domain: Some("cdn.sstatic.net".to_string()),
            ice_timeout_seconds: 30,
            max_peers: 1,
        }
    }
}

/// Network security configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// Operation mode (default: TorSnowflake)
    pub mode: NetworkMode,
    /// Tor client preference order
    pub tor_client: TorClientPreference,
    /// SOCKS proxy port for Tor
    pub socks_port: u16,
    /// Snowflake configuration
    pub snowflake: SnowflakeConfig,
    /// Path to Tor data directory (for C Tor)
    pub tor_data_dir: Option<PathBuf>,
    /// Path to Arti configuration directory
    pub arti_config_dir: Option<PathBuf>,
    /// Path to Arti data directory
    pub arti_data_dir: Option<PathBuf>,
    /// Connection timeout in seconds
    pub connect_timeout_seconds: u32,
    /// Request timeout in seconds
    pub request_timeout_seconds: u32,
    /// Whether to allow .onion addresses
    pub allow_onion: bool,
    /// Domains that bypass Tor (empty by default - nothing bypasses)
    /// SECURITY: Only for localhost/internal services
    pub bypass_domains: Vec<String>,
    /// User agent to use (randomized by default)
    pub user_agent: Option<String>,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            mode: NetworkMode::TorSnowflake,
            tor_client: TorClientPreference::default(),
            socks_port: 9150,
            snowflake: SnowflakeConfig::default(),
            tor_data_dir: None,
            arti_config_dir: None,
            arti_data_dir: None,
            connect_timeout_seconds: 120,
            request_timeout_seconds: 300,
            allow_onion: true,
            bypass_domains: vec![],
            user_agent: None,
        }
    }
}

impl NetworkConfig {
    pub fn socks_proxy_url(&self) -> String {
        format!("socks5://127.0.0.1:{}", self.socks_port)
    }

    pub fn is_bypass_domain(&self, domain: &str) -> bool {
        if domain == "localhost" || domain == "127.0.0.1" || domain == "::1" {
            return true;
        }
        self.bypass_domains.iter().any(|d| {
            domain == d || domain.ends_with(&format!(".{}", d))
        })
    }

    pub fn validate(&self) -> Result<(), NetworkConfigError> {
        if self.mode == NetworkMode::DirectUnsafe && self.bypass_domains.is_empty() {
            return Err(NetworkConfigError::UnsafeModeWithoutBypass);
        }
        if self.mode.requires_pluggable_transport() {
            if !self.snowflake.client_path.exists() {
                return Err(NetworkConfigError::SnowflakeClientNotFound(
                    self.snowflake.client_path.clone(),
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum NetworkConfigError {
    #[error("Direct unsafe mode enabled but no bypass domains configured")]
    UnsafeModeWithoutBypass,
    #[error("Snowflake client not found at: {0}")]
    SnowflakeClientNotFound(PathBuf),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_mode_is_strictest() {
        let config = NetworkConfig::default();
        assert_eq!(config.mode, NetworkMode::TorSnowflake);
        assert!(config.mode.requires_tor());
        assert!(config.mode.requires_pluggable_transport());
    }

    #[test]
    fn test_network_mode_requirements() {
        assert!(NetworkMode::TorSnowflake.requires_tor());
        assert!(NetworkMode::TorSnowflake.requires_pluggable_transport());
        assert_eq!(NetworkMode::TorSnowflake.transport_name(), Some("snowflake"));

        assert!(NetworkMode::TorObfs4.requires_tor());
        assert!(NetworkMode::TorObfs4.requires_pluggable_transport());
        assert_eq!(NetworkMode::TorObfs4.transport_name(), Some("obfs4"));

        assert!(NetworkMode::TorDirect.requires_tor());
        assert!(!NetworkMode::TorDirect.requires_pluggable_transport());
        assert_eq!(NetworkMode::TorDirect.transport_name(), None);

        assert!(!NetworkMode::DirectUnsafe.requires_tor());
        assert!(!NetworkMode::DirectUnsafe.requires_pluggable_transport());
    }

    #[test]
    fn test_localhost_always_bypassed() {
        let config = NetworkConfig::default();

        assert!(config.is_bypass_domain("localhost"));
        assert!(config.is_bypass_domain("127.0.0.1"));
        assert!(config.is_bypass_domain("::1"));
    }

    #[test]
    fn test_explicit_bypass_domains() {
        let config = NetworkConfig {
            bypass_domains: vec!["internal.example.com".into()],
            ..Default::default()
        };

        assert!(config.is_bypass_domain("internal.example.com"));
        assert!(config.is_bypass_domain("api.internal.example.com"));
        assert!(!config.is_bypass_domain("example.com"));
        assert!(!config.is_bypass_domain("external.com"));
    }

    #[test]
    fn test_no_bypass_by_default() {
        let config = NetworkConfig::default();

        assert!(!config.is_bypass_domain("example.com"));
        assert!(!config.is_bypass_domain("google.com"));
    }

    #[test]
    fn test_tor_client_preference_default() {
        let pref = TorClientPreference::default();
        assert_eq!(pref.order, vec![TorClient::Tor, TorClient::Arti]);
    }

    #[test]
    fn test_tor_client_preference_factories() {
        let pref = TorClientPreference::prefer_arti();
        assert_eq!(pref.order, vec![TorClient::Arti, TorClient::Tor]);

        let pref = TorClientPreference::tor_only();
        assert_eq!(pref.order, vec![TorClient::Tor]);

        let pref = TorClientPreference::arti_only();
        assert_eq!(pref.order, vec![TorClient::Arti]);

        let pref = TorClientPreference::tor_browser();
        assert_eq!(pref.order, vec![TorClient::TorBrowser]);

        let pref = TorClientPreference::existing_proxy();
        assert_eq!(pref.order, vec![TorClient::ExistingProxy]);
    }

    #[test]
    fn test_tor_client_binary_names() {
        assert_eq!(TorClient::Tor.binary_name(), Some("tor"));
        assert_eq!(TorClient::Arti.binary_name(), Some("arti"));
        assert_eq!(TorClient::TorBrowser.binary_name(), None);
        assert_eq!(TorClient::ExistingProxy.binary_name(), None);
    }

    #[test]
    fn test_tor_client_manages_process() {
        assert!(TorClient::Tor.manages_process());
        assert!(TorClient::Arti.manages_process());
        assert!(!TorClient::TorBrowser.manages_process());
        assert!(!TorClient::ExistingProxy.manages_process());
    }

    #[test]
    fn test_existing_proxy_always_available() {
        let pref = TorClientPreference::existing_proxy();
        assert_eq!(pref.find_available(), Some(TorClient::ExistingProxy));

        let pref = TorClientPreference::tor_browser();
        assert_eq!(pref.find_available(), Some(TorClient::TorBrowser));
    }

    #[test]
    fn test_socks_proxy_url() {
        let config = NetworkConfig::default();
        assert_eq!(config.socks_proxy_url(), "socks5://127.0.0.1:9150");

        let config = NetworkConfig {
            socks_port: 9050,
            ..Default::default()
        };
        assert_eq!(config.socks_proxy_url(), "socks5://127.0.0.1:9050");
    }

    #[test]
    fn test_snowflake_default_config() {
        let sf = SnowflakeConfig::default();
        assert_eq!(sf.broker_url, "https://snowflake-broker.torproject.net/");
        assert_eq!(sf.front_domain, Some("cdn.sstatic.net".into()));
        assert!(!sf.stun_servers.is_empty());
        assert_eq!(sf.max_peers, 1);
    }

    #[test]
    fn test_config_serialization() {
        let config = NetworkConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let parsed: NetworkConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(config.mode, parsed.mode);
        assert_eq!(config.socks_port, parsed.socks_port);
    }
}
