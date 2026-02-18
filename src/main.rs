use std::io::{self, stdout};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use ratatui::crossterm::event::{self, Event, KeyEventKind};
use ratatui::crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::crossterm::ExecutableCommand;
use ratatui::prelude::CrosstermBackend;
use ratatui::Terminal;

use voiceforge::app::{Action, AppState, FileInfo};
use voiceforge::audio;
use voiceforge::input::handler::handle_key_event;
use voiceforge::ui::layout;

/// RAII guard that restores the terminal on drop (including panics).
struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        // #18: Log errors instead of silently ignoring — a corrupted terminal is worse
        // than a warning on stderr.
        if let Err(e) = disable_raw_mode() {
            eprintln!("warning: failed to disable raw mode: {e}");
        }
        if let Err(e) = stdout().execute(LeaveAlternateScreen) {
            eprintln!("warning: failed to leave alternate screen: {e}");
        }
    }
}

fn main() -> io::Result<()> {
    // Set up terminal
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    let _guard = TerminalGuard;

    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;

    let mut app = AppState::new();

    // Optional: load file from command line argument
    // Keep stream alive in main — it's not Send so can't go into AppState.
    let mut _stream: Option<cpal::Stream> = None;

    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 2 {
        match load_file(&args[1], &mut app) {
            Ok(stream) => _stream = Some(stream),
            Err(e) => {
                app.status_message = Some(format!("Error: {e}"));
            }
        }
    }

    // Main event loop — ~30 fps
    loop {
        terminal.draw(|frame| {
            layout::render(frame, &app);
        })?;

        if app.should_quit {
            break;
        }

        if event::poll(Duration::from_millis(33))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                if let Some(action) = handle_key_event(key, &mut app) {
                    match action {
                        Action::Quit => break,
                        Action::LoadFile(path) => match load_file(&path, &mut app) {
                            Ok(stream) => {
                                _stream = Some(stream);
                                app.status_message = None;
                            }
                            Err(e) => {
                                app.status_message = Some(format!("Error: {e}"));
                            }
                        },
                    }
                }
            }
        }
    }

    Ok(())
    // _guard Drop restores terminal
}

/// Decode and start playback for a file. Updates app state and returns the cpal Stream.
fn load_file(path: &str, app: &mut AppState) -> Result<cpal::Stream, Box<dyn std::error::Error>> {
    let path = Path::new(path);
    // #14: Pre-check file existence for a clearer error message.
    if !path.exists() {
        return Err(format!("file not found: {}", path.display()).into());
    }
    if !path.is_file() {
        return Err(format!("not a file: {}", path.display()).into());
    }
    let audio_data = audio::decoder::decode_file(path)?;

    let file_info = FileInfo {
        name: path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string(),
        sample_rate: audio_data.sample_rate,
        channels: audio_data.channels,
        duration_secs: audio_data.duration_secs(),
        total_samples: audio_data.samples.len(),
    };

    let audio = Arc::new(audio_data);
    let (stream, state) = audio::playback::start_playback(Arc::clone(&audio))?;

    app.playback = state;
    app.file_info = Some(file_info);
    app.audio_data = Some(audio);

    Ok(stream)
}
