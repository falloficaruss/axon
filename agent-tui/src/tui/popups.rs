use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Clear, List, ListItem, Paragraph, Wrap},
    Frame,
};
use std::collections::HashMap;

use crate::tui::theme;
use crate::tui::PendingConfirmation;
use crate::types::{Agent, Session, SessionMode};

pub struct PopupRenderer;

impl PopupRenderer {
    /// Draw the top application header
    pub fn draw_header(frame: &mut Frame, session: &Session, task_running: bool, active_agent: Option<&Agent>) {
        let area = Rect {
            x: frame.area().x,
            y: frame.area().y,
            width: frame.area().width,
            height: 3,
        };

        let mode_text = match session.mode {
            SessionMode::Auto => "AUTO",
            SessionMode::Manual => "MANUAL",
        };

        let agent_name = active_agent.map(|a| a.name.as_str()).unwrap_or("none");
        let task_text = if task_running { "RUNNING" } else { "READY" };

        let block = theme::glass_panel(" AXON ", theme::accent_cyan());
        let content = vec![
            Line::from(vec![
                ratatui::text::Span::styled(
                    "Axon",
                    Style::default()
                        .fg(theme::text_primary())
                        .add_modifier(Modifier::BOLD),
                ),
                ratatui::text::Span::styled(
                    "  orchestration studio",
                    Style::default().fg(theme::text_subtle()),
                ),
                ratatui::text::Span::raw("   "),
                ratatui::text::Span::styled(
                    format!("MODE {}", mode_text),
                    Style::default()
                        .fg(theme::accent_gold())
                        .add_modifier(Modifier::BOLD),
                ),
                ratatui::text::Span::raw("   "),
                ratatui::text::Span::styled(
                    format!("AGENT {}", agent_name),
                    Style::default().fg(theme::accent_mint()),
                ),
                ratatui::text::Span::raw("   "),
                ratatui::text::Span::styled(
                    format!("STATUS {}", task_text),
                    Style::default().fg(theme::accent_cyan()),
                ),
            ]),
        ];

        let paragraph = Paragraph::new(content)
            .block(block)
            .style(Style::default().bg(theme::panel_bg_alt()).fg(theme::text_primary()));

        frame.render_widget(paragraph, area);
    }

    /// Draw slash-command suggestions popup
    pub fn draw_command_suggestions(
        frame: &mut Frame,
        suggestions: &[(&str, &str)],
        selected_index: usize,
    ) {
        let gold = theme::accent_gold();
        let cloud = theme::text_primary();
        let slate = theme::text_subtle();

        if suggestions.is_empty() {
            return;
        }

        let visible_count = suggestions.len().min(8) as u16;
        let height = visible_count + 2;
        let width = frame.area().width.saturating_sub(20).clamp(44, 84);
        let area = Rect {
            x: 18.min(frame.area().width.saturating_sub(width)),
            y: frame.area().height.saturating_sub(height + 6),
            width,
            height,
        };

        let block = theme::glass_popup(" AXON COMMANDS ", gold);

        let items: Vec<ListItem> = suggestions
            .iter()
            .enumerate()
            .take(visible_count as usize)
            .map(|(i, (command, description))| {
                let style = if i == selected_index {
                    Style::default()
                        .bg(gold)
                        .fg(Color::Black)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(cloud)
                };

                ListItem::new(Line::from(vec![
                    ratatui::text::Span::styled(format!("{:<18}", command), style),
                    ratatui::text::Span::styled(
                        format!("  {}", description),
                        if i == selected_index {
                            style
                        } else {
                            Style::default().fg(slate)
                        },
                    ),
                ]))
            })
            .collect();

        let list = List::new(items)
            .block(block)
            .style(Style::default().bg(theme::panel_bg_alt()).fg(cloud));

        frame.render_widget(Clear, area);
        frame.render_widget(list, area);
    }

    /// Draw agent selector popup
    pub fn draw_agent_selector(frame: &mut Frame, agents: &[Agent], selected_index: usize) {
        let cyan = theme::accent_cyan();
        let area = Self::centered_rect(60, 60, frame.area());

        let block = theme::glass_popup(" SELECT AGENT ", cyan);

        let mut text: Vec<Line> = vec![Line::from("Available agents:"), Line::from("")];

        for (idx, agent) in agents.iter().enumerate() {
            let mut style = Style::default();
            if idx == selected_index {
                style = style
                    .bg(Color::Cyan)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD);
            }

            let line = Line::from(vec![
                ratatui::text::Span::styled(format!("{}. ", idx + 1), style),
                ratatui::text::Span::styled(&agent.name, style),
                ratatui::text::Span::raw(" - "),
                ratatui::text::Span::raw(&agent.description),
            ]);
            text.push(line);
        }

        text.push(Line::from(""));
        text.push(Line::from("Press number to select, ESC to cancel"));

        let paragraph = Paragraph::new(text).block(block).wrap(Wrap { trim: true });
        let paragraph = paragraph.style(Style::default().bg(theme::panel_bg_alt()).fg(theme::text_primary()));

