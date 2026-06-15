//! Privileged actions, factored as free functions.
//!
//! These are the operations that require root: applying/removing firewall
//! rules and starting/stopping the nfqws2 daemon. They are deliberately
//! stateless wrappers over [`FirewallManager`] and [`DaemonManager`] so that
//! both the privileged helper binary (`zapret2-helper`) and the in-process
//! [`DirectExecutor`](crate::privilege::DirectExecutor) call the exact same
//! audited code path. There is a single source of truth for the nftables
//! script and the nfqws2 argument whitelist.

use crate::config::ZapretConfig;
use crate::daemon::DaemonManager;
use crate::firewall::FirewallManager;
use crate::Result;

/// Verify the local environment can run zapret2 with the given config.
///
/// Currently checks that the nfqws2 binary exists. Returns an error describing
/// the first problem found. Does not require root.
pub fn check(config: &ZapretConfig) -> Result<()> {
    let bin = config.nfqws2_bin();
    if !bin.exists() {
        return Err(crate::ZapretError::DaemonNotFound(bin));
    }
    Ok(())
}

/// Apply firewall redirection rules.
pub async fn firewall_apply(config: &ZapretConfig) -> Result<()> {
    FirewallManager::new(config).apply().await
}

/// Remove firewall redirection rules.
pub async fn firewall_remove(config: &ZapretConfig) -> Result<()> {
    FirewallManager::new(config).remove().await
}

/// Start the nfqws2 daemon detached (survives the caller's exit).
pub async fn daemon_start(config: &ZapretConfig) -> Result<()> {
    DaemonManager::new(config).start_detached().await
}

/// Stop the nfqws2 daemon, discovering it via its pid file.
pub async fn daemon_stop(config: &ZapretConfig) -> Result<()> {
    DaemonManager::new(config).stop().await
}

/// Whether the nfqws2 daemon is currently running (pid-file based).
pub fn daemon_status(config: &ZapretConfig) -> bool {
    DaemonManager::new(config).is_running()
}
