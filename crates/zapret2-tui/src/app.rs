//! Application state management

use zapret2_core::{Status, ZapretController};

use anyhow::Result;

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
    pub running: bool,
    controller: Option<ZapretController>,
}

impl App {
    pub fn new() -> Self {
        Self {
            current_tab: Tab::Status,
            status: Status::default(),
            logs: Vec::new(),
            running: true,
            controller: None,
        }
    }

    pub fn next_tab(&mut self) {
        self.current_tab = self.current_tab.next();
    }

    pub fn prev_tab(&mut self) {
        self.current_tab = self.current_tab.prev();
    }

    pub async fn toggle_status(&mut self) -> Result<()> {
        // Placeholder: will integrate with controller
        self.status.daemon_running = !self.status.daemon_running;
        self.add_log(format!(
            "daemon {}",
            if self.status.daemon_running {
                "started"
            } else {
                "stopped"
            }
        ));
        Ok(())
    }

    pub async fn restart(&mut self) -> Result<()> {
        self.add_log("restart requested".to_string());
        Ok(())
    }

    pub async fn on_tick(&mut self) -> Result<()> {
        // Update status from controller if available
        Ok(())
    }

    pub fn add_log(&mut self, msg: String) {
        let timestamp = chrono::Local::now().format("%H:%M:%S");
        self.logs.push(format!("[{}] {}", timestamp, msg));
        // Keep last 1000 lines
        if self.logs.len() > 1000 {
            self.logs.drain(0..self.logs.len() - 1000);
        }
    }

    pub fn should_quit(&self) -> bool {
        !self.running
    }
}
