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

    // Reserve bottom row for frequency labels if space allows (inner_h >= 2).
    // The spectrum scale is quadratic (t²), which approximates log scale perceptually
    // but is not true log scale. Frequency labels make the scale explicit.
    let has_label_row = inner_h >= 2;
    let bar_height = if has_label_row {
        inner_h.saturating_sub(1) as f32
    } else {
        inner_h as f32
    };

    // Precompute bar heights via quadratic-frequency mapping
    let mut heights = Vec::with_capacity(num_bars);
    for i in 0..num_bars {
        let t = if num_bars > 1 {
            i as f32 / (num_bars - 1) as f32
        } else {
            0.0
        };
        // L-7: Map log-frequency starting from bin 0 (DC) not bin 1.
        // Use (bin_count - 1) * t² to include the full range [0, bin_count-1].
        let bin = ((bin_count as f32 - 1.0) * t.powf(2.0)).round() as usize;
        let db = app.spectrum_bins[bin].clamp(-80.0, 0.0);
        let h = ((db + 80.0) / 80.0 * bar_height).clamp(0.0, bar_height);
        heights.push(h);
    }

    // Build lines top-to-bottom (bars only; label row added later)
    let bar_rows = if has_label_row {
        inner_h - 1
    } else {
        inner_h
    };
    let mut lines = Vec::with_capacity(inner_h);
    for r in 0..bar_rows {
        let level = bar_rows - r; // 1 = bottom bar row, bar_rows = top bar row
        let row_ratio = level as f32 / inner_h as f32; // Use full inner_h for gradient consistency
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

    // Add frequency label row if space was reserved
    if has_label_row {
        let sample_rate = app
            .file_info
            .as_ref()
            .map(|f| f.sample_rate)
            .unwrap_or(44100) as f32;
        let fft_size = 2048.0;

        // Frequency labels: (freq_hz, short_label)
        let freq_labels = [
            (100.0, "100"),
            (500.0, "500"),
            (1000.0, "1k"),
            (5000.0, "5k"),
            (10000.0, "10k"),
            (20000.0, "20k"),
        ];

        // Build label row: compute positions and prevent overlap
        let mut label_row = vec![' '; num_bars];
        let mut last_col = 0;
        let label_color = Color::Rgb(120, 120, 120); // Muted gray

        for &(freq_hz, label_text) in &freq_labels {
            // Inverse quadratic map: bin → col
            let bin = (freq_hz * fft_size / sample_rate).clamp(0.0, (bin_count - 1) as f32);
            let t = (bin / (bin_count as f32 - 1.0)).sqrt();
            let col = (t * (num_bars as f32 - 1.0)).round() as usize;

            // Skip if label overflows or overlaps with previous label
            if col + label_text.len() <= num_bars && col >= last_col {
                // Write label at this position
                for (offset, ch) in label_text.chars().enumerate() {
                    if col + offset < num_bars {
                        label_row[col + offset] = ch;
                    }
                }
                last_col = col + label_text.len();
            }
        }

        // Convert label row to Line with gray color
        let label_spans: Vec<Span> = label_row
            .iter()
            .map(|&ch| Span::styled(ch.to_string(), Style::default().fg(label_color)))
            .collect();
        lines.push(Line::from(label_spans));
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
