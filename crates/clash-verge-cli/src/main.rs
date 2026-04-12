//! Clash Verge CLI - Control TUN mode and System Proxy
//!
//! A command-line tool to manage Clash Verge settings including
//! TUN mode and system proxy configuration.

use anyhow::{Context as _, Result};
use clap::{Parser, Subcommand};
use colored::Colorize as _;
use serde::{Deserialize, Serialize};
use smartstring::alias::String as SmartString;
use std::path::PathBuf;

/// Default application identifier
const APP_ID: &str = "io.github.clash-verge-rev.clash-verge-rev";

/// Singleton server port for IPC with the running app
const SINGLETON_PORT: u16 = 33331;

/// Default external controller port
const DEFAULT_CONTROLLER: &str = "127.0.0.1:9097";

/// CLI arguments
#[derive(Parser)]
#[command(
    name = "cvr",
    about = "Clash Verge CLI - Control TUN mode and System Proxy",
    version,
    author
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Use portable mode configuration path
    #[arg(long, global = true)]
    portable: bool,

    /// Custom config directory path
    #[arg(long, global = true, value_name = "PATH")]
    config_dir: Option<PathBuf>,

    /// External controller address (default: 127.0.0.1:9097)
    #[arg(long, global = true, value_name = "ADDR")]
    controller: Option<String>,

    /// Secret for API authentication
    #[arg(long, global = true, value_name = "SECRET")]
    secret: Option<String>,
}

/// Available commands
#[derive(Subcommand)]
enum Commands {
    /// Enable TUN mode
    #[command(alias = "et")]
    EnableTun,
    /// Disable TUN mode
    #[command(alias = "dt")]
    DisableTun,
    /// Enable system proxy
    #[command(alias = "es")]
    EnableSysProxy,
    /// Disable system proxy
    #[command(alias = "ds")]
    DisableSysProxy,
    /// Show current status
    #[command(alias = "st")]
    Status,
    /// Toggle TUN mode (enable if disabled, disable if enabled)
    #[command(alias = "tt")]
    ToggleTun,
    /// Toggle system proxy (enable if disabled, disable if enabled)
    #[command(alias = "ts")]
    ToggleSysProxy,
}

/// Clash configuration structure (subset for CLI)
#[derive(Default, Debug, Clone, Deserialize, Serialize)]
struct ClashConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    secret: Option<SmartString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    external_controller: Option<SmartString>,
    #[serde(flatten)]
    _extra: serde_json::Value,
}

impl ClashConfig {
    fn _get_controller(&self) -> String {
        self.external_controller
            .as_deref()
            .map(|s| {
                let s = s.trim();
                if s.starts_with(':') {
                    format!("127.0.0.1{s}")
                } else if s.is_empty() {
                    DEFAULT_CONTROLLER.to_string()
                } else {
                    s.to_string()
                }
            })
            .unwrap_or_else(|| DEFAULT_CONTROLLER.to_string())
    }

    fn get_secret(&self) -> Option<String> {
        self.secret.as_deref().map(|s| s.to_string())
    }
}

/// Verge configuration structure (subset of IVerge)
#[derive(Default, Debug, Clone, Deserialize, Serialize)]
struct VergeConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    enable_tun_mode: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    enable_system_proxy: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    verge_mixed_port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    proxy_host: Option<SmartString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    proxy_auto_config: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system_proxy_bypass: Option<SmartString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    use_default_bypass: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    enable_proxy_guard: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    proxy_guard_duration: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pac_file_content: Option<SmartString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    secret: Option<SmartString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    external_controller: Option<SmartString>,
    #[serde(flatten)]
    _extra: serde_json::Value,
}

