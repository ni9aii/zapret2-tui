//! Profile and strategy management for zapret2
//!
//! Profiles define which Lua strategies and hostlists to use.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::{Result, ZapretError};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

    /// Validate a profile name.
    ///
    /// Rejects empty names and anything that could escape the profiles
    /// directory (path separators or `..`). Returns a descriptive error
    /// instead of a bare boolean so callers can surface the reason.
    pub fn validate_name(name: &str) -> Result<()> {
        if name.is_empty() {
            return Err(ZapretError::ConfigError(
                "profile name must not be empty".to_string(),
            ));
        }
        if name.contains('/') || name.contains('\\') || name.contains("..") {
            return Err(ZapretError::ConfigError(format!(
                "invalid profile name {name:?}: path separators and '..' are not allowed"
            )));
        }
        if !name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.')
        {
            return Err(ZapretError::ConfigError(format!(
                "invalid profile name {name:?}: only alphanumerics, '-', '_' and '.' are allowed"
            )));
        }
        Ok(())
    }

    /// Load all profiles from disk into memory.
    ///
    /// Read-only: a missing profiles directory is treated as "no profiles
    /// yet" rather than an error, and the directory is **not** created here —
    /// that could require root for paths like `/opt/zapret2/profiles`. Use
    /// [`ensure_default_profiles`](Self::ensure_default_profiles) when default
    /// creation is wanted. Non-`.toml` files are ignored.
    pub fn load(&mut self) -> Result<()> {
        if !self.profiles_dir.exists() {
            self.profiles.clear();
            return Ok(());
        }

        let mut loaded = HashMap::new();
        for entry in std::fs::read_dir(&self.profiles_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("toml") {
                continue;
            }
            let content = std::fs::read_to_string(&path)?;
            let profile: Profile = toml::from_str(&content).map_err(|e| {
                ZapretError::ConfigError(format!("invalid profile {}: {}", path.display(), e))
            })?;
            loaded.insert(profile.name.clone(), profile);
        }

        // Replace wholesale so memory exactly reflects on-disk state.
        self.profiles = loaded;
        Ok(())
    }

    /// Create the default profile (on disk and in memory) when no profiles
    /// exist. Creating the directory and writing the file may require write
    /// access to the profiles directory.
    pub fn ensure_default_profiles(&mut self) -> Result<()> {
        if !self.profiles.is_empty() {
            return Ok(());
        }
        if !self.profiles_dir.exists() {
            std::fs::create_dir_all(&self.profiles_dir)?;
        }
        self.save_profile(&Profile::default())
    }

    /// Persist a profile to disk and update the in-memory map so the two
    /// cannot silently diverge.
    pub fn save_profile(&mut self, profile: &Profile) -> Result<()> {
        Self::validate_name(&profile.name)?;
        let path = self.profiles_dir.join(format!("{}.toml", profile.name));
        let content = toml::to_string_pretty(profile)
            .map_err(|e| ZapretError::ConfigError(format!("serialize error: {}", e)))?;
        std::fs::write(path, content)?;
        self.profiles.insert(profile.name.clone(), profile.clone());
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&Profile> {
        self.profiles.get(name)
    }

    pub fn list(&self) -> Vec<&Profile> {
        self.profiles.values().collect()
    }

    /// Remove a profile from both disk and the in-memory map.
    pub fn remove(&mut self, name: &str) -> Result<()> {
        Self::validate_name(name)?;
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

    fn sample_profile(name: &str) -> Profile {
        Profile {
            name: name.to_string(),
            description: "Test".to_string(),
            strategy: "test-strategy".to_string(),
            hostlists: vec!["test.txt".to_string()],
            nfqws_opts: "--qnum=300".to_string(),
        }
    }

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

        pm.save_profile(&sample_profile("test-profile")).unwrap();
        pm.load().unwrap();

        let loaded = pm.get("test-profile");
        assert!(loaded.is_some());
        assert!(loaded.unwrap().nfqws_opts.contains("--qnum=300"));
    }

    #[test]
    fn validate_name_rejects_unsafe_names() {
        for bad in ["", "../x", "a/b", "a\\b", "..", "foo bar", "weird!"] {
            assert!(
                ProfileManager::validate_name(bad).is_err(),
                "expected {bad:?} to be rejected"
            );
        }
    }

    #[test]
    fn validate_name_accepts_reasonable_names() {
        for good in [
            "default",
            "my-profile",
            "my_profile",
            "v1.2",
            "yt-discord_2",
        ] {
            assert!(
                ProfileManager::validate_name(good).is_ok(),
                "expected {good:?} to be accepted"
            );
        }
    }

    #[test]
    fn save_profile_updates_memory_without_reload() {
        let tmp = TempDir::new().unwrap();
        let mut pm = ProfileManager::new(tmp.path().to_path_buf());

        pm.save_profile(&sample_profile("in-mem")).unwrap();

        // No load() call: memory must already reflect the saved profile.
        assert!(pm.get("in-mem").is_some());
    }

    #[test]
    fn remove_deletes_file_and_memory_entry() {
        let tmp = TempDir::new().unwrap();
        let mut pm = ProfileManager::new(tmp.path().to_path_buf());
        pm.save_profile(&sample_profile("to-remove")).unwrap();
        let path = tmp.path().join("to-remove.toml");
        assert!(path.exists());

        pm.remove("to-remove").unwrap();

        assert!(pm.get("to-remove").is_none());
        assert!(!path.exists());
    }

    #[test]
    fn load_ignores_non_toml_files() {
        let tmp = TempDir::new().unwrap();
        let mut pm = ProfileManager::new(tmp.path().to_path_buf());
        pm.save_profile(&sample_profile("real")).unwrap();
        std::fs::write(tmp.path().join("notes.txt"), "not a profile").unwrap();
        std::fs::write(tmp.path().join("README.md"), "# nope").unwrap();

        pm.load().unwrap();

        assert_eq!(pm.list().len(), 1);
        assert!(pm.get("real").is_some());
    }

    #[test]
    fn load_reports_useful_error_for_invalid_toml() {
        let tmp = TempDir::new().unwrap();
        let mut pm = ProfileManager::new(tmp.path().to_path_buf());
        std::fs::write(tmp.path().join("broken.toml"), "this = is = not = toml").unwrap();

        let err = pm.load().unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("broken.toml"),
            "error should name the file: {msg}"
        );
    }

    #[test]
    fn load_is_read_only_for_missing_directory() {
        let tmp = TempDir::new().unwrap();
        let missing = tmp.path().join("does-not-exist");
        let mut pm = ProfileManager::new(missing.clone());

        pm.load().unwrap();

        assert!(pm.list().is_empty());
        assert!(
            !missing.exists(),
            "load() must not create the profiles directory"
        );
    }

    #[test]
    fn ensure_default_profiles_creates_default_when_empty() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("profiles");
        let mut pm = ProfileManager::new(dir.clone());

        pm.ensure_default_profiles().unwrap();

        assert!(pm.get("default").is_some());
        assert!(dir.join("default.toml").exists());
    }
}
