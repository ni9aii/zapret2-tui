//! Privilege model: how the (normally unprivileged) TUI performs root-only
//! operations.
//!
//! Three executors implement the same [`PrivilegedExecutor`] contract:
//!
//! - [`DirectExecutor`] runs the operation in-process. Correct when the
//!   process is already root (or for host tests); used as the stateless
//!   counterpart to the helper.
//! - [`PkexecExecutor`] runs `pkexec zapret2-helper …`, triggering a polkit
//!   authentication prompt. Authentication cancellation is reported as
//!   [`ZapretError::AuthCancelled`], distinct from an operational failure.
//! - [`MockExecutor`] records calls without doing anything; for downstream
//!   tests.
//!
//! [`PrivilegeMode`] selects the strategy, resolving `Auto` to `Direct` when
//! already root and `Pkexec` otherwise.

use std::path::PathBuf;
use std::sync::Mutex;

use tokio::process::Command;

use crate::config::ZapretConfig;
use crate::{actions, Result, ZapretError};

/// User-requested privilege strategy (e.g. from `--privilege-mode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PrivilegeMode {
    /// Direct if already root, otherwise pkexec.
    #[default]
    Auto,
    /// Always use pkexec.
    Pkexec,
    /// Never use pkexec (root/debug/server mode).
    Direct,
}

/// A privilege strategy with `Auto` already resolved to a concrete choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ResolvedMode {
    #[default]
    Direct,
    Pkexec,
}

impl PrivilegeMode {
    /// Resolve to a concrete strategy from observed environment facts. Pure, so
    /// it is unit-testable without touching the real process state.
    ///
    /// - `Direct`/`Pkexec` pass through unchanged.
    /// - `Auto` → `Direct` when root; otherwise `Pkexec` when pkexec is
    ///   available; otherwise `Direct` as a best effort (the privileged action
    ///   will then surface a permission error honestly).
    pub fn resolve_with(self, is_root: bool, pkexec_available: bool) -> ResolvedMode {
        match self {
            PrivilegeMode::Direct => ResolvedMode::Direct,
            PrivilegeMode::Pkexec => ResolvedMode::Pkexec,
            PrivilegeMode::Auto => {
                if is_root {
                    ResolvedMode::Direct
                } else if pkexec_available {
                    ResolvedMode::Pkexec
                } else {
                    ResolvedMode::Direct
                }
            }
        }
    }

    /// Resolve using the real environment (effective uid and `pkexec` on PATH).
    pub fn resolve(self) -> ResolvedMode {
        self.resolve_with(is_root(), pkexec_available())
    }
}

/// Whether the process is running as root (euid 0).
#[cfg(unix)]
pub fn is_root() -> bool {
    unsafe { libc::geteuid() == 0 }
}

#[cfg(not(unix))]
pub fn is_root() -> bool {
    false
}

/// Whether `pkexec` is available on PATH.
pub fn pkexec_available() -> bool {
    which::which("pkexec").is_ok()
}

/// The privileged operations the TUI needs. Each implementation performs them
/// a different way; the contract is identical.
#[allow(async_fn_in_trait)]
pub trait PrivilegedExecutor {
    async fn apply_firewall(&self) -> Result<()>;
    async fn remove_firewall(&self) -> Result<()>;
    async fn start_daemon(&self, profile: Option<&str>) -> Result<()>;
    async fn stop_daemon(&self) -> Result<()>;
    async fn save_profile(&self, profile: &crate::profile::Profile) -> Result<()>;
    async fn remove_profile(&self, name: &str) -> Result<()>;
}

/// In-process executor: performs operations directly (no pkexec). Correct when
/// already root. Daemon start is detached, mirroring the helper.
pub struct DirectExecutor {
    config: ZapretConfig,
}

impl DirectExecutor {
    pub fn new(config: ZapretConfig) -> Self {
        Self { config }
    }

    fn config_for_profile(&self, profile: Option<&str>) -> Result<ZapretConfig> {
        let mut config = self.config.clone();
        if let Some(name) = profile {
            crate::profile::ProfileManager::validate_name(name)?;
            let mut manager =
                crate::profile::ProfileManager::new(config.zapret_base.join("profiles"));
            manager.load()?;
            let p = manager
                .get(name)
                .ok_or_else(|| ZapretError::ConfigError(format!("profile not found: {name}")))?;
            config.nfqws2_opt = p.nfqws_opts.clone();
            config.current_profile = Some(name.to_string());
        }
        Ok(config)
    }
}

