use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::AppState;

pub fn render(frame: &mut Frame, app: &AppState) {
    let total = app.file_picker_matches.len();
    let n_visible = total.min(5);
    let popup_h: u16 = if n_visible == 0 { 4 } else { (5 + n_visible) as u16 };

    let area = centered_rect(60, popup_h, frame.area());

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

    // Split inner area vertically: hint (1) + input (1) + match area (remaining)
    let hint_area = Rect {
        x: inner.x,
        y: inner.y,
        width: inner.width,
        height: 1,
    };
    let input_area = Rect {
        x: inner.x,
        y: inner.y + 1,
        width: inner.width,
        height: 1,
    };
    let match_area = Rect {
        x: inner.x,
        y: inner.y + 2,
        width: inner.width,
        height: inner.height.saturating_sub(2),
    };

    // Render hint line
    let hint_line = Line::from(Span::styled(
        " ↑↓ select   Tab complete   Esc cancel ",
        Style::default().fg(Color::DarkGray),
    ));
    frame.render_widget(Paragraph::new(hint_line), hint_area);

    // Render input line with cursor and horizontal scrolling
    let (before, after) = render_input_line(app, input_area.width as usize);
    let input_line = Line::from(vec![
        Span::styled(" > ", Style::default().fg(Color::Yellow)),
        Span::styled(before, Style::default().fg(Color::White)),
        Span::styled("█", Style::default().fg(Color::Cyan)),
        Span::styled(after, Style::default().fg(Color::White)),
    ]);
    frame.render_widget(Paragraph::new(input_line), input_area);

    // Render match area
    if n_visible > 0 {
        let mut match_lines = Vec::new();

        // Divider with scroll indicator
        let scroll = app.file_picker_scroll;
        let above = scroll;
        let below = total.saturating_sub(scroll + n_visible);
        let width = match_area.width as usize;

        let divider_text = match (above > 0, below > 0) {
            (false, false) => "─".repeat(width),
            (true, false) => {
                let indicator = format!("─ ↑{} ─", above);
                let padding = width.saturating_sub(indicator.len());
                format!("{}{}", indicator, "─".repeat(padding))
            }
            (false, true) => {
                let indicator = format!("─ ↓{} ─", below);
                let padding = width.saturating_sub(indicator.len());
                format!("{}{}", indicator, "─".repeat(padding))
            }
            (true, true) => {
                let indicator = format!("─ ↑{} ↓{} ─", above, below);
                let padding = width.saturating_sub(indicator.len());
                format!("{}{}", indicator, "─".repeat(padding))
            }
        };

        match_lines.push(Line::from(Span::styled(
            divider_text,
            Style::default().fg(Color::DarkGray),
        )));

        // Match items (windowed slice)
        let window_end = (scroll + n_visible).min(total);
        for abs_idx in scroll..window_end {
            let match_path = &app.file_picker_matches[abs_idx];
            let is_selected = app.file_picker_selected == Some(abs_idx);
            let is_dir = match_path.ends_with('/');

            let prefix = if is_selected { "▶ " } else { "  " };
            let prefix_style = if is_selected {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            let path_style = if is_selected {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else if is_dir {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default().fg(Color::White)
            };

            // Truncate path if it's too long
            let max_path_len = width.saturating_sub(3);
            let display_path = if match_path.len() > max_path_len {
                format!("{}…", &match_path[..max_path_len - 1])
            } else {
                match_path.to_string()
            };

            let line = Line::from(vec![
                Span::styled(prefix, prefix_style),
                Span::styled(display_path, path_style),
            ]);
            match_lines.push(line);
        }

        let matches_para = Paragraph::new(match_lines);
        frame.render_widget(matches_para, match_area);
    } else if !app.file_picker_input.is_empty() {
        // Show "no matches" message if user typed something but got no results
        let no_matches_line = Line::from(Span::styled(
            "  no matches",
            Style::default().fg(Color::DarkGray),
        ));
        frame.render_widget(Paragraph::new(no_matches_line), match_area);
    }
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
