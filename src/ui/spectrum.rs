use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;
use image::{RgbaImage, Rgba};

use crate::app::AppState;

const BLOCKS: [char; 9] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

/// Render the spectrum analyzer with GPU pixel support or Unicode fallback.
pub fn render(frame: &mut Frame, area: Rect, app: &mut AppState) {
    let block = Block::default()
        .title(" Spectrum ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Try GPU pixel path if picker is available
    if let Some(ref mut state) = app.spectrum_state {
        let widget = ratatui_image::StatefulImage::new(None);
        frame.render_stateful_widget(widget, inner, state);
    } else {
        // Fallback to Unicode rendering
        render_unicode_fallback(frame, inner, app);
    }
}

/// Fallback Unicode/Braille renderer for terminals without graphics protocol support.
fn render_unicode_fallback(frame: &mut Frame, area: Rect, app: &AppState) {
    if app.spectrum_bins.is_empty() || area.width < 2 || area.height < 1 {
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
    frame.render_widget(paragraph, area);
}

/// Render spectrum to an RGB image with smooth gradient coloring.
/// Each pixel column represents one logarithmic frequency bin.
/// Vertical height represents amplitude; colors interpolate from violet → purple → pink.
pub fn spectrum_to_image(bins: &[f32], width: u32, height: u32) -> RgbaImage {
    let mut img = RgbaImage::from_pixel(width, height, Rgba([0, 0, 0, 255]));

    let num_bars = width as usize;
    let bin_count = bins.len();
    if num_bars == 0 || bin_count == 0 || height == 0 {
        return img;
    }

    for col in 0..num_bars {
        // Log-frequency mapping: same as current render
        let t = col as f64 / (num_bars - 1).max(1) as f64;
        // L-7: Use quadratic log-frequency mapping that includes bin 0.
        let bin = ((bin_count as f64 - 1.0) * t.powf(2.0)).round().min((bin_count - 1) as f64) as usize;

        // Amplitude fraction [0.0, 1.0] from dB value in [-80, 0]
        let db = bins[bin].clamp(-80.0, 0.0);
        let amp = (db + 80.0) / 80.0; // 0.0 = silence, 1.0 = peak

        let filled_px = (amp as f64 * height as f64).round() as u32;

        for row in 0..filled_px {
            // frac: 0.0 at bottom, 1.0 at top of filled portion
            let frac = row as f64 / height as f64;
            let color = punk_color(frac);
            let y = height - 1 - row; // render bottom-up
            img.put_pixel(col as u32, y, color);
        }
    }
    img
}

/// Interpolate color through the punk gradient:
/// 0.0 (bottom) → #3D0066 deep violet
/// 0.5 (mid) → #CC00FF electric purple
/// 1.0 (top) → #FF0099 neon pink
fn punk_color(frac: f64) -> Rgba<u8> {
    let (r, g, b) = if frac < 0.5 {
        let t = frac * 2.0;
        // deep violet #3D0066 → electric purple #CC00FF
        (lerp(0x3D, 0xCC, t), lerp(0x00, 0x00, t), lerp(0x66, 0xFF, t))
    } else {
        let t = (frac - 0.5) * 2.0;
        // electric purple #CC00FF → neon pink #FF0099
        (lerp(0xCC, 0xFF, t), lerp(0x00, 0x00, t), lerp(0xFF, 0x99, t))
    };
    Rgba([r, g, b, 255])
}

/// Linear interpolation between two u8 values.
fn lerp(a: u8, b: u8, t: f64) -> u8 {
    (a as f64 + (b as f64 - a as f64) * t).round() as u8
}
