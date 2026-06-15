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
use tokio::sync::mpsc;

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
    log_rx: Option<mpsc::UnboundedReceiver<String>>,
}

impl ZapretController {
    /// Creates a new controller, loading configuration from the given path.
    ///
    /// If `config_path` is `None`, uses `DEFAULT_CONFIG_PATH`.
    pub fn new(config_path: Option<PathBuf>) -> Result<Self> {
        let config = config::ZapretConfig::load(config_path)?;
        let (log_tx, log_rx) = mpsc::unbounded_channel();
        let mut daemon = daemon::DaemonManager::new(&config);
        daemon.set_log_channel(log_tx);
        let firewall = firewall::FirewallManager::new(&config);

        Ok(Self {
            config,
            daemon,
            firewall,
            log_rx: Some(log_rx),
        })
    }

    /// Get the nfqws2 log receiver. Can only be called once.
    pub fn take_log_receiver(&mut self) -> Option<mpsc::UnboundedReceiver<String>> {
        self.log_rx.take()
    }

    pub fn config(&self) -> &config::ZapretConfig {
        &self.config
    }

    pub fn daemon_pid(&self) -> Option<u32> {
        self.daemon.pid()
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

    /// Apply a profile to the runtime configuration.
    ///
    /// Validates the profile, patches the in-memory config (nfqws2 options and
    /// active profile name) and rebuilds the daemon/firewall managers so the
    /// next start/restart uses the selected profile. Does not write to
    /// `/opt/zapret2/config` and does not require root.
    ///
    /// On error the configuration is left unchanged, so a failed apply never
    /// silently mutates runtime state.
    pub fn apply_profile(&mut self, profile: &profile::Profile) -> Result<()> {
        // Validate everything before mutating any state.
        profile::ProfileManager::validate_name(&profile.name)?;
        daemon::DaemonManager::validate_opts(&profile.nfqws_opts)?;

        self.config.nfqws2_opt = profile.nfqws_opts.clone();
        self.config.current_profile = Some(profile.name.clone());

        // Preserve any running child/log channel; only refresh derived config.
        self.daemon.apply_config(&self.config);
        self.firewall = firewall::FirewallManager::new(&self.config);

        tracing::info!("applied profile '{}'", profile.name);
        Ok(())
    }

    /// The currently active profile name, if any.
    pub fn current_profile(&self) -> Option<&str> {
        self.config.current_profile.as_deref()
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

#[cfg(test)]
mod controller_tests {
    use super::*;
    use crate::profile::Profile;

    fn controller() -> ZapretController {
        // A non-existent config path yields the default config without root.
        ZapretController::new(Some(PathBuf::from("/nonexistent/zapret2/config"))).unwrap()
    }

    fn profile(name: &str, opts: &str) -> Profile {
        Profile {
            name: name.to_string(),
            description: "test".to_string(),
            strategy: "test".to_string(),
            hostlists: vec![],
            nfqws_opts: opts.to_string(),
        }
    }

    #[test]
    fn apply_profile_sets_active_profile_and_opts() {
        let mut c = controller();
        assert_eq!(c.current_profile(), None);

        c.apply_profile(&profile("yt", "--qnum=300 --dpi-desync"))
            .unwrap();

        assert_eq!(c.current_profile(), Some("yt"));
        assert_eq!(c.config().nfqws2_opt, "--qnum=300 --dpi-desync");
    }

    #[test]
    fn apply_profile_rejects_invalid_name_without_mutating_state() {
        let mut c = controller();
        let original_opts = c.config().nfqws2_opt.clone();

        assert!(c.apply_profile(&profile("../evil", "--qnum=200")).is_err());

        assert_eq!(c.current_profile(), None);
        assert_eq!(c.config().nfqws2_opt, original_opts);
    }

    #[test]
    fn apply_profile_rejects_forbidden_opts_without_mutating_state() {
        let mut c = controller();

        assert!(c.apply_profile(&profile("ok", "--rm --force")).is_err());

        assert_eq!(c.current_profile(), None);
    }
}
