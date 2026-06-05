//! zapret2-tui — Terminal UI for zapret2

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::{
    backend::CrosstermBackend,
    Frame, Terminal,
};
use std::io::{self, stdout};
use std::path::PathBuf;
use tracing::info;

mod app;
mod ui;

use app::App;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    info!("starting zapret2-tui");

    stdout().execute(EnterAlternateScreen)?;
    enable_raw_mode()?;

    // Parse optional config path from args (can be extended with clap later)
    let config_path = std::env::args().nth(1).map(PathBuf::from);
    
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    let mut app = App::new(config_path)?;

    let result = run_app(&mut terminal, &mut app).await;

    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;

    result
}

async fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut App) -> Result<()> {
    let mut last_tick = std::time::Instant::now();
    let tick_rate = std::time::Duration::from_millis(250);

    loop {
        terminal.draw(|f| ui::draw(f, app))?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| std::time::Duration::from_secs(0));

        if crossterm::event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Char('Q') => {
                            info!("quit requested");
                            break;
                        }
                        KeyCode::Char('s') | KeyCode::Char('S') => {
                            if let Err(e) = app.toggle_status().await {
                                app.add_log(format!("toggle error: {}", e));
                            }
                        }
                        KeyCode::Char('r') | KeyCode::Char('R') => {
                            if let Err(e) = app.restart().await {
                                app.add_log(format!("restart error: {}", e));
                            }
                        }
                        KeyCode::Tab => app.next_tab(),
                        KeyCode::BackTab => app.prev_tab(),
                        _ => {}
                    }
                }
            }
        }

        if last_tick.elapsed() >= tick_rate {
            if let Err(e) = app.on_tick().await {
                app.add_log(format!("tick error: {}", e));
            }
            last_tick = std::time::Instant::now();
        }
    }

    Ok(())
}