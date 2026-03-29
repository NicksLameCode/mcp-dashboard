use ratatui::style::{Color, Style};

pub const ACCENT: Color = Color::Rgb(38, 139, 210);
pub const BG_SELECTED: Color = Color::Rgb(238, 232, 213);
pub const CHAT_USER: Color = Color::Cyan;
pub const CHAT_ASSISTANT: Color = Color::Green;
pub const CHAT_TOOL_CALL: Color = Color::Yellow;
pub const CHAT_TOOL_RESULT: Color = Color::DarkGray;

pub fn border_style() -> Style {
    Style::default().fg(ACCENT)
}
