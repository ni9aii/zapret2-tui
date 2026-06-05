//! Profile and strategy management for zapret2
//!
//! Profiles define which Lua strategies and hostlists to use.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

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
            hostlists: vec![
                "youtube.txt".to_string(),
                "discord.txt".to_string(),
            ],
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
                let profile: Profile = toml::from_str(&content)
                    .map_err(|e| crate::ZapretError::ConfigError(format!("invalid profile: {}", e)))?;
                self.profiles.insert(profile.name.clone(), profile);
            }
        }

        Ok(())
    }

    pub fn save_profile(&self, profile: &Profile) -> Result<()> {
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
        self.profiles.remove(name);
        let path = self.profiles_dir.join(format!("{}.toml", name));
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        Ok(())
    }
}
