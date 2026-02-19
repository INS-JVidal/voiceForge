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
        AppMode::Help => {
            // Any key dismisses the help overlay.
            app.mode = AppMode::Normal;
            None
        }
        AppMode::Normal => handle_normal(key, app),
    }
}

/// L-11: Handle text editing keys (cursor movement, insert, delete) for input fields.
/// Returns `true` if the key was handled as a text editing action.
fn handle_text_input(key: &KeyEvent, app: &mut AppState) -> bool {
    match key.code {
        KeyCode::Left => {
            if app.input_cursor > 0 {
                // Move cursor left by one char (handle multi-byte UTF-8).
                let prev = app.file_picker_input[..app.input_cursor]
                    .char_indices()
                    .next_back()
                    .map(|(i, _)| i)
                    .unwrap_or(0);
                app.input_cursor = prev;
            }
            true
        }
        KeyCode::Right => {
            if app.input_cursor < app.file_picker_input.len() {
                let next = app.file_picker_input[app.input_cursor..]
                    .char_indices()
                    .nth(1)
                    .map(|(i, _)| app.input_cursor + i)
                    .unwrap_or(app.file_picker_input.len());
                app.input_cursor = next;
            }
            true
        }
        KeyCode::Home => {
            app.input_cursor = 0;
            true
        }
        KeyCode::End => {
            app.input_cursor = app.file_picker_input.len();
            true
        }
        KeyCode::Backspace => {
            if app.input_cursor > 0 {
                let prev = app.file_picker_input[..app.input_cursor]
                    .char_indices()
                    .next_back()
                    .map(|(i, _)| i)
                    .unwrap_or(0);
                app.file_picker_input.drain(prev..app.input_cursor);
                app.input_cursor = prev;
            }
            true
        }
        KeyCode::Delete => {
            if app.input_cursor < app.file_picker_input.len() {
                let next = app.file_picker_input[app.input_cursor..]
                    .char_indices()
                    .nth(1)
                    .map(|(i, _)| app.input_cursor + i)
                    .unwrap_or(app.file_picker_input.len());
                app.file_picker_input.drain(app.input_cursor..next);
            }
            true
        }
        KeyCode::Char(c) => {
            app.file_picker_input.insert(app.input_cursor, c);
            app.input_cursor += c.len_utf8();
            true
        }
        _ => false,
    }
}

/// Update file picker suggestions based on current input.
/// Lists directory contents and filters by the input prefix, keeping first 5 matches.
fn update_file_picker_matches(app: &mut AppState) {
    use std::env;
    use std::fs;

    let input = &app.file_picker_input;

    // Parse input into (dir_part, prefix) at the last '/'
    let (mut dir_part, prefix) = if let Some(pos) = input.rfind('/') {
        (input[..=pos].to_string(), input[pos + 1..].to_string())
    } else {
        (".".to_string(), input.to_string())
    };

    // Expand ~ to home directory
    if dir_part.starts_with("~/") {
        if let Ok(home) = env::var("HOME") {
            dir_part = format!("{}{}", home, &dir_part[1..]);
        } else {
            dir_part = ".".to_string();
        }
    }

    // Fallback: empty dir means current directory
    if dir_part.is_empty() {
        dir_part = ".".to_string();
    }

    // Read directory entries
    let mut matches = Vec::new();
    if let Ok(entries) = fs::read_dir(&dir_part) {
        for entry in entries.take(1000) {
            let Ok(entry) = entry else { continue };
            let name = entry.file_name().to_string_lossy().to_string();

            // Hide dotfiles unless prefix starts with '.'
            if name.starts_with('.') && !prefix.starts_with('.') {
                continue;
            }

            // Filter by prefix (case-sensitive, standard Unix behavior)
            if !name.starts_with(&prefix) {
                continue;
            }

            // Determine if directory (follow symlinks)
            let is_dir = entry
                .path()
                .metadata()
                .map(|m| m.is_dir())
                .unwrap_or(false);

            // Build stored path: strip "./" prefix if dir_part was "."
            let display_path = if dir_part == "." {
                format!("{}{}", name, if is_dir { "/" } else { "" })
            } else {
                format!("{}{}{}", dir_part, name, if is_dir { "/" } else { "" })
            };

            matches.push((is_dir, display_path));
        }
    }

    // Sort: directories first, then files; alphabetical within each group
    matches.sort_by(|a, b| match (a.0, b.0) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.1.cmp(&b.1),
    });

    // Store all filtered matches
    app.file_picker_matches = matches
        .into_iter()
        .map(|(_, path)| path)
        .collect();

    // Reset scroll offset when list is recomputed
    app.file_picker_scroll = 0;

    // Clamp selection to valid range
    if let Some(sel) = app.file_picker_selected {
        if app.file_picker_matches.is_empty() {
            app.file_picker_selected = None;
        } else if sel >= app.file_picker_matches.len() {
            app.file_picker_selected = Some(app.file_picker_matches.len() - 1);
        }
    }
}

