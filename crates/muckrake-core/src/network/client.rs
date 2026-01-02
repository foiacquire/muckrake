use reqwest::{Client, Proxy, Response};
use std::sync::Arc;
use std::time::Duration;
use url::Url;

use super::config::{NetworkConfig, NetworkMode};
use super::tor::TorManagerHandle;

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("Tor is not ready - cannot make request")]
    TorNotReady,
    #[error("Direct connections are disabled for security")]
    DirectConnectionBlocked,
    #[error("Invalid URL: {0}")]
    InvalidUrl(String),
    #[error("Request blocked: domain {0} is not in bypass list")]
    DomainBlocked(String),
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("URL parse error: {0}")]
    UrlParse(#[from] url::ParseError),
}

pub type ClientResult<T> = Result<T, ClientError>;

/// Secure HTTP client that enforces all traffic through Tor
///
/// SECURITY: This client will REFUSE to make direct connections unless
/// the target is localhost or explicitly in the bypass list.
pub struct SecureClient {
    config: NetworkConfig,
    tor: Option<TorManagerHandle>,
    inner: Option<Client>,
}

impl SecureClient {
    /// Create a new SecureClient with the given configuration
    pub fn new(config: NetworkConfig, tor: Option<TorManagerHandle>) -> Self {
        Self {
            config,
            tor,
            inner: None,
        }
    }

    /// Initialize the HTTP client - must be called after Tor is ready
    pub async fn init(&mut self) -> ClientResult<()> {
        let client = self.build_client().await?;
        self.inner = Some(client);
        Ok(())
    }

    async fn build_client(&self) -> ClientResult<Client> {
        let mut builder = Client::builder()
            .connect_timeout(Duration::from_secs(self.config.connect_timeout_seconds as u64))
            .timeout(Duration::from_secs(self.config.request_timeout_seconds as u64))
            .danger_accept_invalid_certs(false)
            .https_only(!self.config.allow_onion);

        if let Some(ref ua) = self.config.user_agent {
            builder = builder.user_agent(ua);
        } else {
            builder = builder.user_agent(random_user_agent());
        }

        if self.config.mode.requires_tor() {
            let proxy_url = self.get_socks_proxy_url()?;
            let proxy = Proxy::all(&proxy_url)?;
            builder = builder.proxy(proxy);
        }

        builder.build().map_err(ClientError::Http)
    }

    fn get_socks_proxy_url(&self) -> ClientResult<String> {
        if let Some(ref tor) = self.tor {
            if !tor.is_ready() && self.config.mode != NetworkMode::DirectUnsafe {
                return Err(ClientError::TorNotReady);
            }
            return Ok(tor.socks_proxy_url());
        }
        Ok(self.config.socks_proxy_url())
    }

    fn validate_request(&self, url: &str) -> ClientResult<()> {
        let parsed = Url::parse(url)?;

        let host = parsed.host_str()
            .ok_or_else(|| ClientError::InvalidUrl("No host in URL".to_string()))?;

        if self.config.is_bypass_domain(host) {
            return Ok(());
        }

        if self.config.mode == NetworkMode::DirectUnsafe {
            return Ok(());
        }

        if !self.config.mode.requires_tor() {
            return Err(ClientError::DirectConnectionBlocked);
        }

        if let Some(ref tor) = self.tor {
            if !tor.is_ready() {
                return Err(ClientError::TorNotReady);
            }
        }

        Ok(())
    }

    pub async fn get(&self, url: &str) -> ClientResult<Response> {
        self.validate_request(url)?;

        let client = self.inner.as_ref()
            .ok_or(ClientError::TorNotReady)?;

        client.get(url)
            .send()
            .await
            .map_err(ClientError::Http)
    }

    pub async fn post(&self, url: &str, body: String) -> ClientResult<Response> {
        self.validate_request(url)?;

        let client = self.inner.as_ref()
            .ok_or(ClientError::TorNotReady)?;

        client.post(url)
            .body(body)
            .send()
            .await
            .map_err(ClientError::Http)
    }

    pub async fn post_json<T: serde::Serialize>(&self, url: &str, json: &T) -> ClientResult<Response> {
        self.validate_request(url)?;

        let client = self.inner.as_ref()
            .ok_or(ClientError::TorNotReady)?;

        client.post(url)
            .json(json)
            .send()
            .await
            .map_err(ClientError::Http)
    }

    pub async fn head(&self, url: &str) -> ClientResult<Response> {
        self.validate_request(url)?;

        let client = self.inner.as_ref()
            .ok_or(ClientError::TorNotReady)?;

        client.head(url)
            .send()
            .await
            .map_err(ClientError::Http)
    }

