//! UI rendering for zapret2-tui

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    symbols,
    text::{Line, Span, Text},
    widgets::{
        Block, Borders, Cell, Clear, Gauge, Paragraph, Row, Scrollbar, ScrollbarOrientation,
        ScrollbarState, Table, Tabs, Wrap,
    },
    Frame,
};

use crate::app::{App, Tab};

pub fn draw(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(f.area());

    draw_header(f, app, chunks[0]);
    draw_content(f, app, chunks[1]);
    draw_footer(f, app, chunks[2]);
}

fn draw_header(f: &mut Frame, app: &App, area: Rect) {
    let titles: Vec<Line> = [Tab::Status, Tab::Profiles, Tab::Logs, Tab::Settings]
        .iter()
        .map(|t| {
            let title = t.title();
            Line::from(vec![Span::styled(
                format!("  {}  ", title),
                Style::default(),
            )])
        })
        .collect();

    let tabs = Tabs::new(titles)
        .block(
            Block::default()
                .title(" zapret2-tui ")
                .title_alignment(Alignment::Center)
                .borders(Borders::ALL),
        )
        .select(app.current_tab as usize)
        .highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .divider(symbols::line::VERTICAL);

    f.render_widget(tabs, area);
}

fn draw_content(f: &mut Frame, app: &App, area: Rect) {
    match app.current_tab {
        Tab::Status => draw_status_tab(f, app, area),
        Tab::Profiles => draw_profiles_tab(f, app, area),
        Tab::Logs => draw_logs_tab(f, app, area),
        Tab::Settings => draw_settings_tab(f, app, area),
    }
}

fn draw_status_tab(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(8),
            Constraint::Min(0),
        ])
        .margin(1)
        .split(area);

    // Status block
    let status_text = format!(
        "Daemon:     {}\n\
         Firewall:   {}\n\
         Profile:    {}\n\n\
         Press [s] to toggle, [r] to restart, [q] to quit",
        if app.status.daemon_running {
            "● Running"
        } else {
            "○ Stopped"
        },
        if app.status.firewall_active {
            "● Active"
        } else {
            "○ Inactive"
        },
        app.status
            .current_profile
            .as_deref()
            .unwrap_or("none"),
    );

    let status_color = if app.status.daemon_running {
        Color::Green
    } else {
        Color::Red
    };

    let status_para = Paragraph::new(status_text)
        .block(
            Block::default()
                .title(" Status ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(status_color)),
        )
        .style(Style::default().fg(Color::White));

    f.render_widget(status_para, chunks[0]);

    // Info block
    let info_text = "zapret2-tui v0.1.0\n\
        Terminal UI for zapret2 DPI bypass\n\
        \n\
        https://github.com/ni9aii/zapret2-tui";

    let info_para = Paragraph::new(info_text)
        .block(Block::default().title(" Info ").borders(Borders::ALL))
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });

    f.render_widget(info_para, chunks[1]);
}

fn draw_profiles_tab(f: &mut Frame, app: &App, area: Rect) {
    let text = "Profiles management\n\n\
        [↑/↓] Navigate  [Enter] Select  [a] Add  [d] Delete";

    let para = Paragraph::new(text)
        .block(Block::default().title(" Profiles ").borders(Borders::ALL))
        .wrap(Wrap { trim: true });

    f.render_widget(para, area);
}

fn draw_logs_tab(f: &mut Frame, app: &App, area: Rect) {
    let logs_text = if app.logs.is_empty() {
        "No logs yet. Start the daemon to see output.".to_string()
    } else {
        app.logs.join("\n")
    };

    let para = Paragraph::new(logs_text)
        .block(Block::default().title(" Logs ").borders(Borders::ALL))
        .wrap(Wrap { trim: true });

    f.render_widget(para, area);
}

fn draw_settings_tab(f: &mut Frame, app: &App, area: Rect) {
    let text = "Settings\n\n\
        Config path: /opt/zapret2/config\n\
        zapret2 base: /opt/zapret2\n\n\
        [e] Edit config  [r] Reload";

    let para = Paragraph::new(text)
        .block(Block::default().title(" Settings ").borders(Borders::ALL))
        .wrap(Wrap { trim: true });

    f.render_widget(para, area);
}

fn draw_footer(f: &mut Frame, app: &App, area: Rect) {
    let status = format!(
        " {} | {}",
        if app.status.daemon_running {
            "running"
        } else {
            "stopped"
        },
        app.current_tab.title()
    );

    let footer = Paragraph::new(status)
        .style(Style::default().fg(Color::DarkGray));

    f.render_widget(footer, area);
}