impl PrivilegedExecutor for DirectExecutor {
    async fn apply_firewall(&self) -> Result<()> {
        actions::firewall_apply(&self.config).await
    }

    async fn remove_firewall(&self) -> Result<()> {
        actions::firewall_remove(&self.config).await
    }

    async fn start_daemon(&self, profile: Option<&str>) -> Result<()> {
        let config = self.config_for_profile(profile)?;
        actions::daemon_start(&config).await
    }

    async fn stop_daemon(&self) -> Result<()> {
        actions::daemon_stop(&self.config).await
    }

    async fn save_profile(&self, profile: &crate::profile::Profile) -> Result<()> {
        actions::profile_save(&self.config, profile)
    }

    async fn remove_profile(&self, name: &str) -> Result<()> {
        actions::profile_remove(&self.config, name)
    }
}

/// Default install path for the privileged helper.
pub const DEFAULT_HELPER_PATH: &str = "/usr/libexec/zapret2-helper";

/// Executor that delegates to `pkexec zapret2-helper …`, triggering polkit.
pub struct PkexecExecutor {
    helper_path: PathBuf,
    config_path: PathBuf,
}

impl PkexecExecutor {
    pub fn new(helper_path: PathBuf, config_path: PathBuf) -> Self {
        Self {
            helper_path,
            config_path,
        }
    }

    /// Build the full `pkexec` argument vector for a helper subcommand. Pure,
    /// so the exact command line is unit-testable. Never goes through a shell.
    fn build_args(&self, subcommand: &[&str]) -> Vec<String> {
        let mut args = vec![
            self.helper_path.to_string_lossy().into_owned(),
            "--config".to_string(),
            self.config_path.to_string_lossy().into_owned(),
        ];
        args.extend(subcommand.iter().map(|s| s.to_string()));
        args
    }

    /// Map a finished `pkexec` invocation to a result. Exit 126 (auth
    /// dismissed / not authorized) → [`ZapretError::AuthCancelled`]; 127
    /// (pkexec/helper not found) and other non-zero codes → process errors,
    /// keeping cancellation distinct from operational failure.
    fn classify(code: Option<i32>, stderr: &str) -> Result<()> {
        match code {
            Some(0) => Ok(()),
            Some(126) => Err(ZapretError::AuthCancelled),
            Some(127) => Err(ZapretError::ProcessError(
                "pkexec or zapret2-helper not found".to_string(),
            )),
            Some(n) => Err(ZapretError::ProcessError(format!(
                "helper failed (exit {n}): {}",
                stderr.trim()
            ))),
            None => Err(ZapretError::ProcessError(
                "helper terminated by signal".to_string(),
            )),
        }
    }

    async fn run(&self, subcommand: &[&str]) -> Result<()> {
        let args = self.build_args(subcommand);
        let output = Command::new("pkexec")
            .args(&args)
            .output()
            .await
            .map_err(|e| ZapretError::ProcessError(format!("failed to run pkexec: {e}")))?;
        let stderr = String::from_utf8_lossy(&output.stderr);
        Self::classify(output.status.code(), &stderr)
    }
}

impl PrivilegedExecutor for PkexecExecutor {
    async fn apply_firewall(&self) -> Result<()> {
        self.run(&["firewall", "apply"]).await
    }

    async fn remove_firewall(&self) -> Result<()> {
        self.run(&["firewall", "remove"]).await
    }

    async fn start_daemon(&self, profile: Option<&str>) -> Result<()> {
        match profile {
            Some(name) => self.run(&["daemon", "start", "--profile", name]).await,
            None => self.run(&["daemon", "start"]).await,
        }
    }

    async fn stop_daemon(&self) -> Result<()> {
        self.run(&["daemon", "stop"]).await
    }

    async fn save_profile(&self, profile: &crate::profile::Profile) -> Result<()> {
        let hostlists = profile.hostlists.join(",");
        self.run(&[
            "profile",
            "save",
            "--name",
            &profile.name,
            "--description",
            &profile.description,
            "--strategy",
            &profile.strategy,
            "--nfqws-opts",
            &profile.nfqws_opts,
            "--hostlists",
            &hostlists,
        ])
        .await
    }

    async fn remove_profile(&self, name: &str) -> Result<()> {
        self.run(&["profile", "remove", "--name", name]).await
    }
}

/// One recorded call against a [`MockExecutor`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MockCall {
    ApplyFirewall,
    RemoveFirewall,
    StartDaemon(Option<String>),
    StopDaemon,
    SaveProfile(crate::profile::Profile),
    RemoveProfile(String),
}

