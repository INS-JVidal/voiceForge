use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::AppState;

const BLOCKS: [char; 9] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

pub fn render(frame: &mut Frame, area: Rect, app: &AppState) {
    let block = Block::default()
        .title(" Spectrum ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if app.spectrum_bins.is_empty() || inner.width < 2 || inner.height < 1 {
        let placeholder = Paragraph::new("  No audio playing")
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(placeholder, inner);
        return;
    }

    let num_bars = inner.width as usize;
    let inner_h = inner.height as usize;
    let bin_count = app.spectrum_bins.len();

    // Precompute bar heights via log-frequency mapping
    let mut heights = Vec::with_capacity(num_bars);
    for i in 0..num_bars {
        let t = if num_bars > 1 {
            i as f32 / (num_bars - 1) as f32
        } else {
            0.0
        };
        let bin = ((bin_count as f32).powf(t) as usize).min(bin_count - 1);
        let db = app.spectrum_bins[bin];
        let h = ((db + 80.0) / 80.0 * inner_h as f32).clamp(0.0, inner_h as f32);
        heights.push(h);
    }

    // Build lines top-to-bottom
    let mut lines = Vec::with_capacity(inner_h);
    for r in 0..inner_h {
        let level = inner_h - r; // 1 = bottom row, inner_h = top row
        let row_ratio = level as f32 / inner_h as f32;
        let mut spans = Vec::with_capacity(num_bars);
        for &h in &heights {
            let full = h as usize;
            let frac = h - full as f32;
            let ch = if level <= full {
                '█'
            } else if level == full + 1 && frac > 0.0 {
                let idx = (frac * 8.0).round() as usize;
                BLOCKS[idx.min(8)]
            } else {
                ' '
            };
            // Color by row position: green at bottom, yellow in middle, red at top
            let color = if row_ratio >= 0.75 {
                Color::Red
            } else if row_ratio >= 0.5 {
                Color::Yellow
            } else {
                Color::Green
            };
            spans.push(Span::styled(
                ch.to_string(),
                Style::default().fg(color),
            ));
        }
        lines.push(Line::from(spans));
    }

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, inner);
}
