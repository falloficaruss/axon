//! Markdown rendering module
//!
//! This module provides Markdown parsing and rendering for the TUI.

use pulldown_cmark::{Event, Parser, Tag, CodeBlockKind, TagEnd};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

/// Parse markdown content into styled lines
pub fn parse_markdown(content: &str) -> Vec<Line<'_>> {
    let mut lines: Vec<Line> = Vec::new();
    let mut current_line: Vec<Span> = Vec::new();
    let mut in_code_block = false;
    let mut _in_list = false;
    let mut list_indent = 0;

    let parser = Parser::new(content);

    for event in parser {
        match event {
            Event::Start(tag) => match tag {
                Tag::Paragraph => {
                    if !current_line.is_empty() {
                        lines.push(Line::from(current_line.clone()));
                        current_line.clear();
                    }
                }
                Tag::Heading { level, .. } => {
                    if !current_line.is_empty() {
                        lines.push(Line::from(current_line.clone()));
                        current_line.clear();
                    }
                    // Add heading marker
                    let style = Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD);
                    let prefix = "#".repeat(level as usize);
                    current_line.push(Span::styled(format!("{} ", prefix), style));
                }
                Tag::CodeBlock(kind) => {
                    in_code_block = true;
                    if let CodeBlockKind::Fenced(lang) = kind {
                        // Add language indicator
                        if !current_line.is_empty() {
                            lines.push(Line::from(current_line.clone()));
                            current_line.clear();
                        }
                        let style = Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD);
                        current_line.push(Span::styled(format!("┌─[{}]", lang), style));
                        lines.push(Line::from(current_line.clone()));
                        current_line.clear();
                    } else {
                        // Indented code block
                        if !current_line.is_empty() {
                            lines.push(Line::from(current_line.clone()));
                            current_line.clear();
                        }
                        let style = Style::default().fg(Color::Yellow);
                        current_line.push(Span::styled("┌─[code]", style));
                        lines.push(Line::from(current_line.clone()));
                        current_line.clear();
                    }
                }
                Tag::Emphasis => {
                    // Italics - handled in text
                }
                Tag::Strong => {
                    // Bold - handled in text
                }
                Tag::Strikethrough => {
                    // Strikethrough - not well supported in terminal
                }
                Tag::Link { .. } => {
                    // Links - just show as text
                }
                Tag::Image { title, .. } => {
                    // Images - show alt text
                    if !title.is_empty() {
                        current_line.push(Span::raw(format!("[Image: {}]", title)));
                    }
                }
                Tag::List(_) => {
                    _in_list = true;
                    list_indent += 2;
                }
                Tag::Item => {
                    // Add bullet point
                    let indent = " ".repeat(list_indent);
                    current_line.push(Span::raw(format!("{}• ", indent)));
                }
                Tag::BlockQuote(_) => {
                    if !current_line.is_empty() {
                        lines.push(Line::from(current_line.clone()));
                        current_line.clear();
                    }
                    let style = Style::default().fg(Color::DarkGray);
                    current_line.push(Span::styled("│ ", style));
                }
                Tag::Table(_) | Tag::TableHead | Tag::TableRow | Tag::TableCell => {
                    // Basic table support - just add spacing
                    if !current_line.is_empty() {
                        lines.push(Line::from(current_line.clone()));
                        current_line.clear();
                    }
                }
                _ => {}
            },
            Event::End(tag_end) => match tag_end {
                TagEnd::Paragraph => {
                    if !current_line.is_empty() {
                        lines.push(Line::from(current_line.clone()));
                        current_line.clear();
                    }
                    lines.push(Line::from("")); // Empty line between paragraphs
                }
                TagEnd::Heading { .. } => {
                    if !current_line.is_empty() {
                        lines.push(Line::from(current_line.clone()));
                        current_line.clear();
                    }
                    lines.push(Line::from(""));
                }
                TagEnd::CodeBlock => {
                    if !current_line.is_empty() {
                        lines.push(Line::from(current_line.clone()));
                        current_line.clear();
                    }
                    // Add code block end
                    let style = Style::default().fg(Color::Yellow);
                    lines.push(Line::from(Span::styled("└────────", style)));
                    lines.push(Line::from(""));
                    in_code_block = false;
                }
                TagEnd::List(_) => {
                    _in_list = false;
                    list_indent = list_indent.saturating_sub(2);
                    if !current_line.is_empty() {
                        lines.push(Line::from(current_line.clone()));
                        current_line.clear();
                    }
                }
                TagEnd::Item => {
                    if !current_line.is_empty() {
                        lines.push(Line::from(current_line.clone()));
                        current_line.clear();
                    }
                }
                TagEnd::BlockQuote(_) => {
                    if !current_line.is_empty() {
                        lines.push(Line::from(current_line.clone()));
                        current_line.clear();
                    }
                }
                TagEnd::Table => {
                    if !current_line.is_empty() {
                        lines.push(Line::from(current_line.clone()));
                        current_line.clear();
                    }
                }
                _ => {}
            },
            Event::Text(text) => {
                if in_code_block {
                    // Code block text - use monospace style
                    let style = Style::default().fg(Color::Green);
                    for line in text.lines() {
                        if !current_line.is_empty() {
                            lines.push(Line::from(current_line.clone()));
                            current_line.clear();
                        }
                        current_line.push(Span::styled(format!("│ {}", line), style));
                        lines.push(Line::from(current_line.clone()));
                        current_line.clear();
                    }
                } else {
                    current_line.push(Span::raw(text.to_string()));
                }
            }
            Event::Code(text) => {
                // Inline code
                let style = Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD);
                current_line.push(Span::styled(format!("`{}`", text), style));
            }
            Event::Html(_) => {
                // Skip HTML tags
            }
            Event::SoftBreak | Event::HardBreak => {
                if !current_line.is_empty() {
                    lines.push(Line::from(current_line.clone()));
                    current_line.clear();
                }
            }
            Event::Rule => {
                if !current_line.is_empty() {
                    lines.push(Line::from(current_line.clone()));
                    current_line.clear();
                }
                lines.push(Line::from(Span::styled(
                    "────────────────────────────────────────",
                    Style::default().fg(Color::DarkGray),
                )));
            }
            Event::FootnoteReference(_) | Event::TaskListMarker(_) |
            Event::InlineMath(_) | Event::DisplayMath(_) | Event::InlineHtml(_) => {
                // Skip footnotes, task markers, math, and inline HTML for now
            }
        }
    }

    // Add any remaining content
    if !current_line.is_empty() {
        lines.push(Line::from(current_line));
    }

    // Remove trailing empty lines
    while lines.last().map(|l| l.spans.is_empty()).unwrap_or(false) {
        lines.pop();
    }

    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_plain_text() {
        let lines = parse_markdown("Hello, world!");
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_parse_code_block() {
        let lines = parse_markdown("```\ncode\n```");
        assert!(lines.iter().any(|l| l.to_string().contains("code")));
    }
}
