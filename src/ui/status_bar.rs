use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::AppState;

pub fn render(frame: &mut Frame, area: Rect, app: &AppState) {
    let line = if let Some(ref info) = app.file_info {
        let mins = (info.duration_secs / 60.0) as u32;
        let secs = (info.duration_secs % 60.0) as u32;
        let ch_str = if info.channels == 1 { "Mono" } else { "Stereo" };
        Line::from(vec![
            Span::styled(" File: ", Style::default().fg(Color::DarkGray)),
            Span::styled(&info.name, Style::default().fg(Color::White)),
            Span::styled(" │ ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{} Hz", info.sample_rate),
                Style::default().fg(Color::White),
            ),
            Span::styled(" │ ", Style::default().fg(Color::DarkGray)),
            Span::styled(ch_str, Style::default().fg(Color::White)),
            Span::styled(" │ ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{mins}:{secs:02}"),
                Style::default().fg(Color::White),
            ),
        ])
    } else if let Some(ref msg) = app.status_message {
        Line::from(Span::styled(
            format!(" {msg}"),
            Style::default().fg(Color::Red),
        ))
    } else {
        Line::from(Span::styled(
            " No file loaded — press 'o' to open",
            Style::default().fg(Color::DarkGray),
        ))
    };

    let paragraph = Paragraph::new(line);
    frame.render_widget(paragraph, area);
}
