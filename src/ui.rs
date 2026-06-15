//! UI rendering for zapret2-tui

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table, TableState, Tabs, Wrap},
    Frame,
};

use crate::app::{App, Tab};
use crate::modal::{Modal, ProfileForm};

pub fn draw(f: &mut Frame, app: &App) {
    let area = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(2),
        ])
        .split(area);

    draw_header(f, app, chunks[0]);
    draw_content(f, app, chunks[1]);
    draw_footer(f, app, chunks[2]);

    match &app.modal {
        Modal::Form(form) => draw_profile_form(f, form, area),
        Modal::DeleteConfirm { name } => draw_delete_confirm(f, name, area),
        Modal::None => {}
    }

    if app.show_help {
        draw_help(f, area);
    }
}

fn draw_header(f: &mut Frame, app: &App, area: Rect) {
    let titles: Vec<Line> = [Tab::Status, Tab::Profiles, Tab::Logs, Tab::Settings]
        .iter()
        .map(|t| Line::from(vec![Span::raw(format!("  {}  ", t.title()))]))
        .collect();

    let tabs = Tabs::new(titles)
        .block(
            Block::default()
                .title(" zapret2-tui ")
                .borders(Borders::ALL),
        )
        .select(app.current_tab.index())
        .highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );

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
            Constraint::Length(6),
            Constraint::Min(0),
        ])
        .margin(1)
        .split(area);

    let daemon_color = if app.status.daemon_running {
        Color::Green
    } else {
        Color::Red
    };
    let daemon_text = if app.status.daemon_running {
        "RUNNING"
    } else {
        "STOPPED"
    };
    let daemon_status = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("Daemon: ", Style::default().fg(Color::Gray)),
            Span::styled(
                daemon_text,
                Style::default()
                    .fg(daemon_color)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("PID: ", Style::default().fg(Color::Gray)),
            Span::raw(
                app.daemon_pid()
                    .map(|p| p.to_string())
                    .unwrap_or_else(|| "-".to_string()),
            ),
        ]),
        Line::from(vec![
            Span::styled("Binary: ", Style::default().fg(Color::Gray)),
            Span::raw(app.binary_path()),
        ]),
    ])
    .block(Block::default().title(" Daemon ").borders(Borders::ALL));
    f.render_widget(daemon_status, chunks[0]);

    let fw_color = if app.status.firewall_active {
        Color::Green
    } else {
        Color::Red
    };
    let fw_text = if app.status.firewall_active {
        "ACTIVE"
    } else {
        "INACTIVE"
    };
    let firewall_status = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("Firewall: ", Style::default().fg(Color::Gray)),
            Span::styled(
                fw_text,
                Style::default().fg(fw_color).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("Queue: ", Style::default().fg(Color::Gray)),
            Span::raw(app.queue_number().to_string()),
        ]),
        Line::from(vec![
            Span::styled("Desync mark: ", Style::default().fg(Color::Gray)),
            Span::raw(format!("{:#x}", app.desync_mark())),
        ]),
    ])
    .block(Block::default().title(" Firewall ").borders(Borders::ALL));
    f.render_widget(firewall_status, chunks[1]);

    let msg = Paragraph::new(app.status_message.as_str())
        .block(Block::default().title(" Status ").borders(Borders::ALL))
        .wrap(Wrap { trim: true });
    f.render_widget(msg, chunks[2]);
}

fn draw_profiles_tab(f: &mut Frame, app: &App, area: Rect) {
    if app.profiles.is_empty() {
        let empty =
            Paragraph::new("No profiles found. Profiles are loaded from /opt/zapret2/profiles.")
                .block(Block::default().title(" Profiles ").borders(Borders::ALL))
                .alignment(Alignment::Center);
        f.render_widget(empty, area);
        return;
    }

    let rows: Vec<Row> = app
        .profiles
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let mut style = Style::default();
            if app.active_profile.as_deref() == Some(p.name.as_str()) {
                style = style.fg(Color::Green);
            }
            if i == app.profile_list_selected {
                style = style.bg(Color::Cyan).fg(Color::Black);
            }
            Row::new(vec![
                Cell::from(p.name.clone()).style(style),
                Cell::from(p.description.clone()).style(style),
                Cell::from(p.strategy.clone()).style(style),
                Cell::from(p.hostlists.join(", ")).style(style),
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(16),
            Constraint::Length(32),
            Constraint::Length(22),
            Constraint::Min(20),
        ],
    )
    .header(
        Row::new(vec!["Name", "Description", "Strategy", "Hostlists"])
            .style(Style::default().add_modifier(Modifier::BOLD)),
    )
    .block(
        Block::default()
            .title(" Profiles (↑/↓ move, Enter select, n new, e edit, d delete) ")
            .borders(Borders::ALL),
    );

    let mut state = TableState::default().with_selected(Some(app.profile_list_selected));
    f.render_stateful_widget(table, area, &mut state);
}

fn draw_logs_tab(f: &mut Frame, app: &App, area: Rect) {
    let log_text: Vec<Line> = if app.logs.is_empty() {
        vec![Line::from(
            "No logs yet. Start the daemon to see nfqws2 output.",
        )]
    } else {
        app.logs
            .iter()
            .map(|line| Line::from(line.as_str()))
            .collect()
    };

    let logs = Paragraph::new(Text::from(log_text))
        .block(Block::default().title(" Logs ").borders(Borders::ALL))
        .scroll((
            app.logs.len().saturating_sub(area.height as usize) as u16,
            0,
        ))
        .wrap(Wrap { trim: false });
    f.render_widget(logs, area);
}