/// Test double recording calls; performs no real work.
#[derive(Default)]
pub struct MockExecutor {
    calls: Mutex<Vec<MockCall>>,
}

impl MockExecutor {
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot of the calls received so far, in order.
    pub fn calls(&self) -> Vec<MockCall> {
        self.calls.lock().unwrap().clone()
    }

    fn record(&self, call: MockCall) {
        self.calls.lock().unwrap().push(call);
    }
}

impl PrivilegedExecutor for MockExecutor {
    async fn apply_firewall(&self) -> Result<()> {
        self.record(MockCall::ApplyFirewall);
        Ok(())
    }

    async fn remove_firewall(&self) -> Result<()> {
        self.record(MockCall::RemoveFirewall);
        Ok(())
    }

    async fn start_daemon(&self, profile: Option<&str>) -> Result<()> {
        self.record(MockCall::StartDaemon(profile.map(str::to_string)));
        Ok(())
    }

    async fn stop_daemon(&self) -> Result<()> {
        self.record(MockCall::StopDaemon);
        Ok(())
    }

    async fn save_profile(&self, profile: &crate::profile::Profile) -> Result<()> {
        self.record(MockCall::SaveProfile(profile.clone()));
        Ok(())
    }

    async fn remove_profile(&self, name: &str) -> Result<()> {
        self.record(MockCall::RemoveProfile(name.to_string()));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_resolves_to_direct_when_root() {
        assert_eq!(
            PrivilegeMode::Auto.resolve_with(true, false),
            ResolvedMode::Direct
        );
        assert_eq!(
            PrivilegeMode::Auto.resolve_with(true, true),
            ResolvedMode::Direct
        );
    }

    #[test]
    fn auto_resolves_to_pkexec_when_unprivileged_and_available() {
        assert_eq!(
            PrivilegeMode::Auto.resolve_with(false, true),
            ResolvedMode::Pkexec
        );
    }

    #[test]
    fn auto_falls_back_to_direct_without_pkexec() {
        assert_eq!(
            PrivilegeMode::Auto.resolve_with(false, false),
            ResolvedMode::Direct
        );
    }

    #[test]
    fn explicit_modes_pass_through() {
        assert_eq!(
            PrivilegeMode::Direct.resolve_with(false, true),
            ResolvedMode::Direct
        );
        assert_eq!(
            PrivilegeMode::Pkexec.resolve_with(true, false),
            ResolvedMode::Pkexec
        );
    }

    #[test]
    fn pkexec_build_args_includes_config_and_subcommand() {
        let ex = PkexecExecutor::new(
            PathBuf::from("/usr/libexec/zapret2-helper"),
            PathBuf::from("/opt/zapret2/config"),
        );
        assert_eq!(
            ex.build_args(&["firewall", "apply"]),
            vec![
                "/usr/libexec/zapret2-helper",
                "--config",
                "/opt/zapret2/config",
                "firewall",
                "apply",
            ]
        );
        assert_eq!(
            ex.build_args(&["daemon", "start", "--profile", "yt"]),
            vec![
                "/usr/libexec/zapret2-helper",
                "--config",
                "/opt/zapret2/config",
                "daemon",
                "start",
                "--profile",
                "yt",
            ]
        );
    }

    #[test]
    fn classify_distinguishes_cancellation_from_failure() {
        assert!(matches!(PkexecExecutor::classify(Some(0), ""), Ok(())));
        assert!(matches!(
            PkexecExecutor::classify(Some(126), ""),
            Err(ZapretError::AuthCancelled)
        ));
        assert!(matches!(
            PkexecExecutor::classify(Some(127), ""),
            Err(ZapretError::ProcessError(_))
        ));
        assert!(matches!(
            PkexecExecutor::classify(Some(1), "nft failed"),
            Err(ZapretError::ProcessError(_))
        ));
        assert!(matches!(
            PkexecExecutor::classify(None, ""),
            Err(ZapretError::ProcessError(_))
        ));
    }

    #[tokio::test]
    async fn mock_records_calls_in_order() {
        let mock = MockExecutor::new();
        mock.apply_firewall().await.unwrap();
        mock.start_daemon(Some("yt")).await.unwrap();
        mock.stop_daemon().await.unwrap();
        mock.remove_firewall().await.unwrap();

        assert_eq!(
            mock.calls(),
            vec![
                MockCall::ApplyFirewall,
                MockCall::StartDaemon(Some("yt".to_string())),
                MockCall::StopDaemon,
                MockCall::RemoveFirewall,
            ]
        );
    }
}