impl VergeConfig {
    fn template() -> Self {
        Self {
            enable_tun_mode: Some(false),
            enable_system_proxy: Some(false),
            verge_mixed_port: Some(7897),
            proxy_host: Some("127.0.0.1".into()),
            proxy_auto_config: Some(false),
            use_default_bypass: Some(true),
            enable_proxy_guard: Some(false),
            proxy_guard_duration: Some(30),
            pac_file_content: Some(
                r#"function FindProxyForURL(url, host) {
  return "PROXY 127.0.0.1:%mixed-port%; SOCKS5 127.0.0.1:%mixed-port%; DIRECT;";
}"#
                .into(),
            ),
            ..Default::default()
        }
    }
}

/// Configuration manager
struct ConfigManager {
    config_path: PathBuf,
}

impl ConfigManager {
    /// Initialize config manager with the appropriate path
    fn new(portable: bool, custom_dir: Option<PathBuf>) -> Result<Self> {
        let config_path = if let Some(dir) = custom_dir {
            dir.join("verge.yaml")
        } else if portable {
            let exe_path = std::env::current_exe()?;
            let exe_dir = exe_path
                .parent()
                .context("Failed to get executable directory")?;
            exe_dir.join(".config").join(APP_ID).join("verge.yaml")
        } else {
            dirs::data_dir()
                .context("Failed to get data directory")?
                .join(APP_ID)
                .join("verge.yaml")
        };

        Ok(Self { config_path })
    }

    /// Get the configuration directory
    fn config_dir(&self) -> PathBuf {
        self.config_path
            .parent()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
    }

    /// Ensure the config directory exists
    fn _ensure_dir(&self) -> Result<()> {
        if let Some(parent) = self.config_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
        }
        Ok(())
    }

    /// Read the current configuration
    fn read(&self) -> Result<VergeConfig> {
        if !self.config_path.exists() {
            return Ok(VergeConfig::template());
        }

        let content = std::fs::read_to_string(&self.config_path).with_context(|| {
            format!(
                "Failed to read config file: {}",
                self.config_path.display()
            )
        })?;

        let config: VergeConfig = serde_yaml_ng::from_str(&content).with_context(|| {
            format!(
                "Failed to parse config file: {}",
                self.config_path.display()
            )
        })?;

        Ok(config)
    }

    /// Write configuration to file
    fn _write(&self, config: &VergeConfig) -> Result<()> {
        self._ensure_dir()?;

        let content = serde_yaml_ng::to_string(config)?;
        std::fs::write(&self.config_path, format!("# Clash Verge Config\n{}", content))
            .with_context(|| {
                format!(
                    "Failed to write config file: {}",
                    self.config_path.display()
                )
            })?;

        Ok(())
    }

    /// Read the clash configuration (for secret/controller)
    fn read_clash_config(&self) -> Result<ClashConfig> {
        let clash_path = self.config_dir().join("config.yaml");

        if !clash_path.exists() {
            return Ok(ClashConfig::default());
        }

        let content = std::fs::read_to_string(&clash_path).with_context(|| {
            format!(
                "Failed to read clash config file: {}",
                clash_path.display()
            )
        })?;

        let config: ClashConfig = serde_yaml_ng::from_str(&content).with_context(|| {
            format!(
                "Failed to parse clash config file: {}",
                clash_path.display()
            )
        })?;

        Ok(config)
    }
}

/// Singleton client for communicating with the running Clash Verge app
struct SingletonClient {
    base_url: String,
}

impl SingletonClient {
    fn new() -> Self {
        Self {
            base_url: format!("http://127.0.0.1:{}", SINGLETON_PORT),
        }
    }

    /// Check if the app is running
    async fn is_running(&self) -> bool {
        let client = reqwest::Client::new();
        client
            .get(format!("{}/commands/status", self.base_url))
            .timeout(std::time::Duration::from_millis(500))
            .send()
            .await
            .is_ok()
    }

