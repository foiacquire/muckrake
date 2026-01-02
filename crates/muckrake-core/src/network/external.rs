use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::Path;
use std::process::{ExitStatus, Stdio};
use tokio::process::Command;

use super::config::NetworkConfig;

#[derive(Debug, thiserror::Error)]
pub enum ExternalError {
    #[error("Tor is not ready - refusing to run external command")]
    TorNotReady,
    #[error("Command not found: {0}")]
    CommandNotFound(String),
    #[error("Command failed with exit code: {0:?}")]
    CommandFailed(Option<i32>),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub type ExternalResult<T> = Result<T, ExternalError>;

/// Output from an external command
#[derive(Debug)]
pub struct CommandOutput {
    pub status: ExitStatus,
    pub stdout: String,
    pub stderr: String,
}

/// Wrapper for running external commands through Tor proxy
///
/// SECURITY: All external commands that make network requests MUST use this
/// wrapper to ensure traffic goes through Tor.
pub struct SecureCommand {
    config: NetworkConfig,
    tor_ready: bool,
}

impl SecureCommand {
    pub fn new(config: NetworkConfig, tor_ready: bool) -> Self {
        Self { config, tor_ready }
    }

    pub fn set_tor_ready(&mut self, ready: bool) {
        self.tor_ready = ready;
    }

    fn check_tor_ready(&self) -> ExternalResult<()> {
        if self.config.mode.requires_tor() && !self.tor_ready {
            return Err(ExternalError::TorNotReady);
        }
        Ok(())
    }

    fn proxy_env_vars(&self) -> HashMap<String, String> {
        let proxy_url = self.config.socks_proxy_url();
        let mut env = HashMap::new();

        env.insert("ALL_PROXY".to_string(), proxy_url.clone());
        env.insert("HTTP_PROXY".to_string(), proxy_url.clone());
        env.insert("HTTPS_PROXY".to_string(), proxy_url.clone());
        env.insert("http_proxy".to_string(), proxy_url.clone());
        env.insert("https_proxy".to_string(), proxy_url.clone());
        env.insert("all_proxy".to_string(), proxy_url);

        env.insert("NO_PROXY".to_string(), "localhost,127.0.0.1,::1".to_string());
        env.insert("no_proxy".to_string(), "localhost,127.0.0.1,::1".to_string());

        env
    }

    fn build_command<S: AsRef<OsStr>>(&self, program: S) -> Command {
        let mut cmd = Command::new(program);

        if self.config.mode.requires_tor() {
            for (key, value) in self.proxy_env_vars() {
                cmd.env(key, value);
            }
        }

        cmd
    }

    /// Run a generic command with proxy environment variables
    pub async fn run<S, I, A>(&self, program: S, args: I) -> ExternalResult<CommandOutput>
    where
        S: AsRef<OsStr>,
        I: IntoIterator<Item = A>,
        A: AsRef<OsStr>,
    {
        self.check_tor_ready()?;

        let output = self.build_command(program)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;

        Ok(CommandOutput {
            status: output.status,
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        })
    }

    /// Run yt-dlp with proper Tor proxy configuration
    pub async fn yt_dlp<P: AsRef<Path>>(&self, url: &str, output_path: P, extra_args: &[&str]) -> ExternalResult<CommandOutput> {
        self.check_tor_ready()?;

        let proxy_url = self.config.socks_proxy_url();
        let output_template = output_path.as_ref().to_string_lossy();

        let mut args = vec![
            "--proxy", &proxy_url,
            "--geo-bypass",
            "-o", &output_template,
        ];

        args.extend_from_slice(extra_args);
        args.push(url);

        self.run("yt-dlp", &args).await
    }

    /// Run curl with proper Tor proxy configuration
    pub async fn curl(&self, url: &str, extra_args: &[&str]) -> ExternalResult<CommandOutput> {
        self.check_tor_ready()?;

        let socks_addr = format!("127.0.0.1:{}", self.config.socks_port);

        let mut args = vec![
            "--socks5-hostname".to_string(),
            socks_addr,
            "--silent".to_string(),
            "--fail".to_string(),
        ];

        for arg in extra_args {
            args.push((*arg).to_string());
        }
        args.push(url.to_string());

        let args_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        self.run("curl", &args_refs).await
    }

