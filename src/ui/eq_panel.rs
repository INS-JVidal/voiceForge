use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

/// 12 EQ band frequencies for display.
const EQ_FREQS: [&str; 12] = [
    "31", "63", "125", "250", "500", "1k", "2k", "3.1k", "4k", "6.3k", "10k", "16k",
];

/// Render the 12-band graphic EQ panel as vertical bars.
pub fn render(
    frame: &mut Frame,
    area: Rect,
    eq_gains: &[f64; 12],
    selected_band: usize,
    focused: bool,
) {
    // Guard: too narrow
    if area.width < 12 {
        return;
    }

    let title_color = if focused { Color::Cyan } else { Color::White };
    let border_style = Style::default().fg(title_color);
    let block = Block::default()
        .title(" Graphic EQ ")
        .title_alignment(ratatui::layout::Alignment::Center)
        .borders(Borders::ALL)
        .style(border_style);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Guard: not enough space for display
    if inner.height < 3 || inner.width < 12 {
        return;
    }

    // Column width for each band
    let col_width = inner.width / 12;
    if col_width < 1 {
        return;
    }

    // Create a buffer to render the content
    // The display shows values from -6 dB (bottom) to +6 dB (top)
    // Reserve 1 row at top for value label, 1 row at bottom for freq label
    let available_rows = inner.height.saturating_sub(2);
    if available_rows < 1 {
        return; // Not enough space for bars
    }

    let total_rows = available_rows;
    let center_row = (total_rows / 2) as usize; // 0 dB line
    let bar_area_y = inner.y + 1; // Start bars after value label row

    // For each column
    for band_idx in 0..12 {
        let col_x = inner.x + (band_idx as u16 * col_width);
        let col_width = col_width.min((inner.x + inner.width).saturating_sub(col_x));

        if col_width < 1 {
            continue;
        }

        let gain = eq_gains[band_idx];

        // Determine row for the gain value (scale: -6 to +6 dB per 'total_rows' pixels)
        // 0 dB is at center_row, +6 dB is at row 0, -6 dB is at row total_rows-1
        let db_per_row = 12.0 / total_rows as f64;
        let gain_row = (center_row as f64 - gain / db_per_row).round() as i32;

        // Render the column
        for row in 0..total_rows {
            let row_idx = row as usize;
            let y = bar_area_y + row;

            // Determine if this is the center (0 dB) line
            let is_zero_line = row_idx == center_row;

            if is_zero_line {
                // 0 dB line
                let marker = if band_idx == selected_band { "▸" } else { "─" };
                let style = if band_idx == selected_band {
                    Style::default().fg(Color::Cyan)
                } else {
                    Style::default().fg(Color::Gray)
                };
                let span = Span::styled(marker, style);
                let para = Paragraph::new(span);
                frame.render_widget(para, Rect {
                    x: col_x,
                    y,
                    width: col_width,
                    height: 1,
                });
            } else if gain_row >= 0 && row_idx <= gain_row as usize && gain > 1e-6 {
                // Boost region (above center)
                let style = if band_idx == selected_band {
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::Cyan).bg(Color::Black)
                };
                let span = Span::styled("█", style);
                let para = Paragraph::new(span);
                frame.render_widget(para, Rect {
                    x: col_x,
                    y,
                    width: col_width,
                    height: 1,
                });
            } else if gain_row >= 0 && row_idx > gain_row as usize && gain < -1e-6 {
                // Cut region (below center): gain is negative, so boost_row doesn't apply
                let boost_row = (center_row as f64 + (-gain) / db_per_row).round() as i32;
                if boost_row >= 0 && row_idx <= boost_row as usize {
                    let style = if band_idx == selected_band {
                        Style::default()
                            .fg(Color::Black)
                            .bg(Color::Red)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::Red).bg(Color::Black)
                    };
                    let span = Span::styled("█", style);
                    let para = Paragraph::new(span);
                    frame.render_widget(para, Rect {
                        x: col_x,
                        y,
                        width: col_width,
                        height: 1,
                    });
                }
            }
        }

        // Render frequency label at bottom
        let freq_label = EQ_FREQS[band_idx];
        let freq_style = if band_idx == selected_band {
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Gray)
        };
        let freq_span = Span::styled(freq_label, freq_style);
        let freq_para = Paragraph::new(freq_span);
        frame.render_widget(
            freq_para,
            Rect {
                x: col_x,
                y: bar_area_y + total_rows,
                width: col_width,
                height: 1,
            },
        );

        // Render value label at top
        let gain_str = format!("{:+.1}", gain);
        let val_style = if band_idx == selected_band {
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Gray)
        };
        let val_span = Span::styled(&gain_str[..gain_str.len().min(col_width as usize)], val_style);
        let val_para = Paragraph::new(val_span);
        frame.render_widget(
            val_para,
            Rect {
                x: col_x,
                y: inner.y,
                width: col_width,
                height: 1,
            },
        );
    }
}
