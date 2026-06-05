//! Firewall rule management for Linux
//!
//! Supports both nftables and iptables backends.
//! Manages NFQUEUE redirection rules for nfqws2.

use std::process::Stdio;

use tokio::process::Command;
use tracing::{debug, error, info, warn};

use crate::{config::ZapretConfig, Result, ZapretError};

pub struct FirewallManager {
    fwtype: crate::config::FirewallType,
    qnum: u16,
    desync_mark: u32,
    desync_mark_postnat: u32,
    table_name: String,
}

impl FirewallManager {
    pub fn new(config: &ZapretConfig) -> Self {
        Self {
            fwtype: config.fwtype,
            qnum: config.qnum,
            desync_mark: config.desync_mark,
            desync_mark_postnat: config.desync_mark_postnat,
            table_name: "zapret2".to_string(),
        }
    }

    pub fn is_active(&self) -> bool {
        // Check if our table/chain exists
        match self.fwtype {
            crate::config::FirewallType::Nftables | crate::config::FirewallType::Auto => {
                Self::check_nft_table_exists(&self.table_name)
            }
            crate::config::FirewallType::Iptables => {
                Self::check_iptables_chain_exists()
            }
        }
    }

    pub async fn apply(&self) -> Result<()> {
        match self.fwtype {
            crate::config::FirewallType::Nftables => self.apply_nftables().await,
            crate::config::FirewallType::Iptables => self.apply_iptables().await,
            crate::config::FirewallType::Auto => {
                // Try nftables first, fall back to iptables
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

    pub async fn remove(&self) -> Result<()> {
        match self.fwtype {
            crate::config::FirewallType::Nftables => self.remove_nftables().await,
            crate::config::FirewallType::Iptables => self.remove_iptables().await,
            crate::config::FirewallType::Auto => {
                // Try both to be safe
                let _ = self.remove_nftables().await;
                let _ = self.remove_iptables().await;
                Ok(())
            }
        }
    }

    // --- nftables implementation ---

    async fn apply_nftables(&self) -> Result<()> {
        info!("applying nftables rules");

        // Remove existing rules first to avoid duplicates
        let _ = self.remove_nftables().await;

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
        let output = Command::new("nft")
            .arg("-f")
            .arg("-")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| ZapretError::FirewallError(format!("failed to spawn nft: {}", e)))?
            .wait_with_output()
            .await
            .map_err(|e| ZapretError::FirewallError(format!("nft failed: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ZapretError::FirewallError(format!(
                "nft failed: {}",
                stderr
            )));
        }

        Ok(())
    }

    fn check_nft_table_exists(table: &str) -> bool {
        std::process::Command::new("nft")
            .args(["list", "table", "inet", table])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn nftables_available() -> bool {
        which::which("nft").is_ok()
    }

    // --- iptables implementation ---

    async fn apply_iptables(&self) -> Result<()> {
        info!("applying iptables rules");
        // TODO: implement iptables rules
        warn!("iptables support not yet implemented");
        Ok(())
    }

    async fn remove_iptables(&self) -> Result<()> {
        // TODO: implement iptables removal
        Ok(())
    }

    fn check_iptables_chain_exists() -> bool {
        // TODO
        false
    }

    fn iptables_available() -> bool {
        which::which("iptables").is_ok()
    }
}
