use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::Frame;

use crate::app::{AppMode, AppState, PanelFocus};
use crate::ui::{file_picker, slider, spectrum, status_bar, transport};

pub fn render(frame: &mut Frame, app: &AppState) {
    let area = frame.area();

    // Main vertical layout:
    // [Slider panels + Spectrum] (fill)  |  [Transport] (3)  |  [Status bar] (1)
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(8),    // slider panels + spectrum
            Constraint::Length(3), // transport
            Constraint::Length(1), // status bar
        ])
        .split(area);

    let top = vertical[0];
    let transport_area = vertical[1];
    let status_area = vertical[2];

    // Top area: split vertically into slider panels (top 60%) and spectrum (rest)
    let top_split = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(70), // slider panels
            Constraint::Min(3),         // spectrum
        ])
        .split(top);

    let sliders_area = top_split[0];
    let spectrum_area = top_split[1];

    // Slider panels: two side-by-side
    let slider_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(sliders_area);

    // Render slider panels
    let world_selected = if app.focus == PanelFocus::WorldSliders {
        Some(app.selected_slider)
    } else {
        None
    };
    slider::render(
        frame,
        slider_cols[0],
        "WORLD Vocoder",
        &app.world_sliders,
        world_selected,
        app.focus == PanelFocus::WorldSliders,
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
    );

    // Spectrum placeholder
    spectrum::render(frame, spectrum_area, app);

    // Transport bar
    transport::render(frame, transport_area, app);

    // Status bar
    status_bar::render(frame, status_area, app);

    // File picker overlay (on top of everything)
    if app.mode == AppMode::FilePicker {
        file_picker::render(frame, app);
    }
}
