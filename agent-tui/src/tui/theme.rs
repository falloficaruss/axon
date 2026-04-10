use ratatui::{
    style::{Color, Modifier, Style},
    symbols::border,
    widgets::Block,
};

pub fn app_bg() -> Color {
    Color::Rgb(12, 18, 28)
}

pub fn panel_bg() -> Color {
    Color::Rgb(22, 30, 44)
}

pub fn panel_bg_alt() -> Color {
    Color::Rgb(28, 38, 54)
}

pub fn border_soft() -> Color {
    Color::Rgb(82, 108, 132)
}

pub fn accent_cyan() -> Color {
    Color::Rgb(92, 225, 230)
}

pub fn accent_mint() -> Color {
    Color::Rgb(111, 231, 183)
}

pub fn accent_gold() -> Color {
    Color::Rgb(255, 193, 94)
}

pub fn accent_coral() -> Color {
    Color::Rgb(255, 107, 107)
}

pub fn text_primary() -> Color {
    Color::Rgb(224, 229, 236)
}

pub fn text_muted() -> Color {
    Color::Rgb(137, 149, 168)
}

pub fn text_subtle() -> Color {
    Color::Rgb(94, 106, 130)
}

pub fn status_bg() -> Color {
    Color::Rgb(17, 27, 40)
}

pub fn glass_panel<'a>(title: &'a str, accent: Color) -> Block<'a> {
    Block::default()
        .title(title)
        .borders(ratatui::widgets::Borders::ALL)
        .border_set(border::ROUNDED)
        .border_style(Style::default().fg(accent))
        .style(Style::default().bg(panel_bg()).fg(text_primary()))
}

pub fn glass_popup<'a>(title: &'a str, accent: Color) -> Block<'a> {
    Block::default()
        .title(title)
        .borders(ratatui::widgets::Borders::ALL)
        .border_set(border::ROUNDED)
        .border_style(Style::default().fg(accent).add_modifier(Modifier::BOLD))
        .style(Style::default().bg(panel_bg_alt()).fg(text_primary()))
}

