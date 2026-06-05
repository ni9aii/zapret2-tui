//! nfqws2 daemon process management

use std::path::PathBuf;
use std::process::Stdio;

use tokio::process::{Child, Command};
use tracing::{debug, error, info, warn};

use crate::{config::ZapretConfig, Result, ZapretError};

pub struct DaemonManager {
    bin_path: PathBuf,
    opts: String,
    qnum: u16,
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
        self.child
            .as_ref()
            .map(|c| {
                // try_wait needs &mut, but we only have &Child here
                // Use a non-blocking check via pid existence instead
                #[cfg(unix)]
                {
                    use std::os::unix::process::CommandExt;
                    // Check if process exists by sending signal 0
                    unsafe {
                        libc::kill(c.id().unwrap_or(0) as i32, 0) == 0
                    }
                }
                #[cfg(not(unix))]
                {
                    // On non-unix, we can't easily check without &mut
                    true // assume running
                }
            })
            .unwrap_or(false)
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

        // Parse NFQWS2_OPT and add arguments
        for arg in shell_words::split(&self.opts).map_err(|e| {
            ZapretError::ConfigError(format!("failed to parse NFQWS2_OPT: {}", e))
        })? {
            cmd.arg(arg);
        }

        cmd.stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        info!("starting nfqws2: {:?}", cmd);

        let child = cmd.spawn().map_err(|e| {
            ZapretError::ProcessError(format!("failed to spawn nfqws2: {}", e))
        })?;

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
                }
                Err(e) => {
                    warn!("failed to kill nfqws2: {}", e);
                }
            }
        }
        Ok(())
    }
}
