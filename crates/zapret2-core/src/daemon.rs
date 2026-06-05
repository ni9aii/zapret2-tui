//! nfqws2 daemon process management

use std::path::PathBuf;
use std::process::Stdio;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tracing::{info, warn};

use crate::{config::ZapretConfig, Result, ZapretError};

pub struct DaemonManager {
    bin_path: PathBuf,
    opts: String,
    qnum: u16,
    #[allow(dead_code)]
    child: Option<Child>,
}

impl DaemonManager {
    pub fn new(config: &ZapretConfig) -> Self {
        Self {
            bin_path: config.nfqws2_bin(),
            opts: config.nfqws2_opt.clone(),
            qnum: config.qnum,
            child: None,
        }
    }

    pub fn is_running(&self) -> bool {
        // Check if process exists by pid file
        #[cfg(unix)]
        {
            use std::fs;
            if let Ok(pid_str) = fs::read_to_string("/tmp/nfqws2.pid") {
                if let Ok(pid) = pid_str.trim().parse::<i32>() {
                    let result = unsafe { libc::kill(pid, 0) };
                    return result == 0;
                }
            }
            false
        }
        #[cfg(not(unix))]
        {
            false
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
            tokio::spawn(async move {
                while let Ok(Some(line)) = reader.next_line().await {
                    tracing::info!("nfqws2: {}", line);
                }
            });
        }

        if let Some(stderr) = child.stderr.take() {
            let mut reader = BufReader::new(stderr).lines();
            tokio::spawn(async move {
                while let Ok(Some(line)) = reader.next_line().await {
                    tracing::warn!("nfqws2 stderr: {}", line);
                }
            });
        }

        // Write pid file for later tracking
        if let Some(pid) = child.id() {
            let _ = std::fs::write("/tmp/nfqws2.pid", pid.to_string());
        }

        self.child = Some(child);
        info!("nfqws2 started");

        Ok(())
    }

    pub async fn stop(&mut self) -> Result<()> {
        if let Some(mut child) = self.child.take() {
            info!("stopping nfqws2");
            match child.kill().await {
                Ok(_) => {
                    let _ = child.wait().await;
                    info!("nfqws2 stopped");
                    let _ = std::fs::remove_file("/tmp/nfqws2.pid");
                }
                Err(e) => {
                    warn!("failed to kill nfqws2: {}", e);
                }
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

    #[test]
    fn test_is_running_returns_false_when_no_pid_file() {
        let config = ZapretConfig::default_with_base(PathBuf::from("/tmp/test"));
        let manager = DaemonManager::new(&config);
        // Clean up any leftover pid file
        let _ = std::fs::remove_file("/tmp/nfqws2.pid");
        assert!(!manager.is_running());
    }
}
