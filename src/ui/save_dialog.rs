use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::AppState;
use crate::ui::file_picker::render_input_line;

pub fn render(frame: &mut Frame, app: &AppState) {
    let area = centered_rect(60, 5, frame.area());

    frame.render_widget(Clear, area);

    let block = Block::default()
        .title(" Save WAV ")
        .borders(Borders::ALL)
        .border_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // L-10/L-11: Reuse shared input rendering with cursor and scrolling.
    let (before, after) = render_input_line(app, inner.width as usize);

    let lines = vec![
        Line::from(Span::styled(
            " Enter output path (Esc to cancel):",
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled(" > ", Style::default().fg(Color::Cyan)),
            Span::styled(before, Style::default().fg(Color::White)),
            Span::styled("█", Style::default().fg(Color::Cyan)),
            Span::styled(after, Style::default().fg(Color::White)),
        ]),
    ];

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, inner);
}

fn centered_rect(percent_x: u16, height: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([Constraint::Length(height)]).flex(Flex::Center);
    let horizontal = Layout::horizontal([Constraint::Percentage(percent_x)]).flex(Flex::Center);
    let [v] = vertical.areas(area);
    let [h] = horizontal.areas(v);
    h
}