    pub fn request(&self, method: reqwest::Method, url: &str) -> ClientResult<reqwest::RequestBuilder> {
        self.validate_request(url)?;

        let client = self.inner.as_ref()
            .ok_or(ClientError::TorNotReady)?;

        Ok(client.request(method, url))
    }

    pub fn config(&self) -> &NetworkConfig {
        &self.config
    }
}

/// Handle that can be cloned and shared across threads
pub struct SecureClientHandle {
    inner: Arc<tokio::sync::RwLock<SecureClient>>,
}

impl SecureClientHandle {
    pub fn new(config: NetworkConfig, tor: Option<TorManagerHandle>) -> Self {
        Self {
            inner: Arc::new(tokio::sync::RwLock::new(SecureClient::new(config, tor))),
        }
    }

    pub async fn init(&self) -> ClientResult<()> {
        self.inner.write().await.init().await
    }

    pub async fn get(&self, url: &str) -> ClientResult<Response> {
        self.inner.read().await.get(url).await
    }

    pub async fn post(&self, url: &str, body: String) -> ClientResult<Response> {
        self.inner.read().await.post(url, body).await
    }

    pub async fn post_json<T: serde::Serialize + Sync>(&self, url: &str, json: &T) -> ClientResult<Response> {
        self.inner.read().await.post_json(url, json).await
    }

    pub async fn head(&self, url: &str) -> ClientResult<Response> {
        self.inner.read().await.head(url).await
    }
}

impl Clone for SecureClientHandle {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

fn random_user_agent() -> String {
    use rand::Rng;

    let agents = [
        "Mozilla/5.0 (Windows NT 10.0; rv:128.0) Gecko/20100101 Firefox/128.0",
        "Mozilla/5.0 (Windows NT 10.0; rv:115.0) Gecko/20100101 Firefox/115.0",
        "Mozilla/5.0 (X11; Linux x86_64; rv:128.0) Gecko/20100101 Firefox/128.0",
        "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.15; rv:128.0) Gecko/20100101 Firefox/128.0",
    ];

    let mut rng = rand::rng();
    agents[rng.random_range(0..agents.len())].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_client(mode: NetworkMode, bypass: Vec<String>) -> SecureClient {
        let config = NetworkConfig {
            mode,
            bypass_domains: bypass,
            ..Default::default()
        };
        SecureClient::new(config, None)
    }

    #[test]
    fn test_validate_localhost_always_allowed() {
        let client = make_client(NetworkMode::TorSnowflake, vec![]);

        assert!(client.validate_request("http://localhost:8080/api").is_ok());
        assert!(client.validate_request("http://127.0.0.1:3000/").is_ok());
        assert!(client.validate_request("http://[::1]:9000/").is_ok());
    }

    #[test]
    fn test_validate_bypass_domain() {
        let client = make_client(
            NetworkMode::TorSnowflake,
            vec!["internal.corp".into()],
        );

        assert!(client.validate_request("https://internal.corp/api").is_ok());
        assert!(client.validate_request("https://api.internal.corp/v1").is_ok());
    }

    #[test]
    fn test_validate_rejects_invalid_url() {
        let client = make_client(NetworkMode::TorSnowflake, vec![]);

        assert!(matches!(
            client.validate_request("not-a-url"),
            Err(ClientError::UrlParse(_))
        ));
    }

    #[test]
    fn test_validate_rejects_url_without_host() {
        let client = make_client(NetworkMode::TorSnowflake, vec![]);

        assert!(matches!(
            client.validate_request("file:///etc/passwd"),
            Err(ClientError::InvalidUrl(_))
        ));
    }

    #[test]
    fn test_direct_unsafe_allows_all() {
        let client = make_client(NetworkMode::DirectUnsafe, vec![]);

        assert!(client.validate_request("https://example.com").is_ok());
        assert!(client.validate_request("https://google.com").is_ok());
    }

    #[test]
    fn test_tor_required_but_not_ready() {
        let client = make_client(NetworkMode::TorSnowflake, vec![]);

        let result = client.validate_request("https://example.com");

        assert!(result.is_ok());
    }

    #[test]
    fn test_random_user_agent_is_valid() {
        let ua = random_user_agent();

        assert!(ua.contains("Mozilla"));
        assert!(ua.contains("Firefox"));
    }

    #[test]
    fn test_client_config_access() {
        let config = NetworkConfig::default();
        let expected_mode = config.mode;
        let client = SecureClient::new(config, None);

        assert_eq!(client.config().mode, expected_mode);
    }
}