    /// Enable TUN mode via the app
    async fn enable_tun(&self) -> Result<()> {
        let client = reqwest::Client::new();
        let url = format!("{}/commands/enable/tun", self.base_url);

        let response = client
            .post(&url)
            .send()
            .await
            .with_context(|| "Failed to connect to Clash Verge app. Is it running?")?;

        if !response.status().is_success() {
            anyhow::bail!("Failed to enable TUN: HTTP {}", response.status());
        }

        let result: serde_json::Value = response.json().await?;
        if !result.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
            let error = result
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown error");
            anyhow::bail!("Failed to enable TUN: {}", error);
        }

        Ok(())
    }

    /// Disable TUN mode via the app
    async fn disable_tun(&self) -> Result<()> {
        let client = reqwest::Client::new();
        let url = format!("{}/commands/disable/tun", self.base_url);

        let response = client
            .post(&url)
            .send()
            .await
            .with_context(|| "Failed to connect to Clash Verge app. Is it running?")?;

        if !response.status().is_success() {
            anyhow::bail!("Failed to disable TUN: HTTP {}", response.status());
        }

        let result: serde_json::Value = response.json().await?;
        if !result.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
            let error = result
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown error");
            anyhow::bail!("Failed to disable TUN: {}", error);
        }

        Ok(())
    }

    /// Toggle TUN mode via the app
    async fn toggle_tun(&self) -> Result<bool> {
        let client = reqwest::Client::new();
        let url = format!("{}/commands/toggle/tun", self.base_url);

        let response = client
            .post(&url)
            .send()
            .await
            .with_context(|| "Failed to connect to Clash Verge app. Is it running?")?;

        if !response.status().is_success() {
            anyhow::bail!("Failed to toggle TUN: HTTP {}", response.status());
        }

        let result: serde_json::Value = response.json().await?;
        if !result.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
            let error = result
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown error");
            anyhow::bail!("Failed to toggle TUN: {}", error);
        }

        Ok(result
            .get("enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(false))
    }

    /// Enable system proxy via the app
    async fn enable_sysproxy(&self) -> Result<()> {
        let client = reqwest::Client::new();
        let url = format!("{}/commands/enable/sysproxy", self.base_url);

        let response = client
            .post(&url)
            .send()
            .await
            .with_context(|| "Failed to connect to Clash Verge app. Is it running?")?;

        if !response.status().is_success() {
            anyhow::bail!("Failed to enable system proxy: HTTP {}", response.status());
        }

        let result: serde_json::Value = response.json().await?;
        if !result.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
            let error = result
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown error");
            anyhow::bail!("Failed to enable system proxy: {}", error);
        }

        Ok(())
    }

    /// Disable system proxy via the app
    async fn disable_sysproxy(&self) -> Result<()> {
        let client = reqwest::Client::new();
        let url = format!("{}/commands/disable/sysproxy", self.base_url);

        let response = client
            .post(&url)
            .send()
            .await
            .with_context(|| "Failed to connect to Clash Verge app. Is it running?")?;

        if !response.status().is_success() {
            anyhow::bail!("Failed to disable system proxy: HTTP {}", response.status());
        }

        let result: serde_json::Value = response.json().await?;
        if !result.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
            let error = result
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown error");
            anyhow::bail!("Failed to disable system proxy: {}", error);
        }

        Ok(())
    }

    /// Toggle system proxy via the app
    async fn toggle_sysproxy(&self) -> Result<bool> {
        let client = reqwest::Client::new();
        let url = format!("{}/commands/toggle/sysproxy", self.base_url);

        let response = client
            .post(&url)
            .send()
            .await
            .with_context(|| "Failed to connect to Clash Verge app. Is it running?")?;

        if !response.status().is_success() {
            anyhow::bail!("Failed to toggle system proxy: HTTP {}", response.status());
        }

        let result: serde_json::Value = response.json().await?;
        if !result.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
            let error = result
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown error");
            anyhow::bail!("Failed to toggle system proxy: {}", error);
        }

        Ok(result
            .get("enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(false))
    }

    /// Get current status from the app
    async fn get_status(&self) -> Result<StatusResponse> {
        let client = reqwest::Client::new();
        let url = format!("{}/commands/status", self.base_url);

        let response = client
            .get(&url)
            .send()
            .await
            .with_context(|| "Failed to connect to Clash Verge app. Is it running?")?;

        if !response.status().is_success() {
            anyhow::bail!("Failed to get status: HTTP {}", response.status());
        }

        let result: StatusResponse = response.json().await?;
        Ok(result)
    }
}

