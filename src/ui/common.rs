use ratatui::style::{Color, Style};

pub const ACCENT: Color = Color::Rgb(38, 139, 210);
pub const BG_SELECTED: Color = Color::Rgb(238, 232, 213);

pub fn border_style() -> Style {
    Style::default().fg(ACCENT)
}