        frame.render_widget(Clear, area);
        frame.render_widget(paragraph, area);
    }

    /// Draw confirmation dialog popup
    pub fn draw_confirmation_dialog(frame: &mut Frame, pending: Option<&PendingConfirmation>) {
        let magenta = Color::Rgb(222, 152, 255);
        let area = Self::centered_rect(70, 70, frame.area());

        let block = theme::glass_popup(
            pending.map(|p| p.title.as_str()).unwrap_or("Confirmation"),
            magenta,
        );

        let mut text = Vec::new();

        if let Some(pending) = pending {
            text.push(Line::from(pending.message.clone()));
            text.push(Line::from(""));
            text.push(Line::from(format!("Files ({}):", pending.changes.len())));

            for change in &pending.changes {
                let op_str = match change.operation {
                    crate::agent::agents::coder::FileOperation::Create => " [CREATE] ",
                    crate::agent::agents::coder::FileOperation::Update => " [UPDATE] ",
                    crate::agent::agents::coder::FileOperation::Delete => " [DELETE] ",
                };

                let op_color = match change.operation {
                    crate::agent::agents::coder::FileOperation::Create => Color::Green,
                    crate::agent::agents::coder::FileOperation::Update => Color::Yellow,
                    crate::agent::agents::coder::FileOperation::Delete => Color::Red,
                };

                let line = Line::from(vec![
                    ratatui::text::Span::styled(
                        op_str,
                        Style::default().fg(op_color).add_modifier(Modifier::BOLD),
                    ),
                    ratatui::text::Span::raw(change.file_path.to_string_lossy().to_string()),
                ]);
                text.push(line);
            }
        } else {
            text.push(Line::from("No pending confirmation."));
        }

        text.push(Line::from(""));
        text.push(Line::from(vec![
            ratatui::text::Span::raw("Press "),
            ratatui::text::Span::styled(
                "y",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            ratatui::text::Span::raw(" to accept, "),
            ratatui::text::Span::styled(
                "n",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
            ratatui::text::Span::raw(" to reject."),
        ]));

        let paragraph = Paragraph::new(text).block(block).wrap(Wrap { trim: true });

        frame.render_widget(Clear, area);
        frame.render_widget(paragraph, area);
    }

    /// Draw memory manager popup
    pub fn draw_memory_manager(
        frame: &mut Frame,
        keys: &[String],
        selected_index: usize,
        values: &HashMap<String, String>,
    ) {
        let area = Self::centered_rect(80, 80, frame.area());

        let block = theme::glass_popup(" MEMORY MANAGER ", theme::accent_gold());

        if keys.is_empty() {
            let text = vec![
                Line::from("No memory entries found."),
                Line::from(""),
                Line::from("Press ESC or 'q' to close, 'r' to refresh"),
            ];

            let paragraph = Paragraph::new(text)
                .block(block)
                .style(Style::default().bg(theme::panel_bg_alt()).fg(theme::text_primary()))
                .wrap(Wrap { trim: true });

            frame.render_widget(Clear, area);
            frame.render_widget(paragraph, area);
        } else {
            let list_layout = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
                .split(area);

            let items: Vec<ListItem> = keys
                .iter()
                .enumerate()
                .map(|(i, key)| {
                    let style = if i == selected_index {
                        Style::default()
                            .bg(Color::Yellow)
                            .fg(Color::Black)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    };
                    ListItem::new(key.clone()).style(style)
                })
                .collect();

            let list = List::new(items)
                .block(theme::glass_popup(" KEYS ", theme::accent_gold()))
                .highlight_style(Style::default().add_modifier(Modifier::BOLD));

            frame.render_widget(Clear, area);
            frame.render_widget(list, list_layout[0]);

            // Show value of selected key
            if let Some(key) = keys.get(selected_index) {
                let value_text = values
                    .get(key)
                    .map(|v| v.as_str())
                    .unwrap_or("Value not found (Press Enter to try fetching again or add to chat)");

                let paragraph = Paragraph::new(value_text)
                    .block(theme::glass_popup(" VALUE ", theme::accent_cyan()))
                    .style(Style::default().bg(theme::panel_bg_alt()).fg(theme::text_primary()))
                    .wrap(Wrap { trim: true });
                frame.render_widget(paragraph, list_layout[1]);
            }
        }
    }

    /// Draw status bar
    pub fn draw_status_bar(frame: &mut Frame, session: &Session) {
        let cloud = theme::text_primary();
        let navy = theme::status_bg();
        let status_area = Rect {
            x: frame.area().x,
            y: frame.area().height - 1,
            width: frame.area().width,
            height: 1,
        };

        let mode_text = match session.mode {
            SessionMode::Auto => "AUTO",
            SessionMode::Manual => "MANUAL",
        };

        let status = format!(
            "  MODE {}  |  {} messages  |  / commands  |  Tab focus  |  Ctrl+B rail  |  Ctrl+C quit  ",
            mode_text,
            session.messages.len()
        );

        let status_bar = Paragraph::new(status).style(
            Style::default()
                .bg(navy)
                .fg(cloud)
                .add_modifier(Modifier::BOLD),
        );

        frame.render_widget(status_bar, status_area);
    }

    /// Calculate centered rectangle
    pub fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
        let popup_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage((100 - percent_y) / 2),
                Constraint::Percentage(percent_y),
                Constraint::Percentage((100 - percent_y) / 2),
            ])
            .split(r);

        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage((100 - percent_x) / 2),
                Constraint::Percentage(percent_x),
                Constraint::Percentage((100 - percent_x) / 2),
            ])
            .split(popup_layout[1])[1]
    }
}
