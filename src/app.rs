//! Application state management

use zapret2_core::{Status, ZapretController};

use anyhow::Result;
use std::path::PathBuf;

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
    pub logs: Vec<String>,
    #[allow(dead_code)]
    controller: ZapretController,
}

impl App {
    pub fn new(config_path: Option<PathBuf>) -> Result<Self> {
        let controller = ZapretController::new(config_path)?;

        Ok(Self {
            current_tab: Tab::Status,
            status: Status::default(),
            logs: Vec::new(),
            controller,
        })
    }

    pub fn next_tab(&mut self) {
        self.current_tab = self.current_tab.next();
    }

    pub fn prev_tab(&mut self) {
        self.current_tab = self.current_tab.prev();
    }

    pub async fn toggle_status(&mut self) -> Result<()> {
        if self.status.daemon_running {
            self.add_log("Stopping zapret2...".to_string());
            match self.controller.stop().await {
                Ok(_) => {
                    self.add_log("zapret2 stopped".to_string());
                }
                Err(e) => {
                    self.add_log(format!("Stop failed: {}", e));
                }
            }
        } else {
            self.add_log("Starting zapret2...".to_string());
            match self.controller.start().await {
                Ok(_) => {
                    self.add_log("zapret2 started".to_string());
                }
                Err(e) => {
                    self.add_log(format!("Start failed: {}", e));
                }
            }
        }

        // Update status after operation
        self.update_status();
        Ok(())
    }

    pub async fn restart(&mut self) -> Result<()> {
        self.add_log("Restarting zapret2...".to_string());
        match self.controller.restart().await {
            Ok(_) => {
                self.add_log("zapret2 restarted".to_string());
            }
            Err(e) => {
                self.add_log(format!("Restart failed: {}", e));
            }
        }
        self.update_status();
        Ok(())
    }

    pub async fn on_tick(&mut self) -> Result<()> {
        // Pull latest status from controller
        self.update_status();
        Ok(())
    }

    fn update_status(&mut self) {
        self.status = self.controller.status();
    }

    pub fn add_log(&mut self, msg: String) {
        let timestamp = chrono::Local::now().format("%H:%M:%S");
        self.logs.push(format!("[{}] {}", timestamp, msg));
        // Keep last 1000 lines
        if self.logs.len() > 1000 {
            self.logs.remove(0);
        }
    }
}
