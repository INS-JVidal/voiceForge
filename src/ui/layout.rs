use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::Frame;

use crate::app::{AppMode, AppState, PanelFocus};
use crate::ui::{eq_panel, file_picker, help, save_dialog, slider, spectrum, status_bar, transport};

pub fn render(frame: &mut Frame, app: &mut AppState) {
    let area = frame.area();

    // H-5: Minimum terminal size guard to prevent zero-height render areas.
    if area.height < 16 || area.width < 40 {
        use ratatui::style::{Color, Style};
        use ratatui::text::Span;
        use ratatui::widgets::Paragraph;
        let msg = Paragraph::new(Span::styled(
            "Terminal too small (min 40×16)",
            Style::default().fg(Color::Red),
        ));
        frame.render_widget(msg, area);
        return;
    }

    // Main vertical layout:
    // [Slider panels + EQ panel + Spectrum] (fill)  |  [Transport] (3)  |  [Status bar] (1)
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(10),   // slider panels + EQ + spectrum
            Constraint::Length(3), // transport
            Constraint::Length(1), // status bar
        ])
        .split(area);

    let top = vertical[0];
    let transport_area = vertical[1];
    let status_area = vertical[2];

    // Top area: split vertically into slider panels (40%), EQ panel (10), and spectrum (Min 4)
    let top_split = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(40), // slider panels
            Constraint::Length(10),     // EQ panel
            Constraint::Min(4),         // spectrum
        ])
        .split(top);

    let sliders_area = top_split[0];
    let eq_area = top_split[1];
    let spectrum_area = top_split[2];

    // Slider panels: three columns (WORLD 40% | Effects 40% | Master 20%)
    let slider_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(40),
            Constraint::Percentage(40),
            Constraint::Percentage(20),
        ])
        .split(sliders_area);

    // Render slider panels
    let world_selected = if app.focus == PanelFocus::WorldSliders {
        Some(app.selected_slider)
    } else {
        None
    };
    let world_title = if app.world_bypass {
        "WORLD Vocoder [OFF]"
    } else {
        "WORLD Vocoder"
    };
    slider::render(
        frame,
        slider_cols[0],
        world_title,
        &app.world_sliders,
        world_selected,
        app.focus == PanelFocus::WorldSliders,
        app.world_bypass,
    );

    let effects_selected = if app.focus == PanelFocus::EffectsSliders {
        Some(app.selected_slider)
    } else {
        None
    };
    slider::render(
        frame,
        slider_cols[1],
        "Effects",
        &app.effects_sliders,
        effects_selected,
        app.focus == PanelFocus::EffectsSliders,
        false,
    );

    let master_selected = if app.focus == PanelFocus::Master {
        Some(app.selected_slider)
    } else {
        None
    };
    slider::render(
        frame,
        slider_cols[2],
        "Master",
        &app.master_sliders,
        master_selected,
        app.focus == PanelFocus::Master,
        false,
    );

    // EQ panel
    eq_panel::render(
        frame,
        eq_area,
        &app.eq_gains,
        app.eq_selected_band,
        app.focus == PanelFocus::EqBands,
    );

    // Spectrum visualizer (GPU pixel or Unicode fallback)
    spectrum::render(frame, spectrum_area, app);

    // Transport bar
    transport::render(frame, transport_area, app);

    // Status bar
    status_bar::render(frame, status_area, app);

    // Modal overlays (on top of everything)
    if app.mode == AppMode::FilePicker {
        file_picker::render(frame, app);
    }
    if app.mode == AppMode::Saving {
        save_dialog::render(frame, app);
    }
    if app.mode == AppMode::Help {
        help::render(frame);
    }
}