/// Status response from the app
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct StatusResponse {
    ok: bool,
    enable_tun_mode: bool,
    enable_system_proxy: bool,
    verge_mixed_port: u16,
    proxy_auto_config: bool,
}

/// Mihomo API client (for reading runtime status only)
#[allow(dead_code)]
struct MihomoClient {
    base_url: String,
    secret: Option<String>,
}

#[allow(dead_code)]
impl MihomoClient {
    fn new(controller: String, secret: Option<String>) -> Self {
        let base_url = format!("http://{}", controller);
        Self { base_url, secret }
    }

    /// Get headers with authentication
    fn get_headers(&self) -> reqwest::header::HeaderMap {
        let mut headers = reqwest::header::HeaderMap::new();
        if let Some(secret) = &self.secret
            && let Ok(val) = format!("Bearer {}", secret).parse()
        {
            headers.insert("Authorization", val);
        }
        headers
    }

    /// Get current configs
    async fn get_configs(&self) -> Result<serde_json::Value> {
        let client = reqwest::Client::new();
        let url = format!("{}/configs", self.base_url);

        let response = client
            .get(&url)
            .headers(self.get_headers())
            .send()
            .await
            .with_context(|| format!("Failed to connect to {}", self.base_url))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            anyhow::bail!("API error {}: {}", status, text);
        }

        response.json().await.context("Failed to parse response")
    }
}

/// System proxy manager
#[allow(dead_code)]
struct SysProxyManager {
    port: u16,
    host: String,
}

#[allow(dead_code)]
impl SysProxyManager {
    const fn new(port: u16, host: String) -> Self {
        Self { port, host }
    }

    /// Apply system proxy settings directly
    fn apply(&self, enabled: bool, pac_mode: bool) -> Result<()> {
        let bypass = self.get_bypass_string();

        let sys_proxy = sysproxy::Sysproxy {
            enable: enabled && !pac_mode,
            host: self.host.clone(),
            port: self.port,
            bypass,
        };

        let auto_proxy = sysproxy::Autoproxy {
            enable: enabled && pac_mode,
            url: format!("http://{}:{}/commands/pac", self.host, SINGLETON_PORT),
        };

        sys_proxy.set_system_proxy().with_context(|| {
            if cfg!(target_os = "macos") {
                "Failed to set system proxy. Try running with sudo."
            } else if cfg!(target_os = "linux") {
                "Failed to set system proxy. Check your desktop environment settings."
            } else {
                "Failed to set system proxy"
            }
        })?;

        auto_proxy
            .set_auto_proxy()
            .with_context(|| "Failed to set auto proxy")?;

        Ok(())
    }

