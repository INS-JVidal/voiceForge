use std::io::{self, stdout};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{self, Event, KeyEventKind};
use ratatui::crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::crossterm::ExecutableCommand;
use ratatui::prelude::CrosstermBackend;
use ratatui::Terminal;

use voiceforge::app::{Action, AppState, FileInfo};
use voiceforge::audio;
use voiceforge::dsp::processing::{ProcessingCommand, ProcessingHandle, ProcessingResult};
use voiceforge::input::handler::handle_key_event;
use voiceforge::ui::layout;

/// RAII guard that restores the terminal on drop (including panics).
struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = stdout().execute(LeaveAlternateScreen);
    }
}

/// Debounce delay for resynthesize commands.
const RESYNTH_DEBOUNCE: Duration = Duration::from_millis(150);

fn main() -> io::Result<()> {
    // Set up terminal
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    let _guard = TerminalGuard;

    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;

    let mut app = AppState::new();

    // Spawn processing thread
    let processing = ProcessingHandle::spawn();

    // Keep stream alive in main — it's not Send so can't go into AppState.
    let mut _stream: Option<cpal::Stream> = None;

    // Debounce timer for resynthesize
    let mut resynth_pending: Option<Instant> = None;

    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 2 {
        match load_file(&args[1], &mut app) {
            Ok(stream) => {
                _stream = Some(stream);
                // Send audio for WORLD analysis
                if let Some(ref audio) = app.audio_data {
                    processing.send(ProcessingCommand::Analyze(Arc::clone(audio)));
                }
            }
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

        // Poll for processing results (non-blocking)
        while let Some(result) = processing.try_recv() {
            match result {
                ProcessingResult::AnalysisDone => {
                    app.processing_status = None;
                    // Auto-resynthesize with current slider values
                    let values = app.world_slider_values();
                    processing.send(ProcessingCommand::Resynthesize(values));
                }
                ProcessingResult::SynthesisDone(audio_data) => {
                    app.processing_status = None;
                    let new_audio = Arc::new(audio_data);

                    // Adjust playback position for channel count changes
                    // (e.g. stereo original → mono after WORLD synthesis).
                    if let Some(ref mut info) = app.file_info {
                        let old_channels = info.channels as usize;
                        let new_channels = new_audio.channels as usize;

                        if old_channels != new_channels
                            && old_channels > 0
                            && new_channels > 0
                        {
                            let current_pos = app
                                .playback
                                .position
                                .load(std::sync::atomic::Ordering::Relaxed);
                            // Convert interleaved position → frame → new interleaved position
                            let frame = current_pos / old_channels;
                            let new_pos =
                                (frame * new_channels).min(new_audio.samples.len());
                            app.playback
                                .position
                                .store(new_pos, std::sync::atomic::Ordering::Relaxed);
                        }

                        info.channels = new_audio.channels;
                        info.total_samples = new_audio.samples.len();
                        info.duration_secs = new_audio.duration_secs();
                    }

                    // Clamp position if it's beyond the new buffer
                    let max_samples = new_audio.samples.len();
                    let current_pos = app
                        .playback
                        .position
                        .load(std::sync::atomic::Ordering::Relaxed);
                    if current_pos > max_samples {
                        app.playback
                            .position
                            .store(max_samples, std::sync::atomic::Ordering::Relaxed);
                    }

                    // Rebuild the cpal stream with new audio
                    match audio::playback::rebuild_stream(Arc::clone(&new_audio), &app.playback) {
                        Ok(stream) => {
                            _stream = Some(stream);
                            app.audio_data = Some(new_audio);
                        }
                        Err(e) => {
                            app.status_message = Some(format!("Playback error: {e}"));
                        }
                    }
                }
                ProcessingResult::Status(msg) => {
                    app.processing_status = Some(msg);
                }
            }
        }

        // Check debounce timer
        if let Some(deadline) = resynth_pending {
            if Instant::now() >= deadline {
                resynth_pending = None;
                let values = app.world_slider_values();
                processing.send(ProcessingCommand::Resynthesize(values));
            }
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
                                // Send audio for WORLD analysis
                                if let Some(ref audio) = app.audio_data {
                                    processing
                                        .send(ProcessingCommand::Analyze(Arc::clone(audio)));
                                }
                            }
                            Err(e) => {
                                app.status_message = Some(format!("Error: {e}"));
                            }
                        },
                        Action::Resynthesize => {
                            // Debounce: reset timer on each slider change
                            resynth_pending = Some(Instant::now() + RESYNTH_DEBOUNCE);
                        }
                    }
                }
            }
        }
    }

    processing.shutdown();

    Ok(())
    // _guard Drop restores terminal
}

/// Decode and start playback for a file. Updates app state and returns the cpal Stream.
fn load_file(path: &str, app: &mut AppState) -> Result<cpal::Stream, Box<dyn std::error::Error>> {
    let path = Path::new(path);
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
