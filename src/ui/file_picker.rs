use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::AppState;

pub fn render(frame: &mut Frame, app: &AppState) {
    let area = centered_rect(60, 5, frame.area());

    // Clear background behind popup
    frame.render_widget(Clear, area);

    let block = Block::default()
        .title(" Open File ")
        .borders(Borders::ALL)
        .border_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // L-10/L-11: Render input with cursor and horizontal scrolling.
    let (before, after) = render_input_line(app, inner.width as usize);

    let lines = vec![
        Line::from(Span::styled(
            " Enter file path (Esc to cancel):",
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled(" > ", Style::default().fg(Color::Yellow)),
            Span::styled(before, Style::default().fg(Color::White)),
            Span::styled("█", Style::default().fg(Color::Cyan)),
            Span::styled(after, Style::default().fg(Color::White)),
        ]),
    ];

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, inner);
}

/// Create a centered rect of `percent_x`% width and `height` rows.
fn centered_rect(percent_x: u16, height: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([Constraint::Length(height)]).flex(Flex::Center);
    let horizontal = Layout::horizontal([Constraint::Percentage(percent_x)]).flex(Flex::Center);
    let [v] = vertical.areas(area);
    let [h] = horizontal.areas(v);
    h
}

/// L-10/L-11: Split input text at cursor and apply horizontal scrolling.
/// Returns (before_cursor, after_cursor) strings truncated to fit the width.
pub fn render_input_line(app: &AppState, width: usize) -> (String, String) {
    let input = &app.file_picker_input;
    let cursor = app.input_cursor.min(input.len());
    let before = &input[..cursor];
    let after = &input[cursor..];

    // Available width for text: width minus the " > " prefix (3) and cursor block (1)
    let avail = width.saturating_sub(4);
    if avail == 0 {
        return (String::new(), String::new());
    }

    let before_chars: Vec<char> = before.chars().collect();
    let after_chars: Vec<char> = after.chars().collect();

    // Give most of the space to text before cursor, but keep some for after.
    let before_budget = avail.min(before_chars.len());
    let after_budget = avail.saturating_sub(before_budget).min(after_chars.len());

    let before_str: String = before_chars[before_chars.len() - before_budget..].iter().collect();
    let after_str: String = after_chars[..after_budget].iter().collect();

    (before_str, after_str)
}
