//! zapret2 configuration parser
//!
//! Parses the shell-based config format used by zapret2.
//! The config is a series of shell variable assignments.

use std::collections::HashMap;
use std::path::PathBuf;

use crate::{Result, DEFAULT_CONFIG_PATH};

#[derive(Debug, Clone, Default)]
pub struct ZapretConfig {
    pub zapret_base: PathBuf,
    pub config_path: PathBuf,
    pub nfqws2_enable: bool,
    pub nfqws2_opt: String,
    pub qnum: u16,
    pub fwtype: FirewallType,
    pub mode_filter: String,
    pub desync_mark: u32,
    pub desync_mark_postnat: u32,
    pub current_profile: Option<String>,
    pub raw: HashMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FirewallType {
    #[default]
    Auto,
    Iptables,
    Nftables,
}

impl ZapretConfig {
    pub fn load(path: Option<PathBuf>) -> Result<Self> {
        let path = path.unwrap_or_else(|| PathBuf::from(DEFAULT_CONFIG_PATH));

        if !path.exists() {
            return Ok(Self::default_with_base(path));
        }

        let content = std::fs::read_to_string(&path)?;
        Self::parse(&content, path)
    }

    pub(crate) fn default_with_base(config_path: PathBuf) -> Self {
        Self {
            zapret_base: PathBuf::from(crate::DEFAULT_ZAPRET_BASE),
            config_path,
            qnum: 200,
            fwtype: FirewallType::Auto,
            desync_mark: 0x40000000,
            desync_mark_postnat: 0x20000000,
            ..Default::default()
        }
    }

    fn parse(content: &str, config_path: PathBuf) -> Result<Self> {
        let mut config = Self::default_with_base(config_path);
        let mut raw = HashMap::new();

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if let Some((key, value)) = line.split_once('=') {
                let key = key.trim();
                let value = value.trim().trim_matches('"').trim_matches('\'');
                raw.insert(key.to_string(), value.to_string());

                match key {
                    "ZAPRET_BASE" => config.zapret_base = PathBuf::from(value),
                    "NFQWS2_ENABLE" => config.nfqws2_enable = value == "1",
                    "NFQWS2_OPT" => config.nfqws2_opt = value.to_string(),
                    "QNUM" => config.qnum = value.parse().unwrap_or(200),
                    "FWTYPE" => {
                        config.fwtype = match value {
                            "iptables" => FirewallType::Iptables,
                            "nftables" => FirewallType::Nftables,
                            _ => FirewallType::Auto,
                        }
                    }
                    "MODE_FILTER" => config.mode_filter = value.to_string(),
                    "DESYNC_MARK" => {
                        config.desync_mark = parse_hex_or_dec(value).unwrap_or(0x40000000)
                    }
                    "DESYNC_MARK_POSTNAT" => {
                        config.desync_mark_postnat = parse_hex_or_dec(value).unwrap_or(0x20000000)
                    }
                    _ => {}
                }
            }
        }

        config.raw = raw;
        Ok(config)
    }

    pub fn nfqws2_bin(&self) -> PathBuf {
        self.zapret_base.join("nfq2").join("nfqws2")
    }

    pub fn hostlist_dir(&self) -> PathBuf {
        self.zapret_base.join("files")
    }
}

fn parse_hex_or_dec(s: &str) -> Option<u32> {
    if s.starts_with("0x") || s.starts_with("0X") {
        u32::from_str_radix(&s[2..], 16).ok()
    } else {
        s.parse().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_basic() {
        let content = r#"
ZAPRET_BASE=/opt/zapret2
NFQWS2_ENABLE=1
NFQWS2_OPT="--qnum=200 --hostlist=/opt/zapret2/files/youtube.txt"
QNUM=200
FWTYPE=nftables
DESYNC_MARK=0x40000000
"#;

        let config = ZapretConfig::parse(content, PathBuf::from("/test")).unwrap();
        assert_eq!(config.zapret_base, PathBuf::from("/opt/zapret2"));
        assert!(config.nfqws2_enable);
        assert_eq!(config.qnum, 200);
        assert_eq!(config.fwtype, FirewallType::Nftables);
        assert_eq!(config.desync_mark, 0x40000000);
    }

    #[test]
    fn test_parse_missing_config_returns_default() {
        let result = ZapretConfig::load(Some(PathBuf::from("/nonexistent/config")));
        assert!(result.is_ok());
        let config = result.unwrap();
        assert_eq!(config.qnum, 200);
        assert_eq!(config.desync_mark, 0x40000000);
    }
}
