//! Profile and strategy management for zapret2
//!
//! Profiles define which Lua strategies and hostlists to use.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    pub description: String,
    pub strategy: String,
    pub hostlists: Vec<String>,
    pub nfqws_opts: String,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            name: "default".to_string(),
            description: "Default YouTube + Discord bypass".to_string(),
            strategy: "youtube-discord".to_string(),
            hostlists: vec!["youtube.txt".to_string(), "discord.txt".to_string()],
            nfqws_opts: "--qnum=200".to_string(),
        }
    }
}

pub struct ProfileManager {
    profiles_dir: PathBuf,
    profiles: HashMap<String, Profile>,
}

impl ProfileManager {
    pub fn new(profiles_dir: PathBuf) -> Self {
        Self {
            profiles_dir,
            profiles: HashMap::new(),
        }
    }

    /// Validates profile name to prevent path traversal attacks.
    fn validate_profile_name(name: &str) -> bool {
        // Disallow path separators and parent directory references
        !name.is_empty() 
            && !name.contains('/') 
            && !name.contains('\\') 
            && !name.contains("..")
            && name.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.')
    }

    pub fn load(&mut self) -> Result<()> {
        if !self.profiles_dir.exists() {
            std::fs::create_dir_all(&self.profiles_dir)?;
            // Create default profile
            let default = Profile::default();
            self.save_profile(&default)?;
            self.profiles.insert(default.name.clone(), default);
            return Ok(());
        }

        for entry in std::fs::read_dir(&self.profiles_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("toml") {
                let content = std::fs::read_to_string(&path)?;
                let profile: Profile = toml::from_str(&content).map_err(|e| {
                    crate::ZapretError::ConfigError(format!("invalid profile: {}", e))
                })?;
                self.profiles.insert(profile.name.clone(), profile);
            }
        }

        Ok(())
    }

    pub fn save_profile(&self, profile: &Profile) -> Result<()> {
        if !Self::validate_profile_name(&profile.name) {
            return Err(crate::ZapretError::ConfigError(
                "invalid profile name: path traversal not allowed".to_string(),
            ));
        }
        let path = self.profiles_dir.join(format!("{}.toml", profile.name));
        let content = toml::to_string_pretty(profile)
            .map_err(|e| crate::ZapretError::ConfigError(format!("serialize error: {}", e)))?;
        std::fs::write(path, content)?;
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&Profile> {
        self.profiles.get(name)
    }

    pub fn list(&self) -> Vec<&Profile> {
        self.profiles.values().collect()
    }

    pub fn remove(&mut self, name: &str) -> Result<()> {
        if !Self::validate_profile_name(name) {
            return Err(crate::ZapretError::ConfigError(
                "invalid profile name: path traversal not allowed".to_string(),
            ));
        }
        self.profiles.remove(name);
        let path = self.profiles_dir.join(format!("{}.toml", name));
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod profile_tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_profile_default() {
        let p = Profile::default();
        assert_eq!(p.name, "default");
        assert_eq!(p.nfqws_opts, "--qnum=200");
        assert!(!p.hostlists.is_empty());
    }

    #[test]
    fn test_profile_manager_new() {
        let tmp = TempDir::new().unwrap();
        let pm = ProfileManager::new(tmp.path().to_path_buf());
        assert!(pm.profiles.is_empty());
    }

    #[test]
    fn test_profile_save_and_load() {
        let tmp = TempDir::new().unwrap();
        let mut pm = ProfileManager::new(tmp.path().to_path_buf());

        let profile = Profile {
            name: "test-profile".to_string(),
            description: "Test".to_string(),
            strategy: "test-strategy".to_string(),
            hostlists: vec!["test.txt".to_string()],
            nfqws_opts: "--qnum=300".to_string(),
        };

        pm.save_profile(&profile).unwrap();
        pm.load().unwrap();

        let loaded = pm.get("test-profile");
        assert!(loaded.is_some());
        assert!(loaded.unwrap().nfqws_opts.contains("--qnum=300"));
    }
}
