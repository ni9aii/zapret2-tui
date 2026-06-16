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
    pid_file: PathBuf,
    child: Option<Child>,
    log_tx: Option<mpsc::UnboundedSender<String>>,
}

impl DaemonManager {
    pub fn new(config: &ZapretConfig) -> Self {
        Self {
            bin_path: config.nfqws2_bin(),
            opts: config.nfqws2_opt.clone(),
            qnum: config.qnum,
            pid_file: PathBuf::from(NFQWS2_PID_FILE),
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

    /// Read and validate the pid stored in the pid file.
    ///
    /// Returns the pid only when the file exists and contains a single
    /// positive integer. Missing files, malformed contents, and non-positive
    /// values all yield `None`.
    fn read_pid_file(&self) -> Option<i32> {
        let contents = std::fs::read_to_string(&self.pid_file).ok()?;
        let pid = contents.trim().parse::<i32>().ok()?;
        (pid > 0).then_some(pid)
    }

    /// Check whether a process with the given pid is currently alive.
    ///
    /// Uses `kill(pid, 0)`, which sends no signal but performs the same
    /// existence/permission checks as a real signal:
    /// - `0`      → the process exists and we may signal it → alive.
    /// - `EPERM`  → the process exists but is owned by another user → alive.
    /// - `ESRCH`  → no such process → dead.
    #[cfg(unix)]
    fn pid_is_alive(pid: i32) -> bool {
        if pid <= 0 {
            return false;
        }
        // SAFETY: pid > 0 is guaranteed by the guard above; signal 0 never
        // delivers, so no process is affected regardless of the result.
        if unsafe { libc::kill(pid, 0) } == 0 {
            return true;
        }
        match std::io::Error::last_os_error().raw_os_error() {
            Some(libc::EPERM) => true,
            Some(libc::ESRCH) => false,
            other => {
                warn!("pid_is_alive: unexpected errno {:?} for pid {}", other, pid);
                false
            }
        }
    }

    #[cfg(not(unix))]
    fn pid_is_alive(_pid: i32) -> bool {
        false
    }

    pub fn is_running(&self) -> bool {
        self.read_pid_file().is_some_and(Self::pid_is_alive)
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

    /// Parse an nfqws2 option string and validate every flag against the
    /// whitelist. Returns the parsed arguments on success.
    pub fn validate_opts(opts: &str) -> Result<Vec<String>> {
        let args = shell_words::split(opts)
            .map_err(|e| ZapretError::ConfigError(format!("failed to parse NFQWS2_OPT: {e}")))?;
        for arg in &args {
            if !Self::validate_arg(arg) {
                return Err(ZapretError::ConfigError(format!(
                    "forbidden argument in NFQWS2_OPT: {arg}"
                )));
            }
        }
        Ok(args)
    }

    /// Update configuration-derived settings (binary path, options, queue
    /// number) from a new config, preserving the tracked child and the log
    /// channel so a running daemon stays controllable.
    pub fn apply_config(&mut self, config: &ZapretConfig) {
        self.bin_path = config.nfqws2_bin();
        self.opts = config.nfqws2_opt.clone();
        self.qnum = config.qnum;
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

        for arg in Self::validate_opts(&self.opts)? {
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

        // Write pid file; propagate failure so start() callers know when
        // status tracking will be broken (e.g. non-root can't write /var/run).
        if let Some(pid) = child.id() {
            std::fs::write(&self.pid_file, pid.to_string())
                .map_err(|e| ZapretError::ProcessError(format!("failed to write pid file: {e}")))?;
        }

        self.child = Some(child);
        info!("nfqws2 started");

        Ok(())
    }

    /// Start nfqws2 **detached**: spawn the process, write its pid file, then
    /// drop the child handle without tracking it. The process keeps running
    /// after this manager (and its owning binary) exits — required for the
    /// privileged helper, which spawns nfqws2 as root and then exits while the
    /// daemon stays up.
    ///
    /// Unlike [`start`](Self::start) there is no `kill_on_drop` and no log
    /// streaming: stdout/stderr are inherited, so output goes to the helper's
    /// stdio (journal/systemd when run under pkexec) rather than the TUI.
    pub async fn start_detached(&self) -> Result<()> {
        if self.is_running() {
            warn!("nfqws2 is already running");
            return Ok(());
        }

        if !self.bin_path.exists() {
            return Err(ZapretError::DaemonNotFound(self.bin_path.clone()));
        }

        let mut cmd = Command::new(&self.bin_path);
        cmd.arg(format!("--qnum={}", self.qnum));
        for arg in Self::validate_opts(&self.opts)? {
            cmd.arg(arg);
        }
        // Redirect all stdio to /dev/null: the daemon is detached and its
        // output would otherwise go to pkexec's socket, risking SIGPIPE.
        cmd.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        let child = cmd
            .spawn()
            .map_err(|e| ZapretError::ProcessError(format!("failed to spawn nfqws2: {e}")))?;

        if let Some(pid) = child.id() {
            std::fs::write(&self.pid_file, pid.to_string())
                .map_err(|e| ZapretError::ProcessError(format!("failed to write pid file: {e}")))?;
            info!("nfqws2 started detached (pid {pid})");
        } else {
            warn!("nfqws2 spawned but pid was unavailable; pid file not written");
        }

        // Drop the handle without waiting or killing: with kill_on_drop unset
        // (tokio's default) the process keeps running, is reparented to init
        // when we exit, and is reaped there.
        drop(child);
        Ok(())
    }

    pub async fn stop(&mut self) -> Result<()> {
        // Number of 100ms polling intervals to wait for graceful shutdown
        // before escalating to SIGKILL (~3 seconds total).
        const GRACE_POLLS: u32 = 30;
        const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(100);

        #[cfg(unix)]
        {
            use nix::sys::signal::{kill, Signal};
            use nix::unistd::Pid;

            if let Some(mut child) = self.child.take() {
                // We own this process: SIGTERM, poll via try_wait (which also
                // reaps on exit so we never read a zombie), then SIGKILL.
                if let Some(id) = child.id() {
                    info!("stopping nfqws2 (tracked pid {id})");
                    let _ = kill(Pid::from_raw(id as i32), Signal::SIGTERM);
                }

                let mut exited = false;
                for _ in 0..GRACE_POLLS {
                    match child.try_wait() {
                        Ok(Some(_)) => {
                            exited = true;
                            break;
                        }
                        Ok(None) => tokio::time::sleep(POLL_INTERVAL).await,
                        Err(e) => {
                            warn!("failed to poll nfqws2 status: {e}");
                            break;
                        }
                    }
                }

                if !exited {
                    warn!("nfqws2 did not exit after SIGTERM; sending SIGKILL");
                    let _ = child.kill().await;
                }
                let _ = child.wait().await; // reap
                info!("nfqws2 stopped");
            } else if let Some(pid) = self.read_pid_file() {
                // A daemon started by a previous process, discovered via the
                // pid file. Signal it directly so it can actually be stopped.
                if Self::pid_is_alive(pid) {
                    info!("stopping nfqws2 (pidfile pid {pid})");
                    let npid = Pid::from_raw(pid);
                    let _ = kill(npid, Signal::SIGTERM);

                    let mut exited = false;
                    for _ in 0..GRACE_POLLS {
                        if !Self::pid_is_alive(pid) {
                            exited = true;
                            break;
                        }
                        tokio::time::sleep(POLL_INTERVAL).await;
                    }

                    if !exited {
                        warn!("nfqws2 (pid {pid}) did not exit after SIGTERM; sending SIGKILL");
                        let _ = kill(npid, Signal::SIGKILL);
                    }
                    info!("nfqws2 stopped");
                }
            }
        }

        #[cfg(not(unix))]
        {
            if let Some(mut child) = self.child.take() {
                let _ = child.kill().await;
                let _ = child.wait().await;
            }
        }

        // Remove the pid file, but never discard one that still points at a
        // live, unrelated process — that would hide a running daemon.
        match self.read_pid_file() {
            Some(pid) if Self::pid_is_alive(pid) => {
                warn!("pid file points to live process {pid}; leaving it in place");
            }
            _ => {
                let _ = std::fs::remove_file(&self.pid_file);
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ZapretConfig;
    use std::path::PathBuf;

    fn manager_with_pidfile(pid_file: PathBuf) -> DaemonManager {
        let config = ZapretConfig::default_with_base(PathBuf::from("/tmp/test"));
        let mut manager = DaemonManager::new(&config);
        manager.pid_file = pid_file;
        manager
    }

    #[test]
    fn read_pid_file_parses_valid_positive_pid() {
        let tempdir = tempfile::tempdir().unwrap();
        let manager = manager_with_pidfile(tempdir.path().join("nfqws2.pid"));
        std::fs::write(&manager.pid_file, "  4242\n").unwrap();

        assert_eq!(manager.read_pid_file(), Some(4242));
    }

    #[test]
    fn read_pid_file_rejects_missing_and_malformed_contents() {
        let tempdir = tempfile::tempdir().unwrap();
        let manager = manager_with_pidfile(tempdir.path().join("nfqws2.pid"));

        // Missing file.
        assert_eq!(manager.read_pid_file(), None);

        for bad in ["", "   ", "abc", "0", "-1", "12 34"] {
            std::fs::write(&manager.pid_file, bad).unwrap();
            assert_eq!(manager.read_pid_file(), None, "should reject {bad:?}");
        }
    }

    #[test]
    fn pid_is_alive_true_for_current_process() {
        assert!(DaemonManager::pid_is_alive(std::process::id() as i32));
    }

    #[test]
    fn pid_is_alive_false_for_non_positive_pid() {
        assert!(!DaemonManager::pid_is_alive(0));
        assert!(!DaemonManager::pid_is_alive(-1));
    }

    #[tokio::test]
    async fn pid_is_alive_false_after_child_is_reaped() {
        let mut child = tokio::process::Command::new("true").spawn().unwrap();
        let pid = child.id().unwrap() as i32;
        child.wait().await.unwrap(); // reap so the pid is fully released

        assert!(!DaemonManager::pid_is_alive(pid));
    }

    #[test]
    fn test_is_running_returns_false_when_no_pid_file() {
        let tempdir = tempfile::tempdir().unwrap();
        let manager = manager_with_pidfile(tempdir.path().join("nfqws2.pid"));

        assert!(!manager.is_running());
    }

    #[test]
    fn is_running_true_when_pidfile_points_to_live_process() {
        let tempdir = tempfile::tempdir().unwrap();
        let manager = manager_with_pidfile(tempdir.path().join("nfqws2.pid"));
        std::fs::write(&manager.pid_file, std::process::id().to_string()).unwrap();

        assert!(manager.is_running());
    }

    #[tokio::test]
    async fn stop_removes_stale_pid_file_when_child_is_not_tracked() {
        let tempdir = tempfile::tempdir().unwrap();
        let mut manager = manager_with_pidfile(tempdir.path().join("nfqws2.pid"));
        std::fs::write(&manager.pid_file, "999999").unwrap();

        manager.stop().await.unwrap();

        assert!(
            !manager.pid_file.exists(),
            "stop must remove stale pid files even when this process did not spawn the child"
        );
    }

    #[tokio::test]
    async fn stop_terminates_tracked_child_and_removes_pidfile() {
        let tempdir = tempfile::tempdir().unwrap();
        let mut manager = manager_with_pidfile(tempdir.path().join("nfqws2.pid"));

        let child = tokio::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .unwrap();
        let pid = child.id().unwrap();
        std::fs::write(&manager.pid_file, pid.to_string()).unwrap();
        manager.child = Some(child);

        assert!(manager.is_running(), "child should be detected as running");

        manager.stop().await.unwrap();

        assert!(
            !DaemonManager::pid_is_alive(pid as i32),
            "stop must terminate the tracked child"
        );
        assert!(
            !manager.pid_file.exists(),
            "stop must remove the pid file once the daemon is gone"
        );
    }
}
