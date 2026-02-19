use std::io::{self, stdout};
use std::path::Path;
use std::sync::atomic::Ordering;
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
use voiceforge::dsp::spectrum::{compute_spectrum, extract_window, FFT_SIZE};
use voiceforge::input::handler::handle_key_event;
use voiceforge::ui::layout;

/// Initialize file-based logging. All output goes to `voiceforge.log` — never to
/// stderr/stdout — so the ratatui TUI is never corrupted.
fn setup_logger() -> Result<(), fern::InitError> {
    fern::Dispatch::new()
        .format(|out, message, record| {
            let secs = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            out.finish(format_args!(
                "[{secs}] [{level:<5}] [{target}] {message}",
                level = record.level(),
                target = record.target(),
            ))
        })
        .level(if cfg!(debug_assertions) {
            log::LevelFilter::Debug
        } else {
            log::LevelFilter::Info
        })
        .chain(fern::log_file("voiceforge.log")?)
        .apply()?;
    Ok(())
}

/// RAII guard that restores the terminal on drop (including panics).
struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        if let Err(e) = disable_raw_mode() {
            log::warn!("failed to disable raw mode: {e}");
        }
        if let Err(e) = stdout().execute(LeaveAlternateScreen) {
            log::warn!("failed to leave alternate screen: {e}");
        }
    }
}

/// Debounce delay for resynthesize / effects commands.
const RESYNTH_DEBOUNCE: Duration = Duration::from_millis(150);
const EFFECTS_DEBOUNCE: Duration = Duration::from_millis(80);

