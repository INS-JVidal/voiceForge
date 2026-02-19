use std::path::Path;
use std::sync::atomic::Ordering;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{Action, AppMode, AppState, PanelFocus};
use crate::audio::export;

/// Handle a key press event, mutating app state and optionally returning an action.
pub fn handle_key_event(key: KeyEvent, app: &mut AppState) -> Option<Action> {
    match app.mode {
        AppMode::FilePicker => handle_file_picker(key, app),
        AppMode::Saving => handle_save_dialog(key, app),
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

fn handle_save_dialog(key: KeyEvent, app: &mut AppState) -> Option<Action> {
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
                Some(Action::ExportWav(path))
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
            if app.focus == PanelFocus::Transport {
                if let Some(ref info) = app.file_info {
                    app.playback.seek_by_secs(
                        -5.0,
                        info.sample_rate,
                        info.channels,
                        info.total_samples,
                    );
                }
                None
            } else {
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
                effects_slider_action(focus, idx, app)
            }
        }
        KeyCode::Right => {
            if app.focus == PanelFocus::Transport {
                if let Some(ref info) = app.file_info {
                    app.playback.seek_by_secs(
                        5.0,
                        info.sample_rate,
                        info.channels,
                        info.total_samples,
                    );
                }
                None
            } else {
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
                effects_slider_action(focus, idx, app)
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
        KeyCode::Home => {
            app.playback.position.store(0, Ordering::Release);
            None
        }
        KeyCode::End => {
            if let Some(ref info) = app.file_info {
                app.playback
                    .position
                    .store(info.total_samples, Ordering::Release);
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
        KeyCode::Char('s') => {
            if app.audio_data.is_some() {
                let default_path = if let Some(ref info) = app.file_info {
                    export::default_export_path(&info.path)
                } else {
                    "output_processed.wav".to_string()
                };
                app.file_picker_input = default_path;
                app.mode = AppMode::Saving;
            } else {
                app.status_message = Some("No audio to export".to_string());
            }
            None
        }
        KeyCode::Char('o') => {
            app.mode = AppMode::FilePicker;
            app.file_picker_input.clear();
            None
        }
        _ => None,
    }
}

/// Determine the action after adjusting an effects or WORLD slider.
/// Gain (effects index 0) is applied live in the audio callback; all other
/// effects go through the processing thread.
fn effects_slider_action(focus: PanelFocus, idx: usize, app: &AppState) -> Option<Action> {
    match focus {
        PanelFocus::WorldSliders => Some(Action::Resynthesize),
        PanelFocus::EffectsSliders => {
            if idx == 0 {
                let linear = 10.0_f32.powf(app.effects_sliders[0].value as f32 / 20.0);
                Some(Action::LiveGain(linear))
            } else {
                Some(Action::ReapplyEffects)
            }
        }
        PanelFocus::Transport => None,
    }
}