    /// Run wget with proper Tor proxy configuration
    pub async fn wget<P: AsRef<Path>>(&self, url: &str, output_path: P, extra_args: &[&str]) -> ExternalResult<CommandOutput> {
        self.check_tor_ready()?;

        let output_str = output_path.as_ref().to_string_lossy().to_string();
        let proxy_url = self.config.socks_proxy_url();
        let use_proxy = "use_proxy=yes".to_string();
        let http_proxy = format!("http_proxy={}", proxy_url);
        let https_proxy = format!("https_proxy={}", proxy_url);

        let mut args = vec![
            "-e".to_string(), use_proxy,
            "-e".to_string(), http_proxy,
            "-e".to_string(), https_proxy,
            "-O".to_string(), output_str,
        ];

        for arg in extra_args {
            args.push((*arg).to_string());
        }
        args.push(url.to_string());

        let args_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        self.run("wget", &args_refs).await
    }

    /// Run git with proper Tor proxy configuration
    pub async fn git(&self, args: &[&str]) -> ExternalResult<CommandOutput> {
        self.check_tor_ready()?;

        let proxy_url = self.config.socks_proxy_url();

        let mut cmd = self.build_command("git");
        cmd.env("GIT_PROXY_COMMAND", format!("nc -x 127.0.0.1:{} %h %p", self.config.socks_port));
        cmd.args(["-c", &format!("http.proxy={}", proxy_url)]);
        cmd.args(["-c", &format!("https.proxy={}", proxy_url)]);
        cmd.args(args);

        let output = cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;

        Ok(CommandOutput {
            status: output.status,
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        })
    }

    /// Run aria2c with proper Tor proxy configuration (for parallel downloads)
    pub async fn aria2c<P: AsRef<Path>>(&self, url: &str, output_path: P, extra_args: &[&str]) -> ExternalResult<CommandOutput> {
        self.check_tor_ready()?;

        let output_str = output_path.as_ref().to_string_lossy().to_string();
        let proxy_url = self.config.socks_proxy_url();

        let mut args = vec![
            "--all-proxy".to_string(), proxy_url,
            "--out".to_string(), output_str,
            "--check-certificate=true".to_string(),
        ];

        for arg in extra_args {
            args.push((*arg).to_string());
        }
        args.push(url.to_string());

        let args_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        self.run("aria2c", &args_refs).await
    }

    /// Run gallery-dl with proper Tor proxy configuration
    pub async fn gallery_dl<P: AsRef<Path>>(&self, url: &str, output_path: P, extra_args: &[&str]) -> ExternalResult<CommandOutput> {
        self.check_tor_ready()?;

        let proxy_url = self.config.socks_proxy_url();
        let output_str = output_path.as_ref().to_string_lossy();

        let mut args = vec![
            "--proxy", &proxy_url,
            "-d", &output_str,
        ];

        args.extend_from_slice(extra_args);
        args.push(url);

        self.run("gallery-dl", &args).await
    }

    /// Open a URL in Tor Browser
    pub async fn open_in_tor_browser(&self, url: &str) -> ExternalResult<CommandOutput> {
        let possible_paths = [
            "torbrowser-launcher",
            "tor-browser",
            "/usr/bin/torbrowser-launcher",
            "/opt/tor-browser/Browser/start-tor-browser",
            "start-tor-browser",
        ];

        for path in possible_paths {
            if let Ok(output) = self.run(path, &[url]).await {
                return Ok(output);
            }
        }

        #[cfg(target_os = "linux")]
        {
            self.run("xdg-open", &[url]).await
        }

        #[cfg(target_os = "macos")]
        {
            self.run("open", &["-a", "Tor Browser", url]).await
        }

        #[cfg(target_os = "windows")]
        {
            self.run("cmd", &["/c", "start", "", url]).await
        }

        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
        {
            Err(ExternalError::CommandNotFound("tor-browser".to_string()))
        }
    }
}

/// Handle that can be cloned and shared
pub struct SecureCommandHandle {
    inner: std::sync::Arc<tokio::sync::RwLock<SecureCommand>>,
}

impl SecureCommandHandle {
    pub fn new(config: NetworkConfig, tor_ready: bool) -> Self {
        Self {
            inner: std::sync::Arc::new(tokio::sync::RwLock::new(SecureCommand::new(config, tor_ready))),
        }
    }

