use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

pub fn render(frame: &mut Frame) {
    let area = centered_rect(70, 20, frame.area());

    frame.render_widget(Clear, area);

    let block = Block::default()
        .title(" Keybindings ")
        .borders(Borders::ALL)
        .border_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let key_style = Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD);
    let desc_style = Style::default().fg(Color::White);
    let sep_style = Style::default().fg(Color::DarkGray);

    let bindings: &[(&str, &str)] = &[
        ("Space", "Play / Pause"),
        ("Tab", "Cycle panel focus"),
        ("\u{2191}/\u{2193}", "Select slider / Boost-cut band"),
        ("\u{2190}/\u{2192}", "Adjust slider / Navigate bands"),
        ("Shift+\u{2190}/\u{2192}", "Fine-adjust slider / Fine-adjust band"),
        ("d", "Reset slider / Reset band to 0dB"),
        ("[ / ]", "Seek \u{00b1}5s"),
        ("Home / End", "Jump to start / end"),
        ("r", "Toggle loop"),
        ("w", "Toggle WORLD bypass (ON/OFF)"),
        ("a", "A/B toggle (original vs processed)"),
        ("s", "Export WAV"),
        ("o", "Open file"),
        ("?", "This help"),
        ("q / Esc", "Quit"),
    ];

    let mut lines: Vec<Line> = Vec::with_capacity(bindings.len() + 2);
    for &(key, desc) in bindings {
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(format!("{key:>17}"), key_style),
            Span::styled("  \u{2502}  ", sep_style),
            Span::styled(desc, desc_style),
        ]));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  Press any key to close",
        Style::default().fg(Color::DarkGray),
    )));

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