fn main() -> io::Result<()> {
    // Initialize logging before anything else. If it fails (e.g., can't create
    // the log file), silently continue — the app should not abort for logging.
    let _ = setup_logger();

    // L-1: Register SIGINT handler so we exit cleanly (TerminalGuard::drop runs).
    let sigint = Arc::new(std::sync::atomic::AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&sigint))
        .expect("failed to register SIGINT handler");

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

    // Debounce timers for resynthesize and effects
    let mut resynth_pending: Option<Instant> = None;
    let mut effects_pending: Option<Instant> = None;

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
                app.set_status(format!("Error: {e}"));
            }
        }
    }

    // L-12: Status message auto-clear timeout.
    const STATUS_TIMEOUT: Duration = Duration::from_secs(5);

    // Main event loop — ~30 fps
    loop {
        // L-12: Auto-clear status message after timeout.
        if let Some(t) = app.status_message_time {
            if t.elapsed() >= STATUS_TIMEOUT {
                app.status_message = None;
                app.status_message_time = None;
            }
        }

        // Sync loop toggle to audio callback atomic.
        app.playback
            .loop_enabled
            .store(app.loop_enabled, Ordering::Relaxed);

        // Update spectrum bins from current playback position
        if app.playback.playing.load(Ordering::Acquire) {
            if let Some(ref lock) = app.playback.audio_lock {
                match lock.try_read() {
                    Ok(guard) => {
                        let pos = app.playback.position.load(Ordering::Acquire);
                        let window = extract_window(&guard, pos, FFT_SIZE);

                        app.spectrum_bins = compute_spectrum(&window, FFT_SIZE);
                    }
                    Err(_) => {
                        // try_read failed — spectrum will not update until lock is available
                    }
                }
            }
        }

        terminal.draw(|frame| {
            layout::render(frame, &mut app);
        })?;

        if app.should_quit || sigint.load(Ordering::Relaxed) {
            break;
        }

        // Poll for processing results (non-blocking)
        while let Some(result) = processing.try_recv() {
            match result {
                ProcessingResult::AnalysisDone(mono_original) => {
                    app.processing_status = None;
                    app.original_audio = Some(Arc::new(mono_original));
                    // Auto-resynthesize with current slider values
                    let values = app.world_slider_values();
                    let fx = app.effects_params();
                    processing.send(ProcessingCommand::Resynthesize(values, fx));
                }
                ProcessingResult::SynthesisDone(audio_data) => {
                    app.processing_status = None;
                    app.status_message = None;
                    let new_audio = Arc::new(audio_data);

                    if app.ab_original {
                        // User is listening to original — just store the new
                        // processed audio without touching the stream.
                        app.audio_data = Some(new_audio);
                    } else {
                        // User is on B (processed) — swap or rebuild.

                        // Adjust playback position for channel count changes
                        // (e.g. stereo original → mono after WORLD synthesis).
                        if let Some(ref mut info) = app.file_info {
                            let old_channels = info.channels as usize;
                            let new_channels = new_audio.channels as usize;

                            if old_channels != new_channels
                                && old_channels > 0
                                && new_channels > 0
                            {
                                let current_pos =
                                    app.playback.position.load(Ordering::Acquire);
                                let frame = current_pos / old_channels;
                                let new_pos =
                                    (frame * new_channels).min(new_audio.samples.len());
                                app.playback.position.store(new_pos, Ordering::Release);
                            }

                            info.channels = new_audio.channels;
                            info.total_samples = new_audio.samples.len();
                            info.duration_secs = new_audio.duration_secs();
                        }

                        // H-4: Clamp position inside swap_audio's write-lock to avoid TOCTOU.
                        let max_samples = new_audio.samples.len();
                        let current_pos = app.playback.position.load(Ordering::Acquire);
                        let clamped_pos = current_pos.min(max_samples);

                        // Swap audio in running stream if we have a lock, else rebuild
                        if let Some(ref lock) = app.playback.audio_lock {
                            audio::playback::swap_audio(
                                lock,
                                Arc::clone(&new_audio),
                                Some((&app.playback.position, clamped_pos)),
                            );
                            app.audio_data = Some(new_audio);
                        } else {
                            match audio::playback::rebuild_stream(
                                Arc::clone(&new_audio),
                                &mut app.playback,
                            ) {
                                Ok(stream) => {
                                    _stream = Some(stream);
                                    app.audio_data = Some(new_audio);
                                }
                                Err(e) => {
                                    app.set_status(format!("Playback error: {e}"));
                                }
                            }
                        }
                    }
                }
                ProcessingResult::Status(msg) => {
                    app.processing_status = Some(msg);
                }
            }
        }

        // Check debounce timers
        if let Some(deadline) = resynth_pending {
            if Instant::now() >= deadline {
                resynth_pending = None;
                effects_pending = None; // Resynthesize includes effects
                let values = app.world_slider_values();
                let fx = app.effects_params();
                processing.send(ProcessingCommand::Resynthesize(values, fx));
            }
        }
        if let Some(deadline) = effects_pending {
            if Instant::now() >= deadline {
                effects_pending = None;
                let fx = app.effects_params();
                processing.send(ProcessingCommand::ReapplyEffects(fx));
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
                                app.spectrum_bins.clear();
                                // M-1: Reset debounce timers on file load to prevent
                                // stale Resynthesize from firing before Analyze completes.
                                resynth_pending = None;
                                effects_pending = None;
                                // Reset A/B state for new file
                                app.ab_original = false;
                                app.original_audio = None;
                                // Send audio for WORLD analysis
                                if let Some(ref audio) = app.audio_data {
                                    processing
                                        .send(ProcessingCommand::Analyze(Arc::clone(audio)));
                                }
                            }
                            Err(e) => {
                                app.set_status(format!("Error: {e}"));
                            }
                        },
                        Action::Resynthesize => {
                            // Debounce: reset timer on each slider change
                            resynth_pending = Some(Instant::now() + RESYNTH_DEBOUNCE);
                        }
                        Action::ReapplyEffects => {
                            effects_pending = Some(Instant::now() + EFFECTS_DEBOUNCE);
                        }
                        Action::LiveGain(linear) => {
                            app.playback
                                .live_gain
                                .store(linear.to_bits(), std::sync::atomic::Ordering::Relaxed);
                        }
                        Action::ExportWav(dest_path) => {
                            // Export what the user is hearing: original when
                            // A/B is on original, processed otherwise.
                            let source = if app.ab_original {
                                app.original_audio.as_ref()
                            } else {
                                app.audio_data.as_ref()
                            };
                            if let Some(audio) = source {
                                let mut samples = audio.samples.clone();
                                // Bake live gain (not stored in audio buffer).
                                let gain_db = app.effects_sliders[0].value as f32;
                                if gain_db != 0.0 {
                                    voiceforge::dsp::effects::apply_gain(
                                        &mut samples,
                                        gain_db,
                                    );
                                }
                                match audio::export::export_wav(
                                    &samples,
                                    audio.sample_rate,
                                    audio.channels,
                                    Path::new(&dest_path),
                                ) {
                                    Ok(()) => {
                                        app.set_status(format!("Saved: {dest_path}"));
                                    }
                                    Err(e) => {
                                        app.set_status(format!("Export error: {e}"));
                                    }
                                }
                            }
                        }
                        Action::ToggleAB => {
                            // ab_original was already flipped by the handler
                            if let Some(ref lock) = app.playback.audio_lock {
                                let target = if app.ab_original {
                                    app.original_audio.as_ref()
                                } else {
                                    app.audio_data.as_ref()
                                };
                                if let Some(audio) = target {
                                    // Scale position proportionally if buffer lengths differ
                                    // CR-2: Recover from poisoned lock.
                                    let old_len = {
                                        let guard =
                                            lock.read().unwrap_or_else(|e| e.into_inner());
                                        guard.samples.len()
                                    };
                                    let new_len = audio.samples.len();
                                    let new_pos = if old_len != new_len {
                                        let pos =
                                            app.playback.position.load(Ordering::Acquire);
                                        let fraction = if old_len > 0 {
                                            pos as f64 / old_len as f64
                                        } else {
                                            0.0
                                        };
                                        (fraction * new_len as f64).round().min(new_len as f64) as usize
                                    } else {
                                        let pos = app.playback.position.load(Ordering::Acquire);
                                        pos.min(new_len)
                                    };

                                    // Update file_info for the active buffer
                                    if let Some(ref mut info) = app.file_info {
                                        info.total_samples = audio.samples.len();
                                        info.duration_secs = audio.duration_secs();
                                        info.channels = audio.channels;
                                    }

                                    // H-4: Position clamp inside swap_audio's write-lock
                                    audio::playback::swap_audio(
                                        lock,
                                        Arc::clone(audio),
                                        Some((&app.playback.position, new_pos)),
                                    );
                                }
                            }
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
        path: path.to_string_lossy().into_owned(),
        sample_rate: audio_data.sample_rate,
        channels: audio_data.channels,
        original_channels: audio_data.channels,
        duration_secs: audio_data.duration_secs(),
        total_samples: audio_data.samples.len(),
    };

    let audio = Arc::new(audio_data);
    let (stream, state) = audio::playback::start_playback(Arc::clone(&audio))?;

    app.playback = state;
    // Restore live gain from current slider value (new PlaybackState defaults to 1.0).
    let gain_db = app.effects_sliders[0].value as f32;
    app.playback
        .live_gain
        .store(10.0_f32.powf(gain_db / 20.0).to_bits(), std::sync::atomic::Ordering::Relaxed);
    // Restore loop state (new PlaybackState defaults to false).
    app.playback
        .loop_enabled
        .store(app.loop_enabled, std::sync::atomic::Ordering::Relaxed);
    app.file_info = Some(file_info);
    app.audio_data = Some(audio);

    Ok(stream)
}
