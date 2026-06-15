//! Application state management

use std::collections::VecDeque;
use std::path::PathBuf;

use anyhow::Result;
use chrono::Local;
use crossterm::event::KeyCode;
use tokio::sync::mpsc;
use zapret2_core::profile::{Profile, ProfileManager};
use zapret2_core::{Status, ZapretController, DEFAULT_ZAPRET_BASE};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Status,
    Profiles,
    Logs,
    Settings,
}

impl Tab {
    pub fn title(self) -> &'static str {
        match self {
            Tab::Status => "Status",
            Tab::Profiles => "Profiles",
            Tab::Logs => "Logs",
            Tab::Settings => "Settings",
        }
    }

    pub fn index(self) -> usize {
        match self {
            Tab::Status => 0,
            Tab::Profiles => 1,
            Tab::Logs => 2,
            Tab::Settings => 3,
        }
    }

    pub fn next(self) -> Self {
        match self {
            Tab::Status => Tab::Profiles,
            Tab::Profiles => Tab::Logs,
            Tab::Logs => Tab::Settings,
            Tab::Settings => Tab::Status,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Tab::Status => Tab::Settings,
            Tab::Profiles => Tab::Status,
            Tab::Logs => Tab::Profiles,
            Tab::Settings => Tab::Logs,
        }
    }
}

pub struct App {
    pub current_tab: Tab,
    pub status: Status,
    pub logs: VecDeque<String>,
    pub status_message: String,
    pub profiles: Vec<Profile>,
    pub profile_list_selected: usize,
    pub active_profile: Option<String>,
    pub show_help: bool,
    controller: ZapretController,
    log_rx: mpsc::UnboundedReceiver<String>,
}

impl App {
    pub fn new(config_path: Option<PathBuf>) -> Result<Self> {
        let mut controller = ZapretController::new(config_path)?;
        let log_rx = controller.take_log_receiver().unwrap_or_else(|| {
            let (_tx, rx) = mpsc::unbounded_channel();
            rx
        });

        let profiles_dir = PathBuf::from(DEFAULT_ZAPRET_BASE).join("profiles");
        let mut profile_manager = ProfileManager::new(profiles_dir);
        let mut profiles = Vec::new();
        if let Err(e) = profile_manager.load() {
            tracing::warn!("failed to load profiles: {e}");
        } else {
            profiles = profile_manager.list().into_iter().cloned().collect();
            profiles.sort_by(|a, b| a.name.cmp(&b.name));
        }

        let active_profile = controller
            .config()
            .current_profile
            .clone()
            .or_else(|| profiles.first().map(|p| p.name.clone()));
        let status = Status {
            current_profile: active_profile.clone(),
            ..Status::default()
        };

        Ok(Self {
            current_tab: Tab::Status,
            status,
            logs: VecDeque::new(),
            status_message: "Ready. Press 's' to start, 'r' to restart, 'h' for help.".to_string(),
            profiles,
            profile_list_selected: 0,
            active_profile,
            show_help: false,
            controller,
            log_rx,
        })
    }

    pub async fn handle_key(&mut self, key: KeyCode) -> Result<bool> {
        if self.show_help {
            self.show_help = false;
            return Ok(false);
        }

        match key {
            KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => return Ok(true),
            KeyCode::Tab => self.current_tab = self.current_tab.next(),
            KeyCode::BackTab => self.current_tab = self.current_tab.prev(),
            KeyCode::Char('s') | KeyCode::Char('S') => self.toggle_status().await?,
            KeyCode::Char('r') | KeyCode::Char('R') => self.restart().await?,
            KeyCode::Char('h') | KeyCode::Char('?') => self.show_help = true,
            KeyCode::Down if self.current_tab == Tab::Profiles => {
                self.profile_list_selected =
                    (self.profile_list_selected + 1).min(self.profiles.len().saturating_sub(1));
            }
            KeyCode::Up if self.current_tab == Tab::Profiles => {
                self.profile_list_selected = self.profile_list_selected.saturating_sub(1);
            }
            KeyCode::Enter if self.current_tab == Tab::Profiles => self.select_profile(),
            _ => {}
        }
        Ok(false)
    }