/// Check if a file appears to be a valid audio file by examining its magic bytes.
/// Returns Err with an error message if the file is not recognized as audio or can't be read.
fn precheck_audio_file(path: &str) -> Result<(), String> {
    use std::fs::File;
    use std::io::Read;

    let mut file =
        File::open(path).map_err(|e| format!("Cannot open file: {}", e))?;

    let mut buf = [0u8; 12];
    let n = file.read(&mut buf).map_err(|e| format!("Cannot read file: {}", e))?;

    if n == 0 {
        return Err("File is empty".to_string());
    }

    // Match known audio magic signatures
    let is_audio = n >= 4
        && (
            // WAV: "RIFF" at 0, "WAVE" at 8
            (buf[..4] == *b"RIFF" && n >= 12 && buf[8..12] == *b"WAVE")
                ||
                // FLAC: "fLaC" at 0
                buf[..4] == *b"fLaC"
                ||
                // OGG (Vorbis, Opus, Flac)
                buf[..4] == *b"OggS"
                ||
                // MP3 with ID3 tag: "ID3" at 0
                buf[..3] == *b"ID3"
                ||
                // MP3 sync word: 0xFF 0xEX (frame sync)
                (buf[0] == 0xFF && (buf[1] & 0xE0 == 0xE0))
                ||
                // M4A/AAC/MP4: "ftyp" at offset 4
                (n >= 8 && buf[4..8] == *b"ftyp")
                ||
                // AIFF: "FORM" at 0, "AIFF" at 8
                (buf[..4] == *b"FORM" && n >= 12 && buf[8..12] == *b"AIFF")
        );

    if is_audio {
        Ok(())
    } else {
        let filename = Path::new(path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file");
        Err(format!("Not a recognized audio format: {}", filename))
    }
}

fn handle_file_picker(key: KeyEvent, app: &mut AppState) -> Option<Action> {
    match key.code {
        KeyCode::Esc => {
            app.mode = AppMode::Normal;
            app.file_picker_input.clear();
            app.input_cursor = 0;
            app.file_picker_matches.clear();
            app.file_picker_scroll = 0;
            app.file_picker_selected = None;
            None
        }
        KeyCode::Down => {
            if !app.file_picker_matches.is_empty() {
                match app.file_picker_selected {
                    None => app.file_picker_selected = Some(0),
                    Some(i) if i + 1 < app.file_picker_matches.len() => {
                        app.file_picker_selected = Some(i + 1);
                    }
                    _ => {}
                }
                // Adjust scroll to keep selection visible in 5-row window
                const VISIBLE: usize = 5;
                if let Some(sel) = app.file_picker_selected {
                    if sel >= app.file_picker_scroll + VISIBLE {
                        app.file_picker_scroll = sel + 1 - VISIBLE;
                    }
                }
            }
            None
        }
        KeyCode::Up => {
            match app.file_picker_selected {
                Some(0) => app.file_picker_selected = None,
                Some(i) => app.file_picker_selected = Some(i - 1),
                None => {}
            }
            // Adjust scroll to keep selection visible in 5-row window
            if let Some(sel) = app.file_picker_selected {
                if sel < app.file_picker_scroll {
                    app.file_picker_scroll = sel;
                }
            }
            None
        }
        KeyCode::Tab => {
            // Determine which match to use: selected index, or first match if no selection
            let match_idx = app
                .file_picker_selected
                .or(if app.file_picker_matches.is_empty() { None } else { Some(0) });

            if let Some(idx) = match_idx {
                if idx < app.file_picker_matches.len() {
                    let match_path = app.file_picker_matches[idx].clone();
                    if match_path.ends_with('/') {
                        // Navigate into directory
                        app.file_picker_input = match_path;
                        app.input_cursor = app.file_picker_input.len();
                        app.file_picker_selected = None;
                        update_file_picker_matches(app);
                    } else {
                        // Set input to file path (preview before Enter)
                        app.file_picker_input = match_path;
                        app.input_cursor = app.file_picker_input.len();
                    }
                }
            }
            None
        }
        KeyCode::Enter => {
            // Check if a suggestion is selected
            if let Some(idx) = app.file_picker_selected {
                if idx < app.file_picker_matches.len() {
                    let match_path = app.file_picker_matches[idx].clone();
                    if match_path.ends_with('/') {
                        // Directory: navigate into it
                        app.file_picker_input = match_path;
                        app.input_cursor = app.file_picker_input.len();
                        app.file_picker_selected = None;
                        update_file_picker_matches(app);
                        return None;
                    } else {
                        // File: precheck and load
                        match precheck_audio_file(&match_path) {
                            Ok(()) => {
                                app.file_picker_input.clear();
                                app.input_cursor = 0;
                                app.file_picker_matches.clear();
                                app.file_picker_selected = None;
                                app.mode = AppMode::Normal;
                                return Some(Action::LoadFile(match_path));
                            }
                            Err(e) => {
                                app.set_status(format!("Error: {}", e));
                                return None;
                            }
                        }
                    }
                }
            }

            // No selection: use raw input
            let path = app.file_picker_input.trim().to_string();
            app.file_picker_input.clear();
            app.input_cursor = 0;
            app.file_picker_matches.clear();
            app.file_picker_selected = None;
            app.mode = AppMode::Normal;

            if path.is_empty() {
                return None;
            }

            // Validate path exists and is a file
            let p = Path::new(&path);
            if !p.exists() {
                app.set_status(format!("File not found: {path}"));
                return None;
            }
            if !p.is_file() {
                app.set_status("Path is not a file".to_string());
                return None;
            }

            // Precheck audio file integrity
            match precheck_audio_file(&path) {
                Ok(()) => Some(Action::LoadFile(path)),
                Err(e) => {
                    app.set_status(format!("Error: {}", e));
                    None
                }
            }
        }
        _ => {
            // All other keys: handle text input, then update matches and reset selection
            let before_len = app.file_picker_input.len();
            let handled = handle_text_input(&key, app);
            if handled && app.file_picker_input.len() != before_len {
                // Input changed: recompute matches and reset selection
                update_file_picker_matches(app);
                app.file_picker_selected = None;
            }
            None
        }
    }
}

fn handle_save_dialog(key: KeyEvent, app: &mut AppState) -> Option<Action> {
    match key.code {
        KeyCode::Esc => {
            app.mode = AppMode::Normal;
            app.file_picker_input.clear();
            app.input_cursor = 0;
            None
        }
        KeyCode::Enter => {
            let path = app.file_picker_input.trim().to_string();
            app.file_picker_input.clear();
            app.input_cursor = 0;
            app.mode = AppMode::Normal;
            if path.is_empty() {
                None
            } else {
                // M-8: Validate the save path before dispatching ExportWav.
                let p = Path::new(&path);
                if p.is_dir() {
                    app.set_status("Path is a directory, not a file".to_string());
                    None
                } else {
                    Some(Action::ExportWav(path))
                }
            }
        }
        _ => {
            handle_text_input(&key, app);
            None
        }
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
            // M-7: Always reset to 0 on Tab for consistent per-panel behavior.
            app.selected_slider = 0;
            None
        }
        KeyCode::Up => {
            match app.focus {
                PanelFocus::EqBands => {
                    // Adjust EQ band gain upward by 0.5 dB
                    app.eq_gains[app.eq_selected_band] =
                        (app.eq_gains[app.eq_selected_band] + 0.5).clamp(-6.0, 6.0);
                    Some(Action::ReapplyEffects)
                }
                _ => {
                    if app.selected_slider > 0 {
                        app.selected_slider -= 1;
                    }
                    None
                }
            }
        }
        KeyCode::Down => {
            match app.focus {
                PanelFocus::EqBands => {
                    // Adjust EQ band gain downward by 0.5 dB
                    app.eq_gains[app.eq_selected_band] =
                        (app.eq_gains[app.eq_selected_band] - 0.5).clamp(-6.0, 6.0);
                    Some(Action::ReapplyEffects)
                }
                _ => {
                    let count = app.focused_slider_count();
                    if app.focus != PanelFocus::Transport && app.selected_slider + 1 < count {
                        app.selected_slider += 1;
                    }
                    None
                }
            }
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
            } else if app.focus == PanelFocus::EqBands {
                // Navigate to previous band or adjust gain with Shift
                if key.modifiers.contains(KeyModifiers::SHIFT) {
                    // Fine adjust: -0.1 dB
                    app.eq_gains[app.eq_selected_band] =
                        (app.eq_gains[app.eq_selected_band] - 0.1).clamp(-6.0, 6.0);
                    Some(Action::ReapplyEffects)
                } else {
                    // Navigate to previous band
                    if app.eq_selected_band > 0 {
                        app.eq_selected_band -= 1;
                    }
                    None
                }
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
            } else if app.focus == PanelFocus::EqBands {
                // Navigate to next band or adjust gain with Shift
                if key.modifiers.contains(KeyModifiers::SHIFT) {
                    // Fine adjust: +0.1 dB
                    app.eq_gains[app.eq_selected_band] =
                        (app.eq_gains[app.eq_selected_band] + 0.1).clamp(-6.0, 6.0);
                    Some(Action::ReapplyEffects)
                } else {
                    // Navigate to next band
                    if app.eq_selected_band + 1 < 12 {
                        app.eq_selected_band += 1;
                    }
                    None
                }
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
            app.playback
                .loop_enabled
                .store(app.loop_enabled, Ordering::Relaxed);
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
            // M-2: Land on the last frame start, not one-past-end.
            if let Some(ref info) = app.file_info {
                let last_frame = info
                    .total_samples
                    .saturating_sub(info.channels as usize);
                app.playback
                    .position
                    .store(last_frame, Ordering::Release);
            }
            None
        }
        KeyCode::Char('a') => {
            // H-9: Also require audio_lock to be available before toggling A/B,
            // so ToggleAB never flips ab_original without a buffer swap.
            if app.audio_data.is_some()
                && app.original_audio.is_some()
                && app.playback.audio_lock.is_some()
            {
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
                app.input_cursor = app.file_picker_input.len();
                // Clear file picker matches to avoid visual bleed into save dialog
                app.file_picker_matches.clear();
                app.file_picker_scroll = 0;
                app.file_picker_selected = None;
                app.mode = AppMode::Saving;
            } else {
                app.set_status("No audio to export".to_string());
            }
            None
        }
        KeyCode::Char('o') => {
            app.mode = AppMode::FilePicker;
            app.file_picker_input.clear();
            app.input_cursor = 0;
            app.file_picker_selected = None;
            // Populate with CWD contents immediately on open
            update_file_picker_matches(app);
            None
        }
        KeyCode::Char('?') => {
            app.mode = AppMode::Help;
            None
        }
        KeyCode::Char('d') => {
            // Reset the selected slider to its default value, or EQ band to 0 dB.
            match app.focus {
                PanelFocus::EqBands => {
                    let old_val = app.eq_gains[app.eq_selected_band];
                    app.eq_gains[app.eq_selected_band] = 0.0;
                    if (old_val - 0.0).abs() > 1e-6 {
                        Some(Action::ReapplyEffects)
                    } else {
                        None
                    }
                }
                _ => {
                    let focus = app.focus;
                    let idx = app.selected_slider;
                    let changed = if let Some(sliders) = app.focused_sliders_mut() {
                        if idx < sliders.len() {
                            sliders[idx].reset()
                        } else {
                            false
                        }
                    } else {
                        false
                    };
                    if changed {
                        effects_slider_action(focus, idx, app)
                    } else {
                        None
                    }
                }
            }
        }
        _ => None,
    }
}

/// Determine the action after adjusting an effects or WORLD slider.
/// Gain (effects index 0) is applied live in the audio callback; all other
/// effects go through the processing thread.
fn effects_slider_action(focus: PanelFocus, _idx: usize, app: &AppState) -> Option<Action> {
    match focus {
        PanelFocus::WorldSliders => Some(Action::Resynthesize),
        PanelFocus::EffectsSliders => Some(Action::ReapplyEffects),
        PanelFocus::Master => {
            let linear = 10.0_f32.powf(app.master_sliders[0].value as f32 / 20.0);
            Some(Action::LiveGain(linear))
        }
        PanelFocus::EqBands => Some(Action::ReapplyEffects),
        PanelFocus::Transport => None,
    }
}
