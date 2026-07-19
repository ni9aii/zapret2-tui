//! Firewall rule management for Linux
//!
//! Supports both nftables and iptables backends.
//! Manages NFQUEUE redirection rules for nfqws2.

use std::process::Stdio;

use tokio::process::Command;
use tracing::{debug, info};

use crate::{config::ZapretConfig, Result, ZapretError};

pub struct FirewallManager {
    fwtype: crate::config::FirewallType,
    qnum: u16,
    desync_mark: u32,
    table_name: String,
}

impl FirewallManager {
    pub fn new(config: &ZapretConfig) -> Self {
        Self {
            fwtype: config.fwtype,
            qnum: config.qnum,
            desync_mark: config.desync_mark,
            table_name: "zapret2".to_string(),
        }
    }

    /// Check if firewall rules are currently applied.
    pub async fn is_active(&self) -> bool {
        match self.fwtype {
            crate::config::FirewallType::Nftables | crate::config::FirewallType::Auto => {
                Self::check_nft_table_exists(&self.table_name).await
            }
            crate::config::FirewallType::Iptables => Self::check_iptables_chain_exists(),
        }
    }

    /// Apply firewall redirection rules.
    pub async fn apply(&self) -> Result<()> {
        match self.fwtype {
            crate::config::FirewallType::Nftables => self.apply_nftables().await,
            crate::config::FirewallType::Iptables => self.apply_iptables().await,
            crate::config::FirewallType::Auto => {
                if Self::nftables_available() {
                    self.apply_nftables().await
                } else if Self::iptables_available() {
                    self.apply_iptables().await
                } else {
                    Err(ZapretError::FirewallError(
                        "neither nftables nor iptables available".to_string(),
                    ))
                }
            }
        }
    }

    /// Remove applied firewall rules.
    pub async fn remove(&self) -> Result<()> {
        match self.fwtype {
            crate::config::FirewallType::Nftables => self.remove_nftables().await,
            crate::config::FirewallType::Iptables => self.remove_iptables().await,
            crate::config::FirewallType::Auto => {
                let _ = self.remove_nftables().await;
                let _ = self.remove_iptables().await;
                Ok(())
            }
        }
    }

    // --- nftables implementation ---

    async fn apply_nftables(&self) -> Result<()> {
        info!("applying nftables rules");

        let script = format!(
            r#"
table inet {table}
delete table inet {table}
table inet {table} {{
    chain post {{
        type filter hook postrouting priority 101; policy accept;
        meta mark and {mark:x} == 0 tcp dport {{ 80, 443 }} ct original packets 1-12 queue num {qnum} bypass
        meta mark and {mark:x} == 0 udp dport {{ 443 }} ct original packets 1-12 queue num {qnum} bypass
    }}
    chain pre {{
        type filter hook prerouting priority -101; policy accept;
        meta mark and {mark:x} == 0 tcp sport {{ 80, 443 }} ct reply packets 1-12 queue num {qnum} bypass
        meta mark and {mark:x} == 0 udp sport {{ 443 }} ct reply packets 1-12 queue num {qnum} bypass
    }}
    chain predefrag {{
        type filter hook output priority -401; policy accept;
        mark & {mark:x} != 0x00000000 notrack
    }}
}}
"#,
            table = self.table_name,
            mark = self.desync_mark,
            qnum = self.qnum,
        );

        self.run_nft(&script).await?;
        info!("nftables rules applied");
        Ok(())
    }

    async fn remove_nftables(&self) -> Result<()> {
        let script = format!("delete table inet {}", self.table_name);
        match self.run_nft(&script).await {
            Ok(_) => info!("nftables rules removed"),
            Err(_) => debug!("nftables table did not exist"),
        }
        Ok(())
    }

    async fn run_nft(&self, script: &str) -> Result<()> {
        let mut child = Command::new("nft")
            .arg("-f")
            .arg("-")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| ZapretError::FirewallError(format!("failed to spawn nft: {}", e)))?;

        if let Some(mut stdin) = child.stdin.take() {
            use tokio::io::AsyncWriteExt;
            stdin.write_all(script.as_bytes()).await.map_err(|e| {
                ZapretError::FirewallError(format!("failed to write nft stdin: {}", e))
            })?;
        }

        let output = child
            .wait_with_output()
            .await
            .map_err(|e| ZapretError::FirewallError(format!("nft failed: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ZapretError::FirewallError(format!(
                "nft failed (status={}): {}",
                output.status.code().unwrap_or(-1),
                stderr
            )));
        }

        debug!("nftables script executed successfully");
        Ok(())
    }

    async fn check_nft_table_exists(table: &str) -> bool {
        Command::new("nft")
            .args(["list", "table", "inet", table])
            .output()
            .await
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn nftables_available() -> bool {
        which::which("nft").is_ok()
    }

    // --- iptables implementation ---

    async fn apply_iptables(&self) -> Result<()> {
        Err(Self::iptables_unsupported_error())
    }

    async fn remove_iptables(&self) -> Result<()> {
        Err(Self::iptables_unsupported_error())
    }

    fn iptables_unsupported_error() -> ZapretError {
        ZapretError::FirewallError(
            "iptables backend is not implemented yet; set FWTYPE=nftables or FWTYPE=auto with nft available"
                .to_string(),
        )
    }

    fn check_iptables_chain_exists() -> bool {
        false
    }

    fn iptables_available() -> bool {
        which::which("iptables").is_ok()
    }
}

#[cfg(test)]
mod firewall_tests {
    use super::*;

    fn test_config() -> ZapretConfig {
        ZapretConfig::default_with_base(std::path::PathBuf::from("/tmp/test"))
    }

    #[test]
    fn test_firewall_manager_new() {
        let config = test_config();
        let fw = FirewallManager::new(&config);
        assert_eq!(fw.qnum, 200);
        assert_eq!(fw.table_name, "zapret2");
    }

    #[tokio::test]
    async fn explicit_iptables_apply_returns_unsupported_error() {
        let mut config = test_config();
        config.fwtype = crate::config::FirewallType::Iptables;
        let fw = FirewallManager::new(&config);

        let err = fw
            .apply()
            .await
            .expect_err("iptables backend must not report success");

        assert!(
            matches!(err, ZapretError::FirewallError(ref message) if message.contains("iptables backend is not implemented")),
            "unexpected error: {err}"
        );
    }

    #[tokio::test]
    async fn explicit_iptables_remove_returns_unsupported_error() {
        let mut config = test_config();
        config.fwtype = crate::config::FirewallType::Iptables;
        let fw = FirewallManager::new(&config);

        let err = fw
            .remove()
            .await
            .expect_err("iptables backend must not report successful cleanup");

        assert!(
            matches!(err, ZapretError::FirewallError(ref message) if message.contains("iptables backend is not implemented")),
            "unexpected error: {err}"
        );
    }
}
