//! zapret2-tui — Terminal UI for zapret2

use anyhow::Result;
use clap::{Parser, ValueEnum};
use crossterm::{
    event::{self, Event, KeyEventKind},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io::{self, stdout};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::info;
use zapret2_core::privilege::PrivilegeMode;

mod app;
mod ui;

use app::App;

/// How the TUI obtains the privileges needed for firewall/daemon control.
#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
enum PrivilegeModeArg {
    /// Direct if already root, otherwise pkexec.
    Auto,
    /// Always use pkexec (polkit authentication).
    Pkexec,
    /// Never use pkexec (root/debug/server mode).
    Direct,
}

impl From<PrivilegeModeArg> for PrivilegeMode {
    fn from(arg: PrivilegeModeArg) -> Self {
        match arg {
            PrivilegeModeArg::Auto => PrivilegeMode::Auto,
            PrivilegeModeArg::Pkexec => PrivilegeMode::Pkexec,
            PrivilegeModeArg::Direct => PrivilegeMode::Direct,
        }
    }
}

/// Terminal UI for zapret2 DPI bypass
#[derive(Parser)]
#[command(name = "zapret2-tui")]
#[command(version)]
#[command(about, long_about = None)]
struct Args {
    /// Path to zapret2 config file
    #[arg(short, long, value_name = "FILE")]
    config: Option<PathBuf>,

    /// How to obtain privileges for firewall/daemon control
    #[arg(long, value_enum, default_value_t = PrivilegeModeArg::Auto)]
    privilege_mode: PrivilegeModeArg,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Setup Ctrl+C handler for graceful shutdown
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
    })
    .map_err(|e| anyhow::anyhow!("Failed to set Ctrl+C handler: {}", e))?;

    tracing_subscriber::fmt::init();
    info!("starting zapret2-tui");

    stdout().execute(EnterAlternateScreen)?;
    enable_raw_mode()?;

    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    let mut app = App::new(args.config, args.privilege_mode.into())?;

    let result = run_app(&mut terminal, &mut app, running).await;

    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;

    result
}

async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    running: Arc<AtomicBool>,
) -> Result<()> {
    let mut last_tick = std::time::Instant::now();
    let tick_rate = std::time::Duration::from_millis(250);

    while running.load(Ordering::SeqCst) {
        terminal.draw(|f| ui::draw(f, app))?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| std::time::Duration::from_secs(0));

        if crossterm::event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press && app.handle_key(key.code).await? {
                    info!("quit requested");
                    break;
                }
            }
        }

        if last_tick.elapsed() >= tick_rate {
            app.on_tick().await;
            last_tick = std::time::Instant::now();
        }
    }

    Ok(())
}
