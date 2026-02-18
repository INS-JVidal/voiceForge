use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::SliderDef;

/// Render a panel of sliders.
pub fn render(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    sliders: &[SliderDef],
    selected: Option<usize>,
    focused: bool,
) {
    let border_color = if focused { Color::Cyan } else { Color::White };
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

        let label_style = if is_selected {
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
            let filled = ((slider.fraction() * track_width as f64).round() as usize).min(track_width);
            let empty = track_width - filled;

            let bar_color = if is_selected { Color::Cyan } else { Color::Blue };
            let bar_line = Line::from(vec![
                Span::raw("  ["),
                Span::styled("█".repeat(filled), Style::default().fg(bar_color)),
                Span::styled("░".repeat(empty), Style::default().fg(Color::DarkGray)),
                Span::raw("] "),
                Span::styled(value_str, Style::default().fg(Color::Yellow)),
            ]);
            lines.push(bar_line);
        } else {
            // Narrow fallback: just show value
            let val_line = Line::from(vec![
                Span::raw("  "),
                Span::styled(value_str, Style::default().fg(Color::Yellow)),
            ]);
            lines.push(val_line);
        }
    }

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, inner);
}