    pub async fn toggle_status(&mut self) -> Result<()> {
        if self.status.daemon_running {
            self.add_log("Stopping zapret2...");
            self.status_message = "Stopping...".to_string();
            match self.controller.stop().await {
                Ok(_) => {
                    self.add_log("zapret2 stopped");
                    self.status_message = "Stopped.".to_string();
                }
                Err(e) => {
                    self.add_log(&format!("Stop failed: {e}"));
                    self.status_message = format!("Stop failed: {e}");
                }
            }
        } else {
            self.add_log("Starting zapret2...");
            self.status_message = "Starting...".to_string();
            match self.controller.start().await {
                Ok(_) => {
                    self.add_log("zapret2 started");
                    self.status_message = "Running.".to_string();
                }
                Err(e) => {
                    self.add_log(&format!("Start failed: {e}"));
                    self.status_message = format!("Start failed: {e}");
                }
            }
        }

        self.update_status().await;
        Ok(())
    }

    pub async fn restart(&mut self) -> Result<()> {
        self.add_log("Restarting zapret2...");
        self.status_message = "Restarting...".to_string();
        match self.controller.restart().await {
            Ok(_) => {
                self.add_log("zapret2 restarted");
                self.status_message = "Restarted.".to_string();
            }
            Err(e) => {
                self.add_log(&format!("Restart failed: {e}"));
                self.status_message = format!("Restart failed: {e}");
            }
        }
        self.update_status().await;
        Ok(())
    }

    pub async fn update_status(&mut self) {
        self.status = self.controller.status().await;
        if self.status.current_profile.is_none() {
            self.status.current_profile = self.active_profile.clone();
        }
    }

    pub async fn on_tick(&mut self) {
        while let Ok(line) = self.log_rx.try_recv() {
            self.add_log(&line);
        }
        self.update_status().await;
    }

    pub fn select_profile(&mut self) {
        let Some(profile) = self.profiles.get(self.profile_list_selected).cloned() else {
            return;
        };
        match self.controller.apply_profile(&profile) {
            Ok(()) => {
                self.active_profile = Some(profile.name.clone());
                self.status.current_profile = Some(profile.name.clone());
                self.status_message = format!("Profile '{}' applied.", profile.name);
            }
            Err(e) => {
                // Leave active profile unchanged on failure.
                self.status_message = format!("Failed to apply profile '{}': {e}", profile.name);
            }
        }
        let msg = self.status_message.clone();
        self.add_log(&msg);
    }

    pub fn add_log(&mut self, msg: &str) {
        let timestamp = Local::now().format("%H:%M:%S");
        self.logs.push_back(format!("[{timestamp}] {msg}"));
        while self.logs.len() > 1000 {
            self.logs.pop_front();
        }
    }

    pub fn daemon_pid(&self) -> Option<u32> {
        self.controller.daemon_pid()
    }

    pub fn binary_path(&self) -> String {
        self.controller.config().nfqws2_bin().display().to_string()
    }

    pub fn config_path(&self) -> String {
        self.controller.config().config_path.display().to_string()
    }

    pub fn queue_number(&self) -> u16 {
        self.controller.config().qnum
    }

    pub fn desync_mark(&self) -> u32 {
        self.controller.config().desync_mark
    }

    pub fn postnat_mark(&self) -> u32 {
        self.controller.config().desync_mark_postnat
    }

    /// Active nfqws2 options as applied to the runtime config. Reflects the
    /// selected profile, so the Settings tab shows the real runtime effect of
    /// `apply_profile`.
    pub fn nfqws_opts(&self) -> &str {
        &self.controller.config().nfqws2_opt
    }
}
