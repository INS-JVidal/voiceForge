use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;
use std::sync::atomic::Ordering;

use crate::app::{AppState, PanelFocus};

pub fn render(frame: &mut Frame, area: Rect, app: &AppState) {
    let focused = app.focus == PanelFocus::Transport;
    let border_color = if focused { Color::Cyan } else { Color::White };

    let block = Block::default()
        .title(" Transport ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.width < 10 {
        return;
    }

    let playing = app.playback.playing.load(Ordering::Relaxed);
    let play_icon = if playing { "▶ Playing" } else { "⏸ Paused " };

    let loop_str = if app.loop_enabled { "On" } else { "Off" };

    let ab_str = if app.ab_original { "A" } else { "B" };

    // Time display
    let (current_time, duration) = if let Some(ref info) = app.file_info {
        let current = app
            .playback
            .current_time_secs(info.sample_rate, info.channels);
        (current, info.duration_secs)
    } else {
        (0.0, 0.0)
    };

    let cur_min = (current_time / 60.0) as u32;
    let cur_sec = (current_time % 60.0) as u32;
    let dur_min = (duration / 60.0) as u32;
    let dur_sec = (duration % 60.0) as u32;

    // Seek bar
    let time_str = format!(" {cur_min}:{cur_sec:02}/{dur_min}:{dur_sec:02} ");
    let loop_str = format!("  [Loop: {loop_str}]  ");
    let ab_display = format!(" [{ab_str}]");
    let bar_budget = (inner.width as usize)
        .saturating_sub(play_icon.len() + loop_str.len() + time_str.len() + ab_display.len());

    let fraction = if duration > 0.0 {
        (current_time / duration).clamp(0.0, 1.0)
    } else {
        0.0
    };

    let filled = ((fraction * bar_budget as f64).round() as usize).min(bar_budget);
    let empty = bar_budget.saturating_sub(filled);

    let line = Line::from(vec![
        Span::styled(
            play_icon,
            Style::default()
                .fg(if playing { Color::Green } else { Color::Yellow })
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(loop_str, Style::default().fg(Color::White)),
        Span::styled("─".repeat(filled), Style::default().fg(Color::Cyan)),
        Span::styled("●", Style::default().fg(Color::White)),
        Span::styled("─".repeat(empty), Style::default().fg(Color::DarkGray)),
        Span::styled(time_str, Style::default().fg(Color::White)),
        Span::styled(ab_display, Style::default().fg(Color::Magenta)),
    ]);

    let paragraph = Paragraph::new(line);
    frame.render_widget(paragraph, inner);
}
