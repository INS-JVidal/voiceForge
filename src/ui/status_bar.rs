use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::AppState;

pub fn render(frame: &mut Frame, area: Rect, app: &AppState) {
    let line = if let Some(ref info) = app.file_info {
        // #13: Clamp to avoid truncation for very long audio (>71 min wraps u32).
        let total_secs = info.duration_secs.max(0.0);
        let mins = (total_secs / 60.0).min(u32::MAX as f64) as u32;
        let secs = (total_secs % 60.0) as u32;
        let ch_str = if info.channels == 1 { "Mono" } else { "Stereo" };
        let mut spans = vec![
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
        ];
        if let Some(ref status) = app.processing_status {
            spans.push(Span::styled(" │ ", Style::default().fg(Color::DarkGray)));
            spans.push(Span::styled(status, Style::default().fg(Color::Yellow)));
        }
        if let Some(ref msg) = app.status_message {
            spans.push(Span::styled(" │ ", Style::default().fg(Color::DarkGray)));
            spans.push(Span::styled(msg, Style::default().fg(Color::Red)));
        }
        Line::from(spans)
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