    /// Get the default bypass string based on OS
    fn get_bypass_string(&self) -> String {
        #[cfg(target_os = "windows")]
        {
            "localhost;127.*;192.168.*;10.*;172.16.*;172.17.*;172.18.*;172.19.*;172.20.*;172.21.*;172.22.*;172.23.*;172.24.*;172.25.*;172.26.*;172.27.*;172.28.*;172.29.*;172.30.*;172.31.*;<local>".to_string()
        }
        #[cfg(target_os = "linux")]
        {
            "localhost,127.0.0.1,192.168.0.0/16,10.0.0.0/8,172.16.0.0/12,::1".to_string()
        }
        #[cfg(target_os = "macos")]
        {
            "127.0.0.1,192.168.0.0/16,10.0.0.0/8,172.16.0.0/12,localhost,*.local,*.crashlytics.com,<local>".to_string()
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
        {
            "localhost,127.0.0.1".to_string()
        }
    }
}

/// Print current status
fn print_status(
    config: &VergeConfig,
    app_status: Option<&StatusResponse>,
    controller: &str,
) {
    println!("{}", "╔══════════════════════════════════════════╗".bright_blue());
    println!("{}", "║        Clash Verge Status                ║".bright_blue());
    println!("{}", "╚══════════════════════════════════════════╝".bright_blue());

    // TUN status from app if available, otherwise from config
    if let Some(status) = app_status {
        let tun_status = if status.enable_tun_mode {
            "ENABLED".green()
        } else {
            "DISABLED".red()
        };
        println!("  TUN Mode:           {} (runtime)", tun_status);
    } else {
        let tun_status = if config.enable_tun_mode.unwrap_or(false) {
            "ENABLED".green()
        } else {
            "DISABLED".red()
        };
        println!("  TUN Mode:           {} (config)", tun_status);
    }

    let sysproxy_status = if config.enable_system_proxy.unwrap_or(false) {
        "ENABLED".green()
    } else {
        "DISABLED".red()
    };
    println!("  System Proxy:       {}", sysproxy_status);

    let pac_mode = if config.proxy_auto_config.unwrap_or(false) {
        "PAC".cyan()
    } else {
        "GLOBAL".cyan()
    };
    println!("  Proxy Mode:         {}", pac_mode);

    let port = config.verge_mixed_port.unwrap_or(7897);
    println!("  Mixed Port:         {}", port.to_string().cyan());

    let host = config.proxy_host.as_deref().unwrap_or("127.0.0.1");
    println!("  Proxy Host:         {}", host.cyan());

    println!("  Controller:         {}", controller.cyan());

    // Show current system proxy settings
    println!("\n{}", "System Proxy Settings:".bright_blue());
    match get_current_system_proxy() {
        Ok((sys, auto)) => {
            if auto.enable {
                println!("  Auto Proxy URL:     {}", auto.url.green());
            } else if sys.enable {
                println!(
                    "  HTTP Proxy:         {}:{}",
                    sys.host.green(),
                    sys.port.to_string().green()
                );
                println!("  Bypass:             {}", sys.bypass.dimmed());
            } else {
                println!("  {}", "No system proxy configured".dimmed());
            }
        }
        Err(e) => {
            println!("  {}", format!("Error reading system proxy: {}", e).red());
        }
    }
}

/// Get current system proxy settings
fn get_current_system_proxy() -> Result<(sysproxy::Sysproxy, sysproxy::Autoproxy)> {
    let sys = sysproxy::Sysproxy::get_system_proxy()
        .map_err(|e| anyhow::anyhow!("Failed to get system proxy: {}", e))?;
    let auto = sysproxy::Autoproxy::get_auto_proxy()
        .map_err(|e| anyhow::anyhow!("Failed to get auto proxy: {}", e))?;
    Ok((sys, auto))
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize config manager
    let config_manager = ConfigManager::new(cli.portable, cli.config_dir)?;

    // Read config for settings
    let config = config_manager.read()?;

    // Read clash config for secret/controller
    let clash_config = config_manager.read_clash_config().unwrap_or_default();

    // Get controller and secret from CLI args, verge config, or clash config
    let controller = cli
        .controller
        .or_else(|| config.external_controller.as_deref().map(|s| s.to_string()))
        .or_else(|| clash_config.external_controller.as_deref().map(|s| s.to_string()))
        .unwrap_or_else(|| DEFAULT_CONTROLLER.to_string());

    let secret = cli
        .secret
        .or_else(|| config.secret.as_deref().map(|s| s.to_string()))
        .or_else(|| clash_config.get_secret());

    // Create singleton client for app communication
    let singleton = SingletonClient::new();

    // Create mihomo client for status reading only
    let _mihomo = MihomoClient::new(controller.clone(), secret);

    // Show config path in verbose mode or for status
    let show_path = matches!(cli.command, Commands::Status);
    if show_path {
        println!("Config path: {}", config_manager.config_dir().display());
        println!();
    }

    match cli.command {
        Commands::EnableTun => {
            // Check if app is running
            if !singleton.is_running().await {
                println!(
                    "{}",
                    "! Clash Verge app is not running. Cannot enable TUN.".yellow()
                );
                return Ok(());
            }

            // Enable via app
            match singleton.enable_tun().await {
                Ok(_) => {
                    println!("{}", "✓ TUN mode enabled".green());
                }
                Err(e) => {
                    println!("{}", format!("! Failed to enable TUN: {}", e).yellow());
                }
            }
        }
        Commands::DisableTun => {
            // Check if app is running
            if !singleton.is_running().await {
                println!(
                    "{}",
                    "! Clash Verge app is not running. Cannot disable TUN.".yellow()
                );
                return Ok(());
            }

            // Disable via app
            match singleton.disable_tun().await {
                Ok(_) => {
                    println!("{}", "✓ TUN mode disabled".green());
                }
                Err(e) => {
                    println!("{}", format!("! Failed to disable TUN: {}", e).yellow());
                }
            }
        }
        Commands::EnableSysProxy => {
            // Check if app is running
            if !singleton.is_running().await {
                println!(
                    "{}",
                    "! Clash Verge app is not running. Cannot enable system proxy.".yellow()
                );
                return Ok(());
            }

            // Enable via app
            match singleton.enable_sysproxy().await {
                Ok(_) => {
                    println!("{}", "✓ System proxy enabled".green());
                }
                Err(e) => {
                    println!("{}", format!("! Failed to enable system proxy: {}", e).yellow());
                }
            }
        }
        Commands::DisableSysProxy => {
            // Check if app is running
            if !singleton.is_running().await {
                println!(
                    "{}",
                    "! Clash Verge app is not running. Cannot disable system proxy.".yellow()
                );
                return Ok(());
            }

            // Disable via app
            match singleton.disable_sysproxy().await {
                Ok(_) => {
                    println!("{}", "✓ System proxy disabled".green());
                }
                Err(e) => {
                    println!("{}", format!("! Failed to disable system proxy: {}", e).yellow());
                }
            }
        }
        Commands::Status => {
            // Try to get status from app
            let app_status = singleton.get_status().await.ok();
            print_status(&config, app_status.as_ref(), &controller);
        }
        Commands::ToggleTun => {
            // Check if app is running
            if !singleton.is_running().await {
                println!(
                    "{}",
                    "! Clash Verge app is not running. Cannot toggle TUN.".yellow()
                );
                return Ok(());
            }

            // Toggle via app
            match singleton.toggle_tun().await {
                Ok(enabled) => {
                    if enabled {
                        println!("{}", "✓ TUN mode enabled".green());
                    } else {
                        println!("{}", "✓ TUN mode disabled".green());
                    }
                }
                Err(e) => {
                    println!("{}", format!("! Failed to toggle TUN: {}", e).yellow());
                }
            }
        }
        Commands::ToggleSysProxy => {
            // Check if app is running
            if !singleton.is_running().await {
                println!(
                    "{}",
                    "! Clash Verge app is not running. Cannot toggle system proxy.".yellow()
                );
                return Ok(());
            }

            // Toggle via app
            match singleton.toggle_sysproxy().await {
                Ok(enabled) => {
                    if enabled {
                        println!("{}", "✓ System proxy enabled".green());
                    } else {
                        println!("{}", "✓ System proxy disabled".green());
                    }
                }
                Err(e) => {
                    println!("{}", format!("! Failed to toggle system proxy: {}", e).yellow());
                }
            }
        }
    }

    Ok(())
}
