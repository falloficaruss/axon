use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

use crate::types::{Agent, AgentState, Session, SessionMode};

/// Sidebar component for displaying sessions and agents
pub struct Sidebar {
    /// Selected session index
    selected_session: usize,
    /// Selected agent index
    selected_agent: usize,
}

impl Sidebar {
    pub fn new() -> Self {
        Self {
            selected_session: 0,
            selected_agent: 0,
        }
    }

    /// Draw the sidebar
    pub fn draw(
        &self,
        frame: &mut Frame,
        area: Rect,
        session: &Session,
        agents: &[Agent],
        active_agent: Option<&Agent>,
    ) {
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(10), Constraint::Min(0)])
            .split(area);

        // Agent status section
        self.draw_agent_status(frame, layout[0], session, agents, active_agent);

        // Sessions section
        self.draw_sessions(frame, layout[1], session);
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
        let block = Block::default()
            .title(" Agents ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));

        let mut lines = vec![];
        lines.push(Line::from(vec![
            Span::styled("Mode: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::styled(
                match session.mode {
                    SessionMode::Auto => "AUTO",
                    SessionMode::Manual => "MANUAL",
                },
                Style::default().fg(Color::Yellow),
            ),
        ]));
        lines.push(Line::from(""));

        // Show all agents from registry with their states
        for agent in agents {
            let (icon, color) = match agent.state {
                AgentState::Idle => ("○", Color::Gray),
                AgentState::Running => ("●", Color::Green),
                AgentState::Completed => ("✓", Color::Blue),
                AgentState::Failed => ("✗", Color::Red),
            };

            // Highlight active agent
            let name_style = if active_agent.map(|a| a.id == agent.id).unwrap_or(false) {
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            lines.push(Line::from(vec![
                Span::styled(format!("{} ", icon), Style::default().fg(color)),
                Span::styled(&agent.name, name_style),
            ]));
        }

        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("Active: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::styled(
                active_agent.map(|a| a.name.as_str()).unwrap_or("None"),
                Style::default().fg(Color::Green),
            ),
        ]));

        let paragraph = Paragraph::new(Text::from(lines)).block(block);

        frame.render_widget(paragraph, area);
    }

    /// Draw sessions list
    fn draw_sessions(&self, frame: &mut Frame, area: Rect, _session: &Session) {
        let block = Block::default()
            .title(" Sessions ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));

        // Placeholder sessions
        let sessions = [
            "Current Session",
            "Previous Session 1",
            "Previous Session 2",
        ];

        let items: Vec<ListItem> = sessions
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let style = if i == self.selected_session {
                    Style::default()
                        .bg(Color::Blue)
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                ListItem::new(*s).style(style)
            })
            .collect();

        let list = List::new(items)
            .block(block)
            .highlight_style(Style::default().add_modifier(Modifier::BOLD));

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
    }

    #[test]
    fn test_sidebar_default() {
        let sidebar = Sidebar::default();
        assert_eq!(sidebar.selected_session, 0);
        assert_eq!(sidebar.selected_agent, 0);
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
