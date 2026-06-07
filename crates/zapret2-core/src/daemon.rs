//! nfqws2 daemon process management
//!
//! Manages NFQUEUE redirection rules for nfqws2.

use std::path::PathBuf;
use std::process::Stdio;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::{config::ZapretConfig, Result, ZapretError};

/// Path to PID file for tracking nfqws2 process.
/// Uses /var/run for proper permissions on Linux systems.
pub const NFQWS2_PID_FILE: &str = "/var/run/nfqws2.pid";

/// Whitelist of allowed nfqws2 arguments for security hardening
const ALLOWED_NFQWS_OPTS: &[&str] = &[
    "--qnum",
    "--desync",
    "--hostlist",
    "--split",
    "--wss",
    "--dpi-desync",
    "--dpi-desync-fw-external",
    "--dpi-desync-ttl",
    "--encrypt",
    "--md5",
    "--server",
    "--port",
    "--proxy",
    "--proxy-host",
    "--proxy-port",
];

pub struct DaemonManager {
    bin_path: PathBuf,
    opts: String,
    qnum: u16,
    #[allow(dead_code)]
    child: Option<Child>,
    log_tx: Option<mpsc::UnboundedSender<String>>,
}

impl DaemonManager {
    pub fn new(config: &ZapretConfig) -> Self {
        Self {
            bin_path: config.nfqws2_bin(),
            opts: config.nfqws2_opt.clone(),
            qnum: config.qnum,
            child: None,
            log_tx: None,
        }
    }

    /// Set the channel used to stream nfqws2 stdout/stderr into the TUI.
    pub fn set_log_channel(&mut self, tx: mpsc::UnboundedSender<String>) {
        self.log_tx = Some(tx);
    }

    pub fn pid(&self) -> Option<u32> {
        self.child.as_ref().and_then(|child| child.id())
    }

    pub fn is_running(&self) -> bool {
        // Check if process exists by pid file
        #[cfg(unix)]
        {
            use std::fs;
            if let Ok(pid_str) = fs::read_to_string(NFQWS2_PID_FILE) {
                if let Ok(pid) = pid_str.trim().parse::<i32>() {
                    // Validate PID is reasonable (must be positive)
                    if pid <= 0 {
                        return false;
                    }
                    // kill(pid, 0) returns 0 on success, -1 on error with errno set
                    let result = unsafe { libc::kill(pid, 0) };
                    if result == 0 {
                        return true;
                    }
                    // kill returned -1 with errno - check for ESRCH (no such process)
                    // errno=3 (ESRCH) means process doesn't exist, which is OK
                    // Other errors (EACCES, etc.) should not be ignored
                    #[cfg(debug_assertions)]
                    let errno = unsafe { *libc::__errno_location() };
                    // ESRCH = 3 (no such process) - process not running is expected
                    // Any other error while kill returned -1 should be logged
                    #[cfg(debug_assertions)]
                    if errno != 3 {
                        warn!("is_running: unexpected errno from kill: {}", errno);
                    }
                }
            }
            false
        }
        #[cfg(not(unix))]
        {
            false
        }
    }

    /// Validates that an argument is in the whitelist (prevents command injection)
    fn validate_arg(arg: &str) -> bool {
        if arg.starts_with('-') {
            // It's a flag - check if it matches whitelist
            let flag = arg.split('=').next().unwrap_or(arg);
            ALLOWED_NFQWS_OPTS.contains(&flag)
        } else {
            // Non-flag arguments (values) are allowed
            true
        }
    }

    pub async fn start(&mut self) -> Result<()> {
        if self.is_running() {
            warn!("nfqws2 is already running");
            return Ok(());
        }

        if !self.bin_path.exists() {
            return Err(ZapretError::DaemonNotFound(self.bin_path.clone()));
        }

        let mut cmd = Command::new(&self.bin_path);
        cmd.arg(format!("--qnum={}", self.qnum));

        for arg in shell_words::split(&self.opts)
            .map_err(|e| ZapretError::ConfigError(format!("failed to parse NFQWS2_OPT: {}", e)))?
        {
            if !Self::validate_arg(&arg) {
                return Err(ZapretError::ConfigError(format!(
                    "forbidden argument in NFQWS2_OPT: {}",
                    arg
                )));
            }
            cmd.arg(arg);
        }

        cmd.stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        info!("starting nfqws2: {:?}", cmd);

        let mut child = cmd
            .spawn()
            .map_err(|e| ZapretError::ProcessError(format!("failed to spawn nfqws2: {}", e)))?;

        // Spawn stdout/stderr log capture task
        if let Some(stdout) = child.stdout.take() {
            let mut reader = BufReader::new(stdout).lines();
            let log_tx = self.log_tx.clone();
            tokio::spawn(async move {
                while let Ok(Some(line)) = reader.next_line().await {
                    if let Some(tx) = &log_tx {
                        let _ = tx.send(format!("[nfqws] {line}"));
                    }
                    tracing::info!("nfqws2: {}", line);
                }
            });
        }

        if let Some(stderr) = child.stderr.take() {
            let mut reader = BufReader::new(stderr).lines();
            let log_tx = self.log_tx.clone();
            tokio::spawn(async move {
                while let Ok(Some(line)) = reader.next_line().await {
                    if let Some(tx) = &log_tx {
                        let _ = tx.send(format!("[nfqws/err] {line}"));
                    }
                    tracing::warn!("nfqws2 stderr: {}", line);
                }
            });
        }

        // Write pid file for later tracking
        if let Some(pid) = child.id() {
            let _ = std::fs::write(NFQWS2_PID_FILE, pid.to_string());
        }

        self.child = Some(child);
        info!("nfqws2 started");

        Ok(())
    }

    pub async fn stop(&mut self) -> Result<()> {
        if let Some(mut child) = self.child.take() {
            info!("stopping nfqws2");
            // Try graceful shutdown first with SIGTERM
            #[cfg(unix)]
            {
                use nix::sys::signal::kill;
                use nix::sys::signal::Signal;
                use nix::unistd::Pid;

                let pid = Pid::from_raw(child.id().unwrap_or(0) as i32);
                let _ = kill(pid, Signal::SIGTERM);

                // Wait briefly for graceful shutdown
                let wait_timeout = tokio::time::Duration::from_secs(3);
                tokio::time::sleep(wait_timeout).await;
            }

            // Force kill if still running
            match child.kill().await {
                Ok(_) => {
                    let _ = child.wait().await;
                    info!("nfqws2 stopped");
                }
                Err(e) => {
                    warn!("failed to stop nfqws2: {e}");
                }
            }
            let _ = std::fs::remove_file(NFQWS2_PID_FILE);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ZapretConfig;
    use std::path::PathBuf;

    #[test]
    fn test_is_running_returns_false_when_no_pid_file() {
        let config = ZapretConfig::default_with_base(PathBuf::from("/tmp/test"));
        let manager = DaemonManager::new(&config);
        // Clean up any leftover pid file
        let _ = std::fs::remove_file(NFQWS2_PID_FILE);
        assert!(!manager.is_running());
    }
}
