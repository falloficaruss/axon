use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::tui::AppMode;
use crate::tui::theme;

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

    /// Move cursor to the beginning of the input
    pub fn move_cursor_home(&mut self) {
        self.cursor = 0;
    }

    /// Move cursor to the end of the input
    pub fn move_cursor_end(&mut self) {
        self.cursor = self.content.len();
    }

    /// Clear input
    pub fn clear(&mut self) {
        self.content.clear();
        self.cursor = 0;
        self.history_index = None;
        self.saved_input.clear();
    }

    /// Replace the current input content and move the cursor to the end
    pub fn set_content(&mut self, content: &str) {
        self.content = content.to_string();
        self.cursor = self.content.len();
        self.history_index = None;
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
    pub fn add_to_history(&mut self, content: &str) {
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
            AppMode::Command => (" COMMAND ", theme::accent_gold()),
            AppMode::Normal => (" INPUT ", theme::accent_cyan()),
            _ => (" INPUT ", theme::accent_cyan()),
        };

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_set(ratatui::symbols::border::ROUNDED)
            .border_style(Style::default().fg(border_color))
            .style(Style::default().bg(theme::panel_bg_alt()).fg(theme::text_primary()));

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
                .bg(theme::text_primary())
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        ));

        // Content after cursor
        if self.cursor + 1 < self.content.len() {
            spans.push(Span::raw(&self.content[self.cursor + 1..]));
        }

        let text = Text::from(Line::from(spans));

        let paragraph = Paragraph::new(text)
            .block(block)
            .style(Style::default().bg(theme::panel_bg_alt()).fg(theme::text_primary()))
            .alignment(Alignment::Left);

        frame.render_widget(paragraph, area);
    }
}

impl Default for Input {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_input_new() {
        let input = Input::new();
        assert!(input.content.is_empty());
        assert_eq!(input.cursor, 0);
        assert!(input.history.is_empty());
        assert!(input.history_index.is_none());
        assert!(input.saved_input.is_empty());
    }

    #[test]
    fn test_input_default() {
        let input = Input::default();
        assert!(input.content.is_empty());
        assert_eq!(input.cursor, 0);
    }

    #[test]
    fn test_input_get_content() {
        let mut input = Input::new();
        input.content = "Hello".to_string();
        assert_eq!(input.get_content(), "Hello");
    }

    #[test]
    fn test_input_is_empty() {
        let mut input = Input::new();
        assert!(input.is_empty());

        input.insert_char('a');
        assert!(!input.is_empty());
    }

    #[test]
    fn test_input_insert_char() {
        let mut input = Input::new();
        input.insert_char('H');
        input.insert_char('i');

        assert_eq!(input.content, "Hi");
        assert_eq!(input.cursor, 2);
    }

    #[test]
    fn test_input_insert_char_at_beginning() {
        let mut input = Input::new();
        input.insert_char('b');
        input.insert_char('a');
        input.cursor = 0;
        input.insert_char('X');

        assert_eq!(input.content, "Xba");
        assert_eq!(input.cursor, 1);
    }

    #[test]
    fn test_input_delete_char() {
        let mut input = Input::new();
        input.insert_char('H');
        input.insert_char('i');
        input.delete_char();

        assert_eq!(input.content, "H");
        assert_eq!(input.cursor, 1);
    }

    #[test]
    fn test_input_delete_char_at_beginning() {
        let mut input = Input::new();
        input.insert_char('H');
        input.insert_char('i');
        input.cursor = 0;
        input.delete_char(); // Should do nothing

        assert_eq!(input.content, "Hi");
        assert_eq!(input.cursor, 0);
    }

    #[test]
    fn test_input_move_cursor_left() {
        let mut input = Input::new();
        input.insert_char('H');
        input.insert_char('i');
        input.move_cursor_left();

        assert_eq!(input.cursor, 1);

        input.move_cursor_left();
        assert_eq!(input.cursor, 0);

        input.move_cursor_left(); // Should not go negative
        assert_eq!(input.cursor, 0);
    }

    #[test]
    fn test_input_move_cursor_right() {
        let mut input = Input::new();
        input.insert_char('H');
        input.insert_char('i');
        input.cursor = 0;

        input.move_cursor_right();
        assert_eq!(input.cursor, 1);

        input.move_cursor_right();
        assert_eq!(input.cursor, 2);

        input.move_cursor_right(); // Should not exceed length
        assert_eq!(input.cursor, 2);
    }

    #[test]
    fn test_input_move_cursor_home() {
        let mut input = Input::new();
        input.insert_char('H');
        input.insert_char('i');
        input.move_cursor_left();

        input.move_cursor_home();

        assert_eq!(input.cursor, 0);
    }

    #[test]
    fn test_input_move_cursor_end() {
        let mut input = Input::new();
        input.insert_char('H');
        input.insert_char('i');
        input.cursor = 0;

        input.move_cursor_end();

        assert_eq!(input.cursor, 2);
    }

    #[test]
    fn test_input_clear() {
        let mut input = Input::new();
        input.insert_char('H');
        input.insert_char('i');
        input.history.push("test".to_string());
        input.history_index = Some(0);
        input.saved_input = "saved".to_string();

        input.clear();

        assert!(input.content.is_empty());
        assert_eq!(input.cursor, 0);
        assert!(input.history_index.is_none());
        assert!(input.saved_input.is_empty());
        assert_eq!(input.history.len(), 1); // History is preserved
    }

    #[test]
    fn test_input_previous_history_empty() {
        let mut input = Input::new();
        input.previous_history(); // Should do nothing

        assert!(input.content.is_empty());
        assert!(input.history_index.is_none());
    }

    #[test]
    fn test_input_previous_history() {
        let mut input = Input::new();
        input.history.push("first".to_string());
        input.history.push("second".to_string());

        input.previous_history();

        assert_eq!(input.content, "second");
        assert_eq!(input.history_index, Some(1));
    }

    #[test]
    fn test_input_next_history() {
        let mut input = Input::new();
        input.history.push("first".to_string());
        input.history.push("second".to_string());
        input.history_index = Some(0);
        input.content = "temp".to_string();

        input.next_history();

        assert_eq!(input.content, "second");
        assert_eq!(input.history_index, Some(1));
    }

    #[test]
    fn test_input_next_history_restore() {
        let mut input = Input::new();
        input.history.push("first".to_string());
        input.history_index = Some(0);
        input.saved_input = "original".to_string();
        input.content = "from history".to_string();

        input.next_history();

        assert_eq!(input.content, "original");
        assert!(input.history_index.is_none());
    }

    #[test]
    fn test_input_add_to_history_empty() {
        let mut input = Input::new();
        input.add_to_history("");
        assert!(input.history.is_empty());
    }

    #[test]
    fn test_input_add_to_history_no_duplicates() {
        let mut input = Input::new();
        input.history.push("test".to_string());
        input.add_to_history("test");
        input.add_to_history("test");

        assert_eq!(input.history.len(), 1);
    }

    #[test]
    fn test_input_add_to_history() {
        let mut input = Input::new();
        input.add_to_history("first");
        input.add_to_history("second");

        assert_eq!(input.history.len(), 2);
        assert_eq!(input.history[0], "first");
        assert_eq!(input.history[1], "second");
    }

    #[test]
    fn test_input_autocomplete() {
        let mut input = Input::new();
        input.autocomplete();
        assert_eq!(input.content, "\t");
        assert_eq!(input.cursor, 1);
    }
}