    pub async fn set_tor_ready(&self, ready: bool) {
        self.inner.write().await.set_tor_ready(ready);
    }

    pub async fn run<S, I, A>(&self, program: S, args: I) -> ExternalResult<CommandOutput>
    where
        S: AsRef<OsStr>,
        I: IntoIterator<Item = A>,
        A: AsRef<OsStr>,
    {
        self.inner.read().await.run(program, args).await
    }

    pub async fn yt_dlp<P: AsRef<Path>>(&self, url: &str, output_path: P, extra_args: &[&str]) -> ExternalResult<CommandOutput> {
        self.inner.read().await.yt_dlp(url, output_path, extra_args).await
    }

    pub async fn curl(&self, url: &str, extra_args: &[&str]) -> ExternalResult<CommandOutput> {
        self.inner.read().await.curl(url, extra_args).await
    }

    pub async fn open_in_tor_browser(&self, url: &str) -> ExternalResult<CommandOutput> {
        self.inner.read().await.open_in_tor_browser(url).await
    }
}

impl Clone for SecureCommandHandle {
    fn clone(&self) -> Self {
        Self {
            inner: std::sync::Arc::clone(&self.inner),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network::config::NetworkMode;

    fn make_command(mode: NetworkMode, tor_ready: bool) -> SecureCommand {
        let config = NetworkConfig {
            mode,
            ..Default::default()
        };
        SecureCommand::new(config, tor_ready)
    }

    #[test]
    fn test_tor_ready_check() {
        let cmd = make_command(NetworkMode::TorSnowflake, false);
        assert!(matches!(cmd.check_tor_ready(), Err(ExternalError::TorNotReady)));

        let cmd = make_command(NetworkMode::TorSnowflake, true);
        assert!(cmd.check_tor_ready().is_ok());

        let cmd = make_command(NetworkMode::DirectUnsafe, false);
        assert!(cmd.check_tor_ready().is_ok());
    }

    #[test]
    fn test_proxy_env_vars() {
        let config = NetworkConfig {
            socks_port: 9150,
            ..Default::default()
        };
        let cmd = SecureCommand::new(config, true);
        let env = cmd.proxy_env_vars();

        assert_eq!(env.get("ALL_PROXY"), Some(&"socks5://127.0.0.1:9150".to_string()));
        assert_eq!(env.get("HTTP_PROXY"), Some(&"socks5://127.0.0.1:9150".to_string()));
        assert_eq!(env.get("HTTPS_PROXY"), Some(&"socks5://127.0.0.1:9150".to_string()));
        assert_eq!(env.get("NO_PROXY"), Some(&"localhost,127.0.0.1,::1".to_string()));
    }

    #[test]
    fn test_set_tor_ready() {
        let mut cmd = make_command(NetworkMode::TorSnowflake, false);
        assert!(cmd.check_tor_ready().is_err());

        cmd.set_tor_ready(true);
        assert!(cmd.check_tor_ready().is_ok());

        cmd.set_tor_ready(false);
        assert!(cmd.check_tor_ready().is_err());
    }

    #[tokio::test]
    async fn test_run_refuses_when_tor_not_ready() {
        let cmd = make_command(NetworkMode::TorSnowflake, false);

        let result = cmd.run("echo", &["test"]).await;

        assert!(matches!(result, Err(ExternalError::TorNotReady)));
    }

    #[tokio::test]
    async fn test_run_allowed_in_direct_mode() {
        let cmd = make_command(NetworkMode::DirectUnsafe, false);

        let result = cmd.run("echo", &["hello"]).await;

        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.status.success());
        assert!(output.stdout.contains("hello"));
    }

    #[tokio::test]
    async fn test_run_with_tor_ready() {
        let cmd = make_command(NetworkMode::TorSnowflake, true);

        let result = cmd.run("echo", &["hello"]).await;

        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.status.success());
    }
}
