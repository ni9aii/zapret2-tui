//! Core library for managing zapret2.
//!
//! Provides abstractions over:
//! - nfqws2 process lifecycle (start, stop, monitor)
//! - nftables/iptables firewall rule management
//! - Configuration parsing (zapret2 config format)
//! - Profile/strategy management

pub mod config;
pub mod daemon;
pub mod firewall;
pub mod profile;

use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ZapretError {
    #[error("daemon not found: {0}")]
    DaemonNotFound(PathBuf),
    #[error("firewall error: {0}")]
    FirewallError(String),
    #[error("config error: {0}")]
    ConfigError(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("process error: {0}")]
    ProcessError(String),
}

pub type Result<T> = std::result::Result<T, ZapretError>;

/// Default paths for zapret2 installation
pub const DEFAULT_ZAPRET_BASE: &str = "/opt/zapret2";
pub const DEFAULT_CONFIG_PATH: &str = "/opt/zapret2/config";
pub const DEFAULT_NFQWS2_BIN: &str = "/opt/zapret2/nfq2/nfqws2";

/// Main controller coordinating daemon and firewall
pub struct ZapretController {
    config: config::ZapretConfig,
    daemon: daemon::DaemonManager,
    firewall: firewall::FirewallManager,
}

impl ZapretController {
    pub fn new(config_path: Option<PathBuf>) -> Result<Self> {
        let config = config::ZapretConfig::load(config_path)?;
        let daemon = daemon::DaemonManager::new(&config);
        let firewall = firewall::FirewallManager::new(&config);

        Ok(Self {
            config,
            daemon,
            firewall,
        })
    }

    pub fn status(&self) -> Status {
        Status {
            daemon_running: self.daemon.is_running(),
            firewall_active: self.firewall.is_active(),
            current_profile: self.config.current_profile.clone(),
        }
    }

    pub async fn start(&mut self) -> Result<()> {
        tracing::info!("starting zapret2");
        self.firewall.apply().await?;
        self.daemon.start().await?;
        Ok(())
    }

    pub async fn stop(&mut self) -> Result<()> {
        tracing::info!("stopping zapret2");
        self.daemon.stop().await?;
        self.firewall.remove().await?;
        Ok(())
    }

    pub async fn restart(&mut self) -> Result<()> {
        self.stop().await?;
        self.start().await
    }
}

#[derive(Debug, Clone, Default)]
pub struct Status {
    pub daemon_running: bool,
    pub firewall_active: bool,
    pub current_profile: Option<String>,
}
