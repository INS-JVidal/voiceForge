use std::path::Path;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{Action, AppMode, AppState, PanelFocus};

/// Handle a key press event, mutating app state and optionally returning an action.
pub fn handle_key_event(key: KeyEvent, app: &mut AppState) -> Option<Action> {
    match app.mode {
        AppMode::FilePicker => handle_file_picker(key, app),
        AppMode::Normal => handle_normal(key, app),
    }
}

fn handle_file_picker(key: KeyEvent, app: &mut AppState) -> Option<Action> {
    match key.code {
        KeyCode::Esc => {
            app.mode = AppMode::Normal;
            app.file_picker_input.clear();
            None
        }
        KeyCode::Enter => {
            let path = app.file_picker_input.trim().to_string();
            app.file_picker_input.clear();
            app.mode = AppMode::Normal;
            if path.is_empty() {
                None
            } else {
                // #6: Validate path before loading — reject obviously invalid paths.
                let p = Path::new(&path);
                if !p.exists() {
                    app.status_message = Some(format!("File not found: {path}"));
                    None
                } else if !p.is_file() {
                    app.status_message = Some("Path is not a file".to_string());
                    None
                } else {
                    Some(Action::LoadFile(path))
                }
            }
        }
        KeyCode::Backspace => {
            app.file_picker_input.pop();
            None
        }
        KeyCode::Char(c) => {
            app.file_picker_input.push(c);
            None
        }
        _ => None,
    }
}

fn handle_normal(key: KeyEvent, app: &mut AppState) -> Option<Action> {
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => {
            app.should_quit = true;
            Some(Action::Quit)
        }
        KeyCode::Char(' ') => {
            app.playback.toggle_playing();
            None
        }
        KeyCode::Tab => {
            app.focus = app.focus.next();
            // #8: Always clamp selected_slider, including when count is 0.
            let count = app.focused_slider_count();
            if count == 0 {
                app.selected_slider = 0;
            } else if app.selected_slider >= count {
                app.selected_slider = count - 1;
            }
            None
        }
        KeyCode::Up => {
            if app.focus != PanelFocus::Transport && app.selected_slider > 0 {
                app.selected_slider -= 1;
            }
            None
        }
        KeyCode::Down => {
            let count = app.focused_slider_count();
            if app.focus != PanelFocus::Transport && app.selected_slider + 1 < count {
                app.selected_slider += 1;
            }
            None
        }
        KeyCode::Left => {
            let steps = if key.modifiers.contains(KeyModifiers::SHIFT) {
                -0.2
            } else {
                -1.0
            };
            let focus = app.focus;
            let idx = app.selected_slider;
            if let Some(sliders) = app.focused_sliders_mut() {
                if idx < sliders.len() {
                    sliders[idx].adjust(steps);
                }
            }
            if focus == PanelFocus::WorldSliders {
                Some(Action::Resynthesize)
            } else {
                None
            }
        }
        KeyCode::Right => {
            let steps = if key.modifiers.contains(KeyModifiers::SHIFT) {
                0.2
            } else {
                1.0
            };
            let focus = app.focus;
            let idx = app.selected_slider;
            if let Some(sliders) = app.focused_sliders_mut() {
                if idx < sliders.len() {
                    sliders[idx].adjust(steps);
                }
            }
            if focus == PanelFocus::WorldSliders {
                Some(Action::Resynthesize)
            } else {
                None
            }
        }
        KeyCode::Char('r') => {
            app.loop_enabled = !app.loop_enabled;
            None
        }
        KeyCode::Char('[') => {
            if let Some(ref info) = app.file_info {
                app.playback.seek_by_secs(
                    -5.0,
                    info.sample_rate,
                    info.channels,
                    info.total_samples,
                );
            }
            None
        }
        KeyCode::Char(']') => {
            if let Some(ref info) = app.file_info {
                app.playback.seek_by_secs(
                    5.0,
                    info.sample_rate,
                    info.channels,
                    info.total_samples,
                );
            }
            None
        }
        KeyCode::Char('a') => {
            if app.audio_data.is_some() && app.original_audio.is_some() {
                app.ab_original = !app.ab_original;
                Some(Action::ToggleAB)
            } else {
                None
            }
        }
        KeyCode::Char('o') => {
            app.mode = AppMode::FilePicker;
            app.file_picker_input.clear();
            None
        }
        _ => None,
    }
}
