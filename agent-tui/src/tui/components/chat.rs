use ratatui::{
    layout::{Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap},
    Frame,
};

use crate::types::{Message, MessageRole, Session};
use crate::tui::markdown;

/// Chat component for displaying messages
pub struct Chat {
    /// Scroll offset
    scroll: u16,
    /// Whether to auto-scroll to bottom
    auto_scroll: bool,
    /// Scrollbar state
    scroll_state: ScrollbarState,
    /// Whether an agent is currently streaming
    is_streaming: bool,
}

impl Chat {
    pub fn new() -> Self {
        Self {
            scroll: 0,
            auto_scroll: true,
            scroll_state: ScrollbarState::new(0),
            is_streaming: false,
        }
    }

    /// Add a message to the chat
    pub fn add_message(&mut self, _message: Message) {
        // Message is stored in session, we just trigger a re-render
        if self.auto_scroll {
            self.scroll = u16::MAX;
        }
    }

    /// Set streaming state
    pub fn set_streaming(&mut self, streaming: bool) {
        self.is_streaming = streaming;
        if streaming && self.auto_scroll {
            self.scroll = u16::MAX;
        }
    }

    /// Clear the chat display
    pub fn clear(&mut self) {
        self.scroll = 0;
        self.scroll_state = ScrollbarState::new(0);
        self.is_streaming = false;
    }

    /// Scroll up
    pub fn scroll_up(&mut self, amount: u16) {
        self.auto_scroll = false;
        self.scroll = self.scroll.saturating_sub(amount);
        self.scroll_state = self.scroll_state.position(self.scroll as usize);
    }

    /// Scroll down
    pub fn scroll_down(&mut self, amount: u16) {
        self.scroll = self.scroll.saturating_add(amount);
        self.scroll_state = self.scroll_state.position(self.scroll as usize);
        // TODO: Check if at bottom to re-enable auto-scroll
    }

    /// Draw the chat component
    pub fn draw(&mut self, frame: &mut Frame, area: Rect, session: &Session) {
        let title = if self.is_streaming {
            format!(" Chat - {} - Streaming... ", session.title)
        } else {
            format!(" Chat - {} ", session.title)
        };

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::White));

        // Render all messages with markdown
        let mut all_lines: Vec<Line> = Vec::new();
        for message in &session.messages {
            // Header with timestamp and sender
            let (prefix, style) = match message.role {
                MessageRole::User => (
                    "You",
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
                MessageRole::Agent => {
                    let agent_name = message
                        .agent_id
                        .as_ref()
                        .map(|id| id.as_str())
                        .unwrap_or("Agent");
                    (
                        agent_name,
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    )
                }
                MessageRole::System => (
                    "System",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
            };

            // Timestamp
            let timestamp = message.timestamp.format("%H:%M:%S").to_string();
            let header = Line::from(vec![
                Span::styled(
                    format!("[{}] ", timestamp),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(format!("{}:", prefix), style),
            ]);
            all_lines.push(header);

            // Parse message content with markdown
            let content_lines = markdown::parse_markdown(&message.content);
            for line in content_lines {
                // Indent content by modifying spans
                let indented_spans: Vec<Span> = line.spans.into_iter().map(|mut span| {
                    if !span.content.starts_with('│') && !span.content.starts_with('┌') && !span.content.starts_with('└') {
                        span.content = format!("  {}", span.content).into();
                    }
                    span
                }).collect();
                all_lines.push(Line::from(indented_spans));
            }

            // Empty line between messages
            all_lines.push(Line::from(""));
        }

        // Add streaming indicator if agent is currently streaming
        if self.is_streaming {
            all_lines.push(Line::from(Span::styled(
                "  ▌",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )));
        }

        let content_height = all_lines.len();
        
        let paragraph = Paragraph::new(Text::from(all_lines))
            .block(block)
            .wrap(Wrap { trim: true })
            .scroll((self.scroll, 0));

        frame.render_widget(paragraph, area);

        // Update scroll state after rendering
        self.scroll_state = self.scroll_state.content_length(content_height);

        // Scrollbar
        let scrollbar = Scrollbar::default()
            .orientation(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("↑"))
            .end_symbol(Some("↓"));

        frame.render_stateful_widget(
            scrollbar,
            area.inner(Margin {
                vertical: 1,
                horizontal: 0,
            }),
            &mut self.scroll_state,
        );
    }
}

impl Default for Chat {
    fn default() -> Self {
        Self::new()
    }
}
