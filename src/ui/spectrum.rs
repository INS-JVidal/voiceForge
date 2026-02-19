use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::AppState;

const BLOCKS: [char; 9] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

/// Render the spectrum analyzer using Unicode colored blocks.
pub fn render(frame: &mut Frame, area: Rect, app: &AppState) {
    let block = Block::default()
        .title(" Spectrum ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    render_unicode_fallback(frame, inner, app);
}

/// Fallback Unicode/Braille renderer for terminals without graphics protocol support.
fn render_unicode_fallback(frame: &mut Frame, area: Rect, app: &AppState) {
    // DEBUG: Log why fallback is happening (only for error cases)
    if app.spectrum_bins.is_empty() {
        let placeholder = Paragraph::new("  No audio playing")
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(placeholder, area);
        return;
    }
    if area.width < 2 || area.height < 1 {
        let placeholder = Paragraph::new("  No audio playing")
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(placeholder, area);
        return;
    }

    let num_bars = area.width as usize;
    let inner_h = area.height as usize;
    let bin_count = app.spectrum_bins.len();

    // Precompute bar heights via log-frequency mapping
    let mut heights = Vec::with_capacity(num_bars);
    for i in 0..num_bars {
        let t = if num_bars > 1 {
            i as f32 / (num_bars - 1) as f32
        } else {
            0.0
        };
        // L-7: Map log-frequency starting from bin 0 (DC) not bin 1.
        // Use (bin_count - 1) * powf(t) to include the full range [0, bin_count-1].
        let bin = ((bin_count as f32 - 1.0) * t.powf(2.0)).round() as usize;
        let db = app.spectrum_bins[bin].clamp(-80.0, 0.0);
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
            // True color punk gradient: smooth interpolation
            // 0.0 (bottom) → #3D0066 deep violet
            // 0.5 (mid) → #CC00FF electric purple
            // 1.0 (top) → #FF0099 neon pink
            let color = punk_gradient_color(row_ratio);
            spans.push(Span::styled(
                ch.to_string(),
                Style::default().fg(color),
            ));
        }
        lines.push(Line::from(spans));
    }

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, area);
}

/// Interpolate true color through the punk gradient:
/// 0.0 (bottom) → #3D0066 deep violet
/// 0.5 (mid) → #CC00FF electric purple
/// 1.0 (top) → #FF0099 neon pink
fn punk_gradient_color(frac: f32) -> Color {
    let frac = frac.clamp(0.0, 1.0);
    let (r, g, b) = if frac < 0.5 {
        let t = frac * 2.0;
        // deep violet #3D0066 → electric purple #CC00FF
        (
            lerp(0x3D, 0xCC, t as f64),
            lerp(0x00, 0x00, t as f64),
            lerp(0x66, 0xFF, t as f64),
        )
    } else {
        let t = (frac - 0.5) * 2.0;
        // electric purple #CC00FF → neon pink #FF0099
        (
            lerp(0xCC, 0xFF, t as f64),
            lerp(0x00, 0x00, t as f64),
            lerp(0xFF, 0x99, t as f64),
        )
    };
    Color::Rgb(r, g, b)
}

/// Linear interpolation between two u8 values.
fn lerp(a: u8, b: u8, t: f64) -> u8 {
    (a as f64 + (b as f64 - a as f64) * t).round() as u8
}
