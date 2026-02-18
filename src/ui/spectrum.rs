use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

pub fn render(frame: &mut Frame, area: Rect) {
    let block = Block::default()
        .title(" Spectrum ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    // #16: When implementing real spectrum rendering, ensure bar indices are
    // bounds-checked against `area.width` and magnitudes clamped to `area.height`.
    let content = Paragraph::new("  Spectrum visualization — coming in P5")
        .style(Style::default().fg(Color::DarkGray))
        .block(block);

    frame.render_widget(content, area);
}
