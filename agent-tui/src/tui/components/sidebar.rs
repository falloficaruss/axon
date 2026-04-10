use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{List, ListItem, Paragraph},
    Frame,
};

use chrono::{DateTime, Local};

use crate::persistence::SessionMetadata;
use crate::tui::theme;
use crate::types::{Agent, AgentState, Session, SessionMode};

/// Sidebar component for displaying sessions and agents
pub struct Sidebar {
    /// Selected session index
    selected_session: usize,
    /// Selected agent index
    selected_agent: usize,
    /// Whether the sidebar is focused
    pub focused: bool,
    /// Last refresh timestamp
    last_refresh: Option<DateTime<Local>>,
}

impl Sidebar {
    pub fn new() -> Self {
        Self {
            selected_session: 0,
            selected_agent: 0,
            focused: false,
            last_refresh: None,
        }
    }

    /// Set the last refresh timestamp
    pub fn set_last_refresh(&mut self, timestamp: DateTime<Local>) {
        self.last_refresh = Some(timestamp);
    }

    /// Draw the sidebar
    pub fn draw(
        &self,
        frame: &mut Frame,
        area: Rect,
        session: &Session,
        agents: &[Agent],
        active_agent: Option<&Agent>,
        sessions: &[SessionMetadata],
    ) {
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(12),
                Constraint::Min(0),
                Constraint::Length(1),
            ])
            .split(area);

        // Agent status section
        self.draw_agent_status(frame, layout[0], session, agents, active_agent);

        // Sessions section
        self.draw_sessions(frame, layout[1], sessions, session.id.as_str());

        // Refresh timestamp at the bottom
        if let Some(ts) = self.last_refresh {
            let refresh_text = format!("Refreshed: {}", ts.format("%H:%M:%S"));
            let paragraph = Paragraph::new(Line::from(vec![Span::styled(
                refresh_text,
                Style::default().fg(theme::text_subtle()),
            )]))
            .style(Style::default().bg(theme::app_bg()));
            frame.render_widget(paragraph, layout[2]);
        }
    }

    /// Draw agent status panel
    fn draw_agent_status(
        &self,
        frame: &mut Frame,
        area: Rect,
        session: &Session,
        agents: &[Agent],
        active_agent: Option<&Agent>,
    ) {
        let slate = theme::text_subtle();
        let cyan = theme::accent_cyan();
        let mint = theme::accent_mint();
        let gold = theme::accent_gold();
        let coral = theme::accent_coral();
        let cloud = theme::text_primary();

        let block = theme::glass_panel(" AXON CONTROL ", cyan);

        let mut lines = vec![];
        lines.push(Line::from(vec![
            Span::styled(
                "Axon",
                Style::default()
                    .fg(cloud)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("  command center", Style::default().fg(slate)),
        ]));
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("Mode  ", Style::default().fg(slate).add_modifier(Modifier::BOLD)),
            Span::styled(
                match session.mode {
                    SessionMode::Auto => "AUTO",
                    SessionMode::Manual => "MANUAL",
                },
                Style::default().fg(gold).add_modifier(Modifier::BOLD),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::styled("Session ", Style::default().fg(slate).add_modifier(Modifier::BOLD)),
            Span::styled(session.title.as_str(), Style::default().fg(cloud)),
        ]));
        lines.push(Line::from(""));

        // Show all agents from registry with their states
        for agent in agents {
            let (icon, color) = match agent.state {
                AgentState::Idle => ("ID", slate),
                AgentState::Running => ("RN", mint),
                AgentState::Completed => ("OK", cyan),
                AgentState::Failed => ("ER", coral),
            };

            // Highlight active agent
            let name_style = if active_agent.map(|a| a.id == agent.id).unwrap_or(false) {
                Style::default()
                    .fg(cloud)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(slate)
            };

            lines.push(Line::from(vec![
                Span::styled(
                    format!("[{}] ", icon),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
                Span::styled(&agent.name, name_style),
            ]));
        }

        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("Active  ", Style::default().fg(slate).add_modifier(Modifier::BOLD)),
            Span::styled(
                active_agent.map(|a| a.name.as_str()).unwrap_or("None"),
                Style::default().fg(mint).add_modifier(Modifier::BOLD),
            ),
        ]));

        let paragraph = Paragraph::new(Text::from(lines))
            .block(block)
            .style(Style::default().bg(theme::panel_bg()).fg(cloud));

        frame.render_widget(paragraph, area);
    }

    /// Draw sessions list
    fn draw_sessions(
        &self,
        frame: &mut Frame,
        area: Rect,
        sessions: &[SessionMetadata],
        current_session_id: &str,
    ) {
        let cyan = theme::accent_cyan();
        let gold = theme::accent_gold();
        let cloud = theme::text_primary();
        let slate = theme::text_subtle();

        let border_color = if self.focused {
            gold
        } else {
            cyan
        };
        let block = theme::glass_panel(" SESSIONS ", border_color);

        let items: Vec<ListItem> = sessions
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let is_current = s.id == current_session_id;
                let is_selected = i == self.selected_session;

                let mut style = Style::default();
                if is_selected {
                    let bg = if self.focused {
                        gold
                    } else {
                        Color::Rgb(33, 43, 60)
                    };
                    let fg = if self.focused {
                        Color::Black
                    } else {
                        cloud
                    };
                    style = style.bg(bg).fg(fg).add_modifier(Modifier::BOLD);
                } else if is_current {
                    style = style.fg(cyan);
                } else {
                    style = style.fg(slate);
                }

                let prefix = if is_current { "* " } else { "  " };
                let content = format!("{}{}", prefix, s.title);

                ListItem::new(content).style(style)
            })
            .collect();

        let list = List::new(items)
            .block(block)
            .highlight_style(Style::default().add_modifier(Modifier::BOLD))
            .style(Style::default().bg(theme::panel_bg()).fg(cloud));

        frame.render_widget(list, area);
    }

    /// Navigate sessions up
    pub fn previous_session(&mut self) {
        if self.selected_session > 0 {
            self.selected_session -= 1;
        }
    }

    /// Navigate sessions down
    pub fn next_session(&mut self, max: usize) {
        if self.selected_session < max.saturating_sub(1) {
            self.selected_session += 1;
        }
    }

    /// Get selected session index
    pub fn selected_session(&self) -> usize {
        self.selected_session
    }
}

