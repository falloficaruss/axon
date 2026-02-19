use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::tui::AppMode;

/// Input component for text entry
pub struct Input {
    /// Current input content
    content: String,
    /// Cursor position
    cursor: usize,
    /// Command history
    history: Vec<String>,
    /// Current history index (None means new input)
    history_index: Option<usize>,
    /// Saved input when navigating history
    saved_input: String,
}

impl Input {
    pub fn new() -> Self {
        Self {
            content: String::new(),
            cursor: 0,
            history: vec![],
            history_index: None,
            saved_input: String::new(),
        }
    }

    /// Get current content
    pub fn get_content(&self) -> String {
        self.content.clone()
    }

    /// Check if content is empty
    pub fn is_empty(&self) -> bool {
        self.content.is_empty()
    }

    /// Insert a character at cursor position
    pub fn insert_char(&mut self, c: char) {
        self.content.insert(self.cursor, c);
        self.cursor += 1;
    }

    /// Delete character before cursor
    pub fn delete_char(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            self.content.remove(self.cursor);
        }
    }

    /// Move cursor left
    pub fn move_cursor_left(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    /// Move cursor right
    pub fn move_cursor_right(&mut self) {
        if self.cursor < self.content.len() {
            self.cursor += 1;
        }
    }

    /// Clear input
    pub fn clear(&mut self) {
        self.content.clear();
        self.cursor = 0;
        self.history_index = None;
        self.saved_input.clear();
    }

    /// Navigate to previous history entry
    pub fn previous_history(&mut self) {
        if self.history.is_empty() {
            return;
        }

        if self.history_index.is_none() {
            // Save current input
            self.saved_input = self.content.clone();
            self.history_index = Some(self.history.len() - 1);
        } else if self.history_index.unwrap() > 0 {
            self.history_index = Some(self.history_index.unwrap() - 1);
        } else {
            return;
        }

        if let Some(index) = self.history_index {
            self.content = self.history[index].clone();
            self.cursor = self.content.len();
        }
    }

    /// Navigate to next history entry
    pub fn next_history(&mut self) {
        if let Some(index) = self.history_index {
            if index + 1 < self.history.len() {
                self.history_index = Some(index + 1);
                self.content = self.history[index + 1].clone();
                self.cursor = self.content.len();
            } else {
                // Restore saved input
                self.history_index = None;
                self.content = self.saved_input.clone();
                self.cursor = self.content.len();
            }
        }
    }

    /// Add content to history
    fn add_to_history(&mut self, content: &str) {
        if !content.trim().is_empty() {
            // Don't add duplicates at the end
            if self.history.last() != Some(&content.to_string()) {
                self.history.push(content.to_string());
            }
        }
    }

    /// Autocomplete (placeholder)
    pub fn autocomplete(&mut self) {
        // TODO: Implement autocomplete logic
        // For now, just insert a tab
        self.insert_char('\t');
    }

    /// Draw the input component
    pub fn draw(&self, frame: &mut Frame, area: Rect, mode: AppMode) {
        let (title, border_color) = match mode {
            AppMode::Command => (" Command (press ESC to cancel) ", Color::Yellow),
            AppMode::Normal => (" Input ", Color::White),
            _ => (" Input ", Color::White),
        };

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color));

        // Create text with cursor
        let mut spans = vec![];

        // Content before cursor
        if self.cursor > 0 {
            spans.push(Span::raw(&self.content[..self.cursor]));
        }

        // Cursor
        let cursor_char = if self.cursor < self.content.len() {
            &self.content[self.cursor..self.cursor + 1]
        } else {
            " "
        };
        spans.push(Span::styled(
            cursor_char,
            Style::default()
                .bg(Color::White)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        ));

        // Content after cursor
        if self.cursor + 1 < self.content.len() {
            spans.push(Span::raw(&self.content[self.cursor + 1..]));
        }

        let text = Text::from(Line::from(spans));

        let paragraph = Paragraph::new(text).block(block).alignment(Alignment::Left);

        frame.render_widget(paragraph, area);
    }
}

impl Default for Input {
    fn default() -> Self {
        Self::new()
    }
}