fn draw_settings_tab(f: &mut Frame, app: &App, area: Rect) {
    let settings = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("Config path: ", Style::default().fg(Color::Gray)),
            Span::raw(app.config_path()),
        ]),
        Line::from(vec![
            Span::styled("Binary: ", Style::default().fg(Color::Gray)),
            Span::raw(app.binary_path()),
        ]),
        Line::from(vec![
            Span::styled("Queue number: ", Style::default().fg(Color::Gray)),
            Span::raw(app.queue_number().to_string()),
        ]),
        Line::from(vec![
            Span::styled("Desync mark: ", Style::default().fg(Color::Gray)),
            Span::raw(format!("{:#x}", app.desync_mark())),
        ]),
        Line::from(vec![
            Span::styled("Postnat mark: ", Style::default().fg(Color::Gray)),
            Span::raw(format!("{:#x}", app.postnat_mark())),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Active profile: ", Style::default().fg(Color::Gray)),
            Span::styled(
                app.active_profile.as_deref().unwrap_or("none"),
                Style::default().fg(Color::Cyan),
            ),
        ]),
        Line::from(vec![
            Span::styled("Profiles loaded: ", Style::default().fg(Color::Gray)),
            Span::raw(app.profiles.len().to_string()),
        ]),
        Line::from(vec![
            Span::styled("nfqws2 opts: ", Style::default().fg(Color::Gray)),
            Span::raw(app.nfqws_opts().to_string()),
        ]),
    ])
    .block(Block::default().title(" Settings ").borders(Borders::ALL));
    f.render_widget(settings, area);
}

fn draw_footer(f: &mut Frame, app: &App, area: Rect) {
    let mut spans = vec![
        Span::styled("Tab", Style::default().fg(Color::Cyan)),
        Span::raw(" next | "),
        Span::styled("s", Style::default().fg(Color::Cyan)),
        Span::raw(" start/stop | "),
        Span::styled("r", Style::default().fg(Color::Cyan)),
        Span::raw(" restart | "),
    ];
    if app.current_tab == Tab::Profiles {
        spans.extend([
            Span::styled("n/e/d", Style::default().fg(Color::Cyan)),
            Span::raw(" new/edit/del | "),
        ]);
    }
    spans.extend([
        Span::styled("h", Style::default().fg(Color::Cyan)),
        Span::raw(" help | "),
        Span::styled("q", Style::default().fg(Color::Cyan)),
        Span::raw(" quit"),
    ]);
    let footer = Paragraph::new(Line::from(spans)).block(Block::default().borders(Borders::TOP));
    f.render_widget(footer, area);
}

fn draw_help(f: &mut Frame, area: Rect) {
    let help_text = vec![
        Line::from(Span::styled(
            "Keyboard Shortcuts",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from("Tab / Shift+Tab  Switch tabs"),
        Line::from("s                Start/Stop daemon"),
        Line::from("r                Restart daemon"),
        Line::from("↑/↓              Navigate profiles"),
        Line::from("Enter            Select highlighted profile"),
        Line::from("n / e / d        New / edit / delete profile"),
        Line::from("h / ?            This help"),
        Line::from("q / Esc          Quit"),
        Line::from(""),
        Line::from("In a dialog: Tab/↑↓ move field, Enter confirm, Esc cancel"),
        Line::from(""),
        Line::from("Press any key to close"),
    ];

    let popup_area = centered_rect(54, 44, area);
    let help = Paragraph::new(Text::from(help_text))
        .block(Block::default().title(" Help ").borders(Borders::ALL));
    f.render_widget(Clear, popup_area);
    f.render_widget(help, popup_area);
}

fn draw_profile_form(f: &mut Frame, form: &ProfileForm, area: Rect) {
    let labels = ProfileForm::field_labels();
    let mut lines: Vec<Line> = Vec::new();
    for (i, label) in labels.iter().enumerate() {
        let focused = i == form.focus;
        let marker = if focused { "> " } else { "  " };
        let label_style = if focused {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Gray)
        };
        let value = form.field_value(i);
        let value_span = if focused {
            Span::styled(format!("{value}_"), Style::default().fg(Color::White))
        } else {
            Span::raw(value.to_string())
        };
        lines.push(Line::from(vec![
            Span::raw(marker),
            Span::styled(format!("{label:<12}"), label_style),
            value_span,
        ]));
    }

    lines.push(Line::from(""));
    if let Some(err) = &form.error {
        lines.push(Line::from(Span::styled(
            format!("✗ {err}"),
            Style::default().fg(Color::Red),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            "Hostlists: comma or space separated",
            Style::default().fg(Color::DarkGray),
        )));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Tab/↑↓ move · Enter save · Esc cancel",
        Style::default().fg(Color::DarkGray),
    )));

    let popup_area = centered_rect(64, 56, area);
    let form_widget = Paragraph::new(Text::from(lines)).block(
        Block::default()
            .title(format!(" {} ", form.title()))
            .borders(Borders::ALL),
    );
    f.render_widget(Clear, popup_area);
    f.render_widget(form_widget, popup_area);
}

fn draw_delete_confirm(f: &mut Frame, name: &str, area: Rect) {
    let lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::raw("Delete profile "),
            Span::styled(
                format!("'{name}'"),
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
            Span::raw("?"),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "y / Enter  confirm     n / Esc  cancel",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    let popup_area = centered_rect(50, 24, area);
    let widget = Paragraph::new(Text::from(lines))
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .title(" Confirm delete ")
                .borders(Borders::ALL),
        );
    f.render_widget(Clear, popup_area);
    f.render_widget(widget, popup_area);
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}