impl Default for Sidebar {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sidebar_new() {
        let sidebar = Sidebar::new();
        assert_eq!(sidebar.selected_session, 0);
        assert_eq!(sidebar.selected_agent, 0);
        assert!(sidebar.last_refresh.is_none());
    }

    #[test]
    fn test_sidebar_default() {
        let sidebar = Sidebar::default();
        assert_eq!(sidebar.selected_session, 0);
        assert_eq!(sidebar.selected_agent, 0);
        assert!(sidebar.last_refresh.is_none());
    }

    #[test]
    fn test_sidebar_set_last_refresh() {
        let mut sidebar = Sidebar::new();
        let now = Local::now();
        sidebar.set_last_refresh(now);
        assert_eq!(sidebar.last_refresh, Some(now));
    }

    #[test]
    fn test_sidebar_previous_session() {
        let mut sidebar = Sidebar::new();
        sidebar.selected_session = 5;

        sidebar.previous_session();
        assert_eq!(sidebar.selected_session, 4);

        sidebar.selected_session = 0;
        sidebar.previous_session();
        assert_eq!(sidebar.selected_session, 0); // Should not go negative
    }

    #[test]
    fn test_sidebar_next_session() {
        let mut sidebar = Sidebar::new();
        sidebar.selected_session = 0;

        sidebar.next_session(5);
        assert_eq!(sidebar.selected_session, 1);

        sidebar.selected_session = 4;
        sidebar.next_session(5);
        assert_eq!(sidebar.selected_session, 4); // Should not exceed max
    }

    #[test]
    fn test_sidebar_selected_session() {
        let mut sidebar = Sidebar::new();
        sidebar.selected_session = 3;

        assert_eq!(sidebar.selected_session(), 3);
    }

    #[test]
    fn test_sidebar_next_session_at_boundary() {
        let mut sidebar = Sidebar::new();
        sidebar.selected_session = 0;

        sidebar.next_session(0);
        assert_eq!(sidebar.selected_session, 0);

        sidebar.next_session(1);
        assert_eq!(sidebar.selected_session, 0);
    }
}
