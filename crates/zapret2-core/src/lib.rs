//! Core library for managing zapret2.
//!
//! Provides abstractions over:
//! - nfqws2 process lifecycle (start, stop, monitor)
//! - nftables/iptables firewall rule management
//! - Configuration parsing (zapret2 config format)
//! - Profile/strategy management
//!
//! # Example
//!
//! ```no_run
//! use zapret2_core::ZapretController;
//! use std::path::PathBuf;
//!
//! # async fn example() -> anyhow::Result<()> {
//! let mut controller = ZapretController::new(None)?;
//! let status = controller.status().await;
//! println!("Daemon running: {}", status.daemon_running);
//!
//! // Start zapret2 (requires root for nftables)
//! // controller.start().await?;
//!
//! // Stop zapret2
//! // controller.stop().await?;
//! # Ok(())
//! # }
//! ```

pub mod config;
pub mod daemon;
pub mod firewall;
pub mod profile;

use std::path::PathBuf;
use thiserror::Error;

/// Errors that can occur when managing zapret2.
#[derive(Error, Debug)]
pub enum ZapretError {
    /// The nfqws2 binary was not found at the expected path.
    #[error("daemon not found: {0}")]
    DaemonNotFound(PathBuf),
    /// Error applying or removing firewall rules.
    #[error("firewall error: {0}")]
    FirewallError(String),
    /// Error parsing zapret2 configuration.
    #[error("config error: {0}")]
    ConfigError(String),
    /// Generic I/O error.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// Error spawning or managing the nfqws2 process.
    #[error("process error: {0}")]
    ProcessError(String),
}

/// Result type for zapret2 operations.
pub type Result<T> = std::result::Result<T, ZapretError>;

/// Default path where zapret2 is installed.
pub const DEFAULT_ZAPRET_BASE: &str = "/opt/zapret2";

/// Default path to zapret2 configuration file.
pub const DEFAULT_CONFIG_PATH: &str = "/opt/zapret2/config";

/// Default path to nfqws2 binary.
pub const DEFAULT_NFQWS2_BIN: &str = "/opt/zapret2/nfq2/nfqws2";

/// Main controller coordinating daemon and firewall.
///
/// This is the primary entry point for programmatic zapret2 management.
///
/// # Example
///
/// ```no_run
/// use zapret2_core::ZapretController;
/// use std::path::PathBuf;
///
/// # async fn example() -> anyhow::Result<()> {
/// let mut controller = ZapretController::new(Some(PathBuf::from("/opt/zapret2/config")))?;
///
/// // Check status
/// let status = controller.status().await;
/// println!("Running: {:?}", status);
///
/// // Start (requires root for nftables)
/// // controller.start().await?;
/// # Ok(())
/// # }
/// ```
pub struct ZapretController {
    config: config::ZapretConfig,
    daemon: daemon::DaemonManager,
    firewall: firewall::FirewallManager,
}

impl ZapretController {
    /// Creates a new controller, loading configuration from the given path.
    ///
    /// If `config_path` is `None`, uses `DEFAULT_CONFIG_PATH`.
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

    /// Returns current status of daemon and firewall.
    pub async fn status(&self) -> Status {
        Status {
            daemon_running: self.daemon.is_running(),
            firewall_active: self.firewall.is_active().await,
            current_profile: self.config.current_profile.clone(),
        }
    }

    /// Start the zapret2 daemon and apply firewall rules.
    ///
    /// Requires root privileges for nftables operations.
    pub async fn start(&mut self) -> Result<()> {
        tracing::info!("starting zapret2");
        self.firewall.apply().await?;
        self.daemon.start().await?;
        Ok(())
    }

    /// Stop the zapret2 daemon and remove firewall rules.
    pub async fn stop(&mut self) -> Result<()> {
        tracing::info!("stopping zapret2");
        self.daemon.stop().await?;
        self.firewall.remove().await?;
        Ok(())
    }

    /// Restart zapret2: stop then start.
    pub async fn restart(&mut self) -> Result<()> {
        self.stop().await?;
        self.start().await
    }
}

/// Current status of zapret2 components.
#[derive(Debug, Clone, Default)]
pub struct Status {
    /// Whether nfqws2 process is running.
    pub daemon_running: bool,
    /// Whether nftables/iptables rules are applied.
    pub firewall_active: bool,
    /// Current profile name, if any.
    pub current_profile: Option<String>,
}