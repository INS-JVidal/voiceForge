use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::SliderDef;

/// Block characters progressing from thin to full: ▏ ▎ ▍ ▌ ▋ ▊ ▉ █
const BLOCK_CHARS: [&str; 8] = ["▏", "▎", "▍", "▌", "▋", "▊", "▉", "█"];

/// Maps position within total filled cells to a block character.
/// Distributes characters evenly across the filled portion.
fn block_char_for_pos(i: usize, total: usize) -> &'static str {
    if total <= 1 {
        return BLOCK_CHARS[7]; // single cell → full block
    }
    let idx = (i * 7 / (total - 1)).min(7);
    BLOCK_CHARS[idx]
}

/// Maps fractional remainder (0.0..1.0) to a partial block character.
fn partial_block_char(frac: f64) -> &'static str {
    match (frac * 8.0) as usize {
        1 => "▏",
        2 => "▎",
        3 => "▍",
        4 => "▌",
        5 => "▋",
        6 => "▊",
        7 => "▉",
        _ => "", // 0 → nothing; 8 → falls into the next full cell
    }
}

/// Computes gradient color across the slider from dark (left) to bright (right).
/// Interpolates linearly based on position `t` (0.0 at left, 1.0 at right).
fn gradient_color(t: f64, is_selected: bool, focused: bool, dimmed: bool) -> Color {
    if dimmed {
        let lerp = |a: u8, b: u8| (a as f64 + (b as f64 - a as f64) * t).round() as u8;
        return Color::Rgb(lerp(30, 110), lerp(30, 110), lerp(30, 110));
    }
    let (from, to) = if focused && is_selected {
        // Cyan gradient: dark cyan → bright cyan
        ((0u8, 60u8, 80u8), (0u8, 220u8, 255u8))
    } else if focused {
        // Blue gradient: dark blue → mid blue
        ((0u8, 25u8, 60u8), (50u8, 120u8, 190u8))
    } else {
        // Unfocused gradient: dark grey-blue → lighter grey-blue
        ((20u8, 35u8, 50u8), (55u8, 85u8, 110u8))
    };

    let lerp = |a: u8, b: u8| (a as f64 + (b as f64 - a as f64) * t).round() as u8;
    Color::Rgb(
        lerp(from.0, to.0),
        lerp(from.1, to.1),
        lerp(from.2, to.2),
    )
}

/// Render a panel of sliders.
pub fn render(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    sliders: &[SliderDef],
    selected: Option<usize>,
    focused: bool,
    dimmed: bool,
) {
    let border_color = if dimmed {
        Color::DarkGray
    } else if focused {
        Color::Cyan
    } else {
        Color::White
    };
    let block = Block::default()
        .title(format!(" {title} "))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height == 0 || inner.width < 10 {
        return;
    }

    let mut lines = Vec::new();
    for (i, slider) in sliders.iter().enumerate() {
        let is_selected = selected == Some(i) && focused;

        let label_style = if dimmed {
            Style::default().fg(Color::DarkGray)
        } else if is_selected {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };

        // Label line: "▸ Pitch Shift" or "  Pitch Shift"
        let indicator = if is_selected { "▸ " } else { "  " };
        let label_line = Line::from(vec![
            Span::styled(indicator, label_style),
            Span::styled(slider.label, label_style),
        ]);
        lines.push(label_line);

        // Bar line: "  [████████░░░░░░░░] 3.5 st"
        let bar_width = (inner.width as usize).saturating_sub(6); // padding + value space
        let value_str = if slider.unit.is_empty() {
            format!("{:.2}", slider.value)
        } else {
            format!("{:.1} {}", slider.value, slider.unit)
        };
        let track_width = bar_width.saturating_sub(value_str.len() + 3);

        if track_width > 2 {
            // Sub-pixel calculation: exact position within track
            let exact = slider.fraction() * track_width as f64;
            let filled = exact.floor() as usize;
            let frac = exact.fract();

            let partial = partial_block_char(frac);
            let has_partial = !partial.is_empty();

            // Build spans: one per filled char + optional partial + empty remainder
            let mut spans: Vec<Span> = Vec::with_capacity(track_width + 4);
            spans.push(Span::raw("  ["));

            // Filled characters with gradient
            for i in 0..filled {
                let t = if track_width > 1 {
                    i as f64 / (track_width - 1) as f64
                } else {
                    0.5
                };
                let ch = block_char_for_pos(i, filled);
                spans.push(Span::styled(
                    ch,
                    Style::default().fg(gradient_color(t, is_selected, focused, dimmed)),
                ));
            }

            // Partial block (sub-pixel leading edge)
            if has_partial {
                let t = if track_width > 1 {
                    filled as f64 / (track_width - 1) as f64
                } else {
                    1.0
                };
                spans.push(Span::styled(
                    partial,
                    Style::default().fg(gradient_color(t, is_selected, focused, dimmed)),
                ));
            }

            // Empty remainder
            let used = filled + if has_partial { 1 } else { 0 };
            let empty = track_width.saturating_sub(used);
            if empty > 0 {
                spans.push(Span::styled(
                    "░".repeat(empty),
                    Style::default().fg(Color::DarkGray),
                ));
            }

            spans.push(Span::raw("] "));
            spans.push(Span::styled(value_str, Style::default().fg(if dimmed { Color::DarkGray } else { Color::Yellow })));

            let bar_line = Line::from(spans);
            lines.push(bar_line);
        } else {
            // Narrow fallback: just show value
            let val_line = Line::from(vec![
                Span::raw("  "),
                Span::styled(value_str, Style::default().fg(if dimmed { Color::DarkGray } else { Color::Yellow })),
            ]);
            lines.push(val_line);
        }
    }

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, inner);
}
