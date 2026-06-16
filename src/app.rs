//! Application state management

use std::collections::VecDeque;
use std::path::PathBuf;

use anyhow::Result;
use chrono::Local;
use crossterm::event::KeyCode;
use tokio::sync::mpsc;
use zapret2_core::privilege::PrivilegeMode;
use zapret2_core::profile::{Profile, ProfileManager};
use zapret2_core::{Status, ZapretController, ZapretError, DEFAULT_ZAPRET_BASE};

use crate::modal::{Modal, ProfileForm};

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
    pub modal: Modal,
    controller: ZapretController,
    profile_manager: ProfileManager,
    log_rx: mpsc::UnboundedReceiver<String>,
}

impl App {
    pub fn new(config_path: Option<PathBuf>, privilege_mode: PrivilegeMode) -> Result<Self> {
        let mut controller = ZapretController::new(config_path)?;
        controller.set_privilege_mode(privilege_mode);
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
            modal: Modal::None,
            controller,
            profile_manager,
            log_rx,
        })
    }

    pub async fn handle_key(&mut self, key: KeyCode) -> Result<bool> {
        if self.show_help {
            self.show_help = false;
            return Ok(false);
        }

        // A modal captures all input until dismissed.
        if self.modal.is_open() {
            self.handle_modal_key(key).await;
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
            KeyCode::Char('n') | KeyCode::Char('N') if self.current_tab == Tab::Profiles => {
                self.modal = Modal::Form(ProfileForm::create());
            }
            KeyCode::Char('e') | KeyCode::Char('E') if self.current_tab == Tab::Profiles => {
                if let Some(profile) = self.profiles.get(self.profile_list_selected) {
                    self.modal = Modal::Form(ProfileForm::edit(profile));
                }
            }
            KeyCode::Char('d') | KeyCode::Char('D') if self.current_tab == Tab::Profiles => {
                if let Some(profile) = self.profiles.get(self.profile_list_selected) {
                    self.modal = Modal::DeleteConfirm {
                        name: profile.name.clone(),
                    };
                }
            }
            _ => {}
        }
        Ok(false)
    }

    /// Handle a key while a modal is open.
    async fn handle_modal_key(&mut self, key: KeyCode) {
        match &mut self.modal {
            Modal::Form(form) => match key {
                KeyCode::Esc => self.modal = Modal::None,
                KeyCode::Enter => self.submit_form().await,
                KeyCode::Tab | KeyCode::Down => form.focus_next(),
                KeyCode::BackTab | KeyCode::Up => form.focus_prev(),
                KeyCode::Backspace => form.backspace(),
                KeyCode::Char(c) => form.input(c),
                _ => {}
            },
            Modal::DeleteConfirm { .. } => match key {
                KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                    self.confirm_delete().await
                }
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => self.modal = Modal::None,
                _ => {}
            },
            Modal::None => {}
        }
    }

    /// Validate and persist the open profile form. On validation failure the
    /// form stays open with an error; nothing is written. On a write failure
    /// (e.g. no permission for /opt/zapret2/profiles) the form also stays open.
    async fn submit_form(&mut self) {
        let Modal::Form(form) = &self.modal else {
            return;
        };
        // Clone out so we can freely mutate `self` below (the form borrows
        // `self.modal`).
        let form = form.clone();
        let profile = match form.validate() {
            Ok(p) => p,
            Err(e) => {
                if let Modal::Form(form) = &mut self.modal {
                    form.error = Some(e);
                }
                return;
            }
        };

        if let Err(e) = self.controller.save_profile(&profile).await {
            if let Modal::Form(form) = &mut self.modal {
                form.error = Some(format!("save failed: {e}"));
            }
            return;
        }
        // A rename (edit where the name changed) removes the old file.
        let is_edit = form.editing.is_some();
        if let Some(old) = form.editing.filter(|old| *old != profile.name) {
            if let Err(e) = self.controller.remove_profile(&old).await {
                self.add_log(&format!(
                    "renamed profile but failed to remove '{old}': {e}"
                ));
            }
        }

        let verb = if is_edit { "updated" } else { "created" };
        self.status_message = format!("Profile '{}' {verb}.", profile.name);
        self.modal = Modal::None;
        self.reload_profiles();
        self.refresh_profiles(Some(&profile.name));
        let msg = self.status_message.clone();
        self.add_log(&msg);
    }

    /// Delete the profile named by the open delete-confirm modal.
    async fn confirm_delete(&mut self) {
        let Modal::DeleteConfirm { name } = &self.modal else {
            return;
        };
        let name = name.clone();
        match self.controller.remove_profile(&name).await {
            Ok(()) => {
                self.status_message = format!("Profile '{name}' deleted.");
                if self.active_profile.as_deref() == Some(name.as_str()) {
                    self.active_profile = None;
                    self.status.current_profile = None;
                }
            }
            Err(e) => {
                self.status_message = format!("Failed to delete '{name}': {e}");
            }
        }
        self.modal = Modal::None;
        self.reload_profiles();
        self.refresh_profiles(None);
        let msg = self.status_message.clone();
        self.add_log(&msg);
    }

    /// Re-read the profiles directory into the in-memory manager.
    fn reload_profiles(&mut self) {
        if let Err(e) = self.profile_manager.load() {
            tracing::warn!("failed to reload profiles: {e}");
        }
    }

    /// Rebuild the cached profile list from the manager, keeping the selection
    /// on `keep` when given (else clamped into range).
    fn refresh_profiles(&mut self, keep: Option<&str>) {
        self.profiles = self.profile_manager.list().into_iter().cloned().collect();
        self.profiles.sort_by(|a, b| a.name.cmp(&b.name));
        self.profile_list_selected = match keep {
            Some(name) => self
                .profiles
                .iter()
                .position(|p| p.name == name)
                .unwrap_or(0),
            None => self
                .profile_list_selected
                .min(self.profiles.len().saturating_sub(1)),
        };
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
                    let msg = if matches!(e, ZapretError::AuthCancelled) {
                        "Authentication cancelled.".to_string()
                    } else {
                        format!("Start failed: {e}")
                    };
                    self.add_log(&msg);
                    self.status_message = msg;
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
                let msg = if matches!(e, ZapretError::AuthCancelled) {
                    "Authentication cancelled.".to_string()
                } else {
                    format!("Restart failed: {e}")
                };
                self.add_log(&msg);
                self.status_message = msg;
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
        if self.logs.len() > 1000 {
            self.logs.drain(..self.logs.len() - 1000);
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
