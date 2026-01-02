use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::time::sleep;

use super::config::{NetworkConfig, TorClient};

#[derive(Debug, thiserror::Error)]
pub enum TorError {
    #[error("Failed to start Tor: {0}")]
    TorStartFailed(String),
    #[error("Failed to start Arti: {0}")]
    ArtiStartFailed(String),
    #[error("Failed to start Snowflake client: {0}")]
    SnowflakeStartFailed(String),
    #[error("Tor is not running")]
    NotRunning,
    #[error("Connection to Tor failed after {0} attempts")]
    ConnectionFailed(u32),
    #[error("Snowflake client not found: {0}")]
    SnowflakeNotFound(PathBuf),
    #[error("No Tor client available (tried: {0:?})")]
    NoClientAvailable(Vec<TorClient>),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub type TorResult<T> = Result<T, TorError>;

struct TorProcesses {
    tor: Option<Child>,
    snowflake: Option<Child>,
}

/// Which client is actually running
#[derive(Debug, Clone, Copy)]
pub enum ActiveClient {
    Tor,
    Arti,
    TorBrowser,
    ExistingProxy,
}

pub struct TorManager {
    config: NetworkConfig,
    processes: RwLock<TorProcesses>,
    running: AtomicBool,
    ready: AtomicBool,
    active_client: RwLock<Option<ActiveClient>>,
}

impl TorManager {
    pub fn new(config: NetworkConfig) -> Self {
        Self {
            config,
            processes: RwLock::new(TorProcesses {
                tor: None,
                snowflake: None,
            }),
            running: AtomicBool::new(false),
            ready: AtomicBool::new(false),
            active_client: RwLock::new(None),
        }
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    pub fn is_ready(&self) -> bool {
        self.ready.load(Ordering::SeqCst)
    }

    pub async fn active_client(&self) -> Option<ActiveClient> {
        *self.active_client.read().await
    }

    pub async fn start(&self) -> TorResult<()> {
        if self.running.load(Ordering::SeqCst) {
            return Ok(());
        }

        if !self.config.mode.requires_tor() {
            return Ok(());
        }

        // Find available client based on preference order
        let client = self.config.tor_client.find_available()
            .ok_or_else(|| TorError::NoClientAvailable(self.config.tor_client.order.clone()))?;

        let mut processes = self.processes.write().await;

        match client {
            TorClient::Tor => {
                self.start_c_tor(&mut processes).await?;
                *self.active_client.write().await = Some(ActiveClient::Tor);
            }
            TorClient::Arti => {
                self.start_arti(&mut processes).await?;
                *self.active_client.write().await = Some(ActiveClient::Arti);
            }
            TorClient::TorBrowser => {
                // No process to start, just verify it's running
                *self.active_client.write().await = Some(ActiveClient::TorBrowser);
            }
            TorClient::ExistingProxy => {
                // No process to start
                *self.active_client.write().await = Some(ActiveClient::ExistingProxy);
            }
        }

        self.running.store(true, Ordering::SeqCst);
        drop(processes);

        self.wait_for_ready().await?;
        Ok(())
    }

    /// Start C Tor with Snowflake pluggable transport
    async fn start_c_tor(&self, processes: &mut TorProcesses) -> TorResult<()> {
        // Start Snowflake PT if needed
        if self.config.mode.requires_pluggable_transport() {
            self.start_snowflake(processes).await?;
        }

        // Write torrc configuration
        let torrc_path = self.write_torrc().await?;

        let tor_path = which::which("tor")
            .map_err(|e| TorError::TorStartFailed(format!("tor not found: {}", e)))?;

        let mut cmd = Command::new(tor_path);
        cmd.arg("-f").arg(&torrc_path);

        cmd.stdout(Stdio::null())
            .stderr(Stdio::piped());

        let child = cmd.spawn().map_err(|e| {
            TorError::TorStartFailed(e.to_string())
        })?;

        processes.tor = Some(child);
        tracing::info!("Started C Tor");
        Ok(())
    }

    /// Write torrc configuration file
    async fn write_torrc(&self) -> TorResult<PathBuf> {
        let config_dir = self.config.tor_data_dir.clone()
            .unwrap_or_else(|| {
                dirs::config_dir()
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join("muckrake")
                    .join("tor")
            });

        tokio::fs::create_dir_all(&config_dir).await?;

        let torrc_path = config_dir.join("torrc");
        let data_dir = config_dir.join("data");
        tokio::fs::create_dir_all(&data_dir).await?;

        let sf_config = &self.config.snowflake;

        // Find snowflake-client path
        let snowflake_path = if sf_config.client_path.exists() {
            sf_config.client_path.clone()
        } else {
            which::which("snowflake-client")
                .map_err(|_| TorError::SnowflakeNotFound(sf_config.client_path.clone()))?
        };

        let mut torrc = format!(
            r#"# Muckrake Tor Configuration
# Auto-generated - modifications will be overwritten

DataDirectory {}
SocksPort {}
"#,
            data_dir.display(),
            self.config.socks_port
        );

        if self.config.mode.requires_pluggable_transport() {
            let stun_arg = sf_config.stun_servers.join(",");
            let front = sf_config.front_domain.as_deref().unwrap_or("cdn.sstatic.net");

            torrc.push_str(&format!(
                r#"
UseBridges 1
ClientTransportPlugin snowflake exec {} -url {} -front {} -ice {} -max {}

Bridge snowflake 192.0.2.3:80 2B280B23E1107BB62ABFC40DDCC8824814F80A72 fingerprint=2B280B23E1107BB62ABFC40DDCC8824814F80A72 url={} front={} ice={}
"#,
                snowflake_path.display(),
                sf_config.broker_url,
                front,
                stun_arg,
                sf_config.max_peers,
                sf_config.broker_url,
                front,
                stun_arg,
            ));
        }

        tokio::fs::write(&torrc_path, torrc).await?;
        Ok(torrc_path)
    }

    /// Start Arti with Snowflake configuration
    async fn start_arti(&self, processes: &mut TorProcesses) -> TorResult<()> {
        // Start Snowflake PT if needed (Arti uses external PT)
        if self.config.mode.requires_pluggable_transport() {
            self.start_snowflake(processes).await?;
            self.write_arti_bridge_config().await?;
        }

        let arti_path = which::which("arti")
            .map_err(|e| TorError::ArtiStartFailed(format!("arti not found: {}", e)))?;

        let mut cmd = Command::new(arti_path);
        cmd.arg("proxy");
        cmd.arg("-p").arg(self.config.socks_port.to_string());

        if let Some(ref config_dir) = self.config.arti_config_dir {
            cmd.arg("-c").arg(config_dir);
        }

        if let Some(ref data_dir) = self.config.arti_data_dir {
            cmd.arg("-d").arg(data_dir);
        }

        cmd.stdout(Stdio::null())
            .stderr(Stdio::piped());

        let child = cmd.spawn().map_err(|e| {
            TorError::ArtiStartFailed(e.to_string())
        })?;

        processes.tor = Some(child);
        tracing::info!("Started Arti");
        Ok(())
    }

    async fn start_snowflake(&self, processes: &mut TorProcesses) -> TorResult<()> {
        let sf_config = &self.config.snowflake;

        let snowflake_path = if sf_config.client_path.exists() {
            sf_config.client_path.clone()
        } else {
            which::which("snowflake-client")
                .map_err(|_| TorError::SnowflakeNotFound(sf_config.client_path.clone()))?
        };

        let mut cmd = Command::new(&snowflake_path);
        self.configure_snowflake_cmd(&mut cmd);

        let child = cmd.spawn().map_err(|e| {
            TorError::SnowflakeStartFailed(e.to_string())
        })?;

        processes.snowflake = Some(child);
        tracing::info!("Started Snowflake client");

        // Give Snowflake time to establish connections
        sleep(Duration::from_secs(3)).await;

        Ok(())
    }

    fn configure_snowflake_cmd(&self, cmd: &mut Command) {
        let sf_config = &self.config.snowflake;

        cmd.arg("-url").arg(&sf_config.broker_url);

        if let Some(ref front) = sf_config.front_domain {
            cmd.arg("-front").arg(front);
        }

        let stun_arg = sf_config.stun_servers.join(",");
        cmd.arg("-ice").arg(&stun_arg);

        cmd.arg("-max").arg(sf_config.max_peers.to_string());

        cmd.stdout(Stdio::null())
            .stderr(Stdio::piped());
    }

    async fn write_arti_bridge_config(&self) -> TorResult<()> {
        let config_dir = self.config.arti_config_dir.clone()
            .unwrap_or_else(|| {
                dirs::config_dir()
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join("muckrake")
                    .join("arti")
            });

        tokio::fs::create_dir_all(&config_dir).await?;

        let config_path = config_dir.join("arti.toml");
        let sf_config = &self.config.snowflake;

        let snowflake_path = if sf_config.client_path.exists() {
            sf_config.client_path.clone()
        } else {
            which::which("snowflake-client")
                .unwrap_or_else(|_| PathBuf::from("snowflake-client"))
        };

        let config_content = format!(
            r#"# Muckrake Arti Configuration
# Auto-generated - modifications will be overwritten

[proxy]
socks_listen = "127.0.0.1:{}"

[bridges]
enabled = true

[[bridges.transports]]
protocols = ["snowflake"]
path = "{}"
arguments = [
    "-url", "{}",
    "-front", "{}",
    "-ice", "{}",
    "-max", "{}"
]
run_on_startup = true
"#,
            self.config.socks_port,
            snowflake_path.display(),
            sf_config.broker_url,
            sf_config.front_domain.as_deref().unwrap_or("cdn.sstatic.net"),
            sf_config.stun_servers.join(","),
            sf_config.max_peers
        );

        tokio::fs::write(&config_path, config_content).await?;
        Ok(())
    }

    async fn wait_for_ready(&self) -> TorResult<()> {
        let max_attempts = 60;
        let delay = Duration::from_secs(2);

        for attempt in 1..=max_attempts {
            if self.check_socks_ready().await {
                self.ready.store(true, Ordering::SeqCst);
                tracing::info!("Tor SOCKS proxy ready on port {}", self.config.socks_port);
                return Ok(());
            }
            sleep(delay).await;

            if attempt % 10 == 0 {
                tracing::info!("Waiting for Tor... attempt {}/{}", attempt, max_attempts);
            }
        }

        Err(TorError::ConnectionFailed(max_attempts))
    }

    async fn check_socks_ready(&self) -> bool {
        use tokio::net::TcpStream;

        let addr = format!("127.0.0.1:{}", self.config.socks_port);
        TcpStream::connect(&addr).await.is_ok()
    }

    pub async fn stop(&self) -> TorResult<()> {
        let mut processes = self.processes.write().await;

        if let Some(ref mut tor) = processes.tor {
            let _ = tor.kill();
            let _ = tor.wait();
        }

        if let Some(ref mut snowflake) = processes.snowflake {
            let _ = snowflake.kill();
            let _ = snowflake.wait();
        }

        processes.tor = None;
        processes.snowflake = None;

        self.running.store(false, Ordering::SeqCst);
        self.ready.store(false, Ordering::SeqCst);
        *self.active_client.write().await = None;

        Ok(())
    }

    pub fn socks_proxy_url(&self) -> String {
        self.config.socks_proxy_url()
    }
}

impl Drop for TorManager {
    fn drop(&mut self) {
        if let Ok(mut processes) = self.processes.try_write() {
            if let Some(ref mut tor) = processes.tor {
                let _ = tor.kill();
            }
            if let Some(ref mut snowflake) = processes.snowflake {
                let _ = snowflake.kill();
            }
        }
    }
}

pub struct TorManagerHandle {
    inner: Arc<TorManager>,
}

impl TorManagerHandle {
    pub fn new(config: NetworkConfig) -> Self {
        Self {
            inner: Arc::new(TorManager::new(config)),
        }
    }

    pub async fn start(&self) -> TorResult<()> {
        self.inner.start().await
    }

    pub async fn stop(&self) -> TorResult<()> {
        self.inner.stop().await
    }

    pub fn is_ready(&self) -> bool {
        self.inner.is_ready()
    }

    pub async fn active_client(&self) -> Option<ActiveClient> {
        self.inner.active_client().await
    }

    pub fn socks_proxy_url(&self) -> String {
        self.inner.socks_proxy_url()
    }
}

impl Clone for TorManagerHandle {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}
