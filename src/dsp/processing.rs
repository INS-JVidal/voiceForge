use std::panic;
use std::path::Path;
use std::sync::Arc;
use std::thread;

use crossbeam_channel::{Receiver, Sender};

use crate::audio::decoder::{self, AudioData};
use crate::dsp::effects::{self, EffectsParams};
use crate::dsp::modifier::{self, WorldSliderValues};
use crate::dsp::world;
use world_sys::WorldParams;

/// Commands sent from the main thread to the processing thread.
pub enum ProcessingCommand {
    Load(String),                                      // path to decode
    ScanDirectory(String),                             // path prefix as typed
    PrecheckAudio(String),                             // path to validate
    Resynthesize(WorldSliderValues, EffectsParams),
    ReapplyEffects(EffectsParams),
    Shutdown,
}

/// Results sent from the processing thread back to the main thread.
pub enum ProcessingResult {
    AudioReady(AudioData, String),                    // decoded audio + path that was decoded
    AnalysisDone(AudioData),
    SynthesisDone(AudioData),
    Status(String),
    DirectoryListing(String, Vec<String>),            // (input_prefix_echo, sorted entries)
    AudioPrecheckDone(String),                        // path is valid audio
    AudioPrecheckFailed(String, String),              // (path, error message)
}

/// Handle for communicating with the processing thread.
pub struct ProcessingHandle {
    cmd_tx: Sender<ProcessingCommand>,
    result_rx: Receiver<ProcessingResult>,
    thread: Option<thread::JoinHandle<()>>,
}

impl ProcessingHandle {
    /// Spawn the processing thread and return a handle.
    pub fn spawn() -> Self {
        let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded();
        let (result_tx, result_rx) = crossbeam_channel::unbounded();

        let thread = thread::spawn(move || {
            processing_loop(cmd_rx, result_tx);
        });

        Self {
            cmd_tx,
            result_rx,
            thread: Some(thread),
        }
    }

    /// Send a command to the processing thread.
    pub fn send(&self, cmd: ProcessingCommand) {
        let _ = self.cmd_tx.send(cmd);
    }

    /// Try to receive a result without blocking.
    pub fn try_recv(&self) -> Option<ProcessingResult> {
        self.result_rx.try_recv().ok()
    }

}

impl Drop for ProcessingHandle {
    fn drop(&mut self) {
        // Always detach the thread without joining. This ensures that any exit path
        // (normal 'q' quit, Ctrl+C, or error propagation) immediately frees the UI
        // for terminal restoration. The main thread sends Shutdown before this Drop runs.
        // The processing thread will exit cleanly when it finishes the current command
        // or when the receiver side of the channel closes.
        self.thread.take(); // Drop JoinHandle → detaches, does NOT join
    }
}

/// Run WORLD analysis and update cached state. Returns `true` on success.
fn run_analyze(
    audio: &AudioData,
    result_tx: &Sender<ProcessingResult>,
    sample_rate: &mut u32,
    cached_params: &mut Option<WorldParams>,
    original_mono: &mut Option<AudioData>,
    post_world_audio: &mut Option<AudioData>,
) -> bool {
    *sample_rate = audio.sample_rate;
    log::info!("analyze: {} samples @ {}Hz", audio.samples.len(), audio.sample_rate);
    let result_tx_clone = result_tx.clone();
    match world::analyze_with_progress(audio, move |pct| {
        let _ = result_tx_clone.send(ProcessingResult::Status(format!("Analyzing... {pct}%")));
    }) {
        Ok(params) => {
            log::info!("analyze: done — {} f0 frames", params.f0.len());
            *cached_params = Some(params);
            let mono = world::to_mono(audio);
            *original_mono = Some(mono.clone());
            *post_world_audio = Some(mono.clone());
            let _ = result_tx.send(ProcessingResult::AnalysisDone(mono));
            true
        }
        Err(e) => {
            log::error!("analyze: failed — {e}");
            let _ = result_tx.send(ProcessingResult::Status(format!("Analysis error: {e}")));
            false
        }
    }
}

/// Run resynthesis with given WORLD and effects params. Returns the result audio or None.
fn run_resynthesize(
    latest_world: &WorldSliderValues,
    latest_fx: &EffectsParams,
    cached_params: &Option<WorldParams>,
    original_mono: &Option<AudioData>,
    post_world_audio: &mut Option<AudioData>,
    sample_rate: u32,
    result_tx: &Sender<ProcessingResult>,
) -> bool {
    log::debug!("resynthesize: starting");
    let world_audio = if latest_world.bypass || latest_world.is_neutral() {
        if let Some(ref mono) = original_mono {
            mono.clone()
        } else {
            return false;
        }
    } else {
        // Stage 1: Modify parameters
        let _ = result_tx.send(ProcessingResult::Status("Modifying parameters... (1/3)".into()));
        // M-11: Use if-let instead of unwrap() for structural safety.
        let params = match cached_params.as_ref() {
            Some(p) => p,
            None => return false,
        };
        let modified = modifier::apply(params, latest_world);

        // Stage 2: Synthesize voice
        let _ = result_tx.send(ProcessingResult::Status("Synthesizing voice... (2/3)".into()));
        match world::synthesize(&modified, sample_rate) {
            Ok(audio) => audio,
            Err(e) => {
                log::error!("resynthesize: failed — {e}");
                let _ = result_tx.send(ProcessingResult::Status(format!("Synthesis error: {e}")));
                return false;
            }
        }
    };

    // Stage 3: Apply effects
    let _ = result_tx.send(ProcessingResult::Status("Applying effects... (3/3)".into()));
    *post_world_audio = Some(world_audio.clone());
    let final_audio = apply_fx_chain(&world_audio, latest_fx);
    let _ = result_tx.send(ProcessingResult::SynthesisDone(final_audio));
    true
}

fn processing_loop(cmd_rx: Receiver<ProcessingCommand>, result_tx: Sender<ProcessingResult>) {
    let mut cached_params: Option<WorldParams> = None;
    let mut original_mono: Option<AudioData> = None;
    let mut post_world_audio: Option<AudioData> = None;
    let mut sample_rate: u32 = 0;

    while let Ok(cmd) = cmd_rx.recv() {
        // CR-1: Wrap each command in catch_unwind so a panic sends an error
        // status instead of silently killing the processing thread.
        let result_tx_panic = result_tx.clone();
        let panicked = panic::catch_unwind(panic::AssertUnwindSafe(|| {
            handle_command(
                cmd,
                &cmd_rx,
                &result_tx,
                &mut cached_params,
                &mut original_mono,
                &mut post_world_audio,
                &mut sample_rate,
            )
        }));

        match panicked {
            Ok(should_exit) => {
                if should_exit {
                    return;
                }
            }
            Err(e) => {
                let msg = if let Some(s) = e.downcast_ref::<&str>() {
                    format!("Internal error: {s}")
                } else if let Some(s) = e.downcast_ref::<String>() {
                    format!("Internal error: {s}")
                } else {
                    "Internal error: processing thread panicked".to_string()
                };
                log::error!("processing thread caught panic: {msg}");
                let _ = result_tx_panic.send(ProcessingResult::Status(msg));
                cached_params = None;
                original_mono = None;
                post_world_audio = None;
            }
        }
    }
}

/// Handle a single processing command. Returns `true` if the thread should exit.
fn handle_command(
    cmd: ProcessingCommand,
    cmd_rx: &Receiver<ProcessingCommand>,
    result_tx: &Sender<ProcessingResult>,
    cached_params: &mut Option<WorldParams>,
    original_mono: &mut Option<AudioData>,
    post_world_audio: &mut Option<AudioData>,
    sample_rate: &mut u32,
) -> bool {
    match cmd {
        ProcessingCommand::Load(path) => {
            run_load_file(
                path,
                result_tx,
                sample_rate,
                cached_params,
                original_mono,
                post_world_audio,
            );
        }
        ProcessingCommand::ScanDirectory(prefix) => {
            let entries = scan_directory_entries(&prefix);
            let _ = result_tx.send(ProcessingResult::DirectoryListing(prefix, entries));
        }
        ProcessingCommand::PrecheckAudio(path) => {
            match precheck_audio_file(&path) {
                Ok(()) => {
                    let _ = result_tx.send(ProcessingResult::AudioPrecheckDone(path));
                }
                Err(e) => {
                    let _ = result_tx.send(ProcessingResult::AudioPrecheckFailed(path, e));
                }
            }
        }
        ProcessingCommand::Resynthesize(values, fx_params) => {
            if cached_params.is_none() {
                return false;
            }

            // Drain any queued commands — only process the latest.
            let mut latest_world = values;
            let mut latest_fx = fx_params;
            loop {
                match cmd_rx.try_recv() {
                    Ok(ProcessingCommand::Resynthesize(newer_w, newer_fx)) => {
                        latest_world = newer_w;
                        latest_fx = newer_fx;
                    }
                    Ok(ProcessingCommand::ReapplyEffects(newer_fx)) => {
                        latest_fx = newer_fx;
                    }
                    Ok(ProcessingCommand::Shutdown) => return true,
                    Ok(ProcessingCommand::Load(path)) => {
                        run_load_file(
                            path,
                            result_tx,
                            sample_rate,
                            cached_params,
                            original_mono,
                            post_world_audio,
                        );
                        // Don't continue draining — new file loaded, abort pending resynth
                        return false;
                    }
                    Ok(ProcessingCommand::ScanDirectory(prefix)) => {
                        let entries = scan_directory_entries(&prefix);
                        let _ = result_tx.send(ProcessingResult::DirectoryListing(prefix, entries));
                        // Continue draining — fast I/O
                    }
                    Ok(ProcessingCommand::PrecheckAudio(path)) => {
                        match precheck_audio_file(&path) {
                            Ok(()) => {
                                let _ = result_tx.send(ProcessingResult::AudioPrecheckDone(path));
                            }
                            Err(e) => {
                                let _ = result_tx.send(ProcessingResult::AudioPrecheckFailed(path, e));
                            }
                        }
                        // Continue draining — fast I/O
                    }
                    Err(_) => break,
                }
            }

            run_resynthesize(
                &latest_world,
                &latest_fx,
                cached_params,
                original_mono,
                post_world_audio,
                *sample_rate,
                result_tx,
            );
        }
        ProcessingCommand::ReapplyEffects(fx_params) => {
            let mut latest_fx = fx_params;
            loop {
                match cmd_rx.try_recv() {
                    Ok(ProcessingCommand::ReapplyEffects(newer)) => {
                        latest_fx = newer;
                    }
                    Ok(ProcessingCommand::Load(path)) => {
                        run_load_file(
                            path,
                            result_tx,
                            sample_rate,
                            cached_params,
                            original_mono,
                            post_world_audio,
                        );
                        return false;
                    }
                    Ok(ProcessingCommand::ScanDirectory(prefix)) => {
                        let entries = scan_directory_entries(&prefix);
                        let _ = result_tx.send(ProcessingResult::DirectoryListing(prefix, entries));
                        // Continue draining — fast I/O
                    }
                    Ok(ProcessingCommand::PrecheckAudio(path)) => {
                        match precheck_audio_file(&path) {
                            Ok(()) => {
                                let _ = result_tx.send(ProcessingResult::AudioPrecheckDone(path));
                            }
                            Err(e) => {
                                let _ = result_tx.send(ProcessingResult::AudioPrecheckFailed(path, e));
                            }
                        }
                        // Continue draining — fast I/O
                    }
                    Ok(ProcessingCommand::Resynthesize(world_vals, fx_vals)) => {
                        // Full resynthesis supersedes effects-only.
                        // Drain further and run resynthesize.
                        let mut lw = world_vals;
                        let mut lf = fx_vals;
                        loop {
                            match cmd_rx.try_recv() {
                                Ok(ProcessingCommand::Resynthesize(w, fx)) => {
                                    lw = w;
                                    lf = fx;
                                }
                                Ok(ProcessingCommand::ReapplyEffects(fx)) => {
                                    lf = fx;
                                }
                                Ok(ProcessingCommand::Shutdown) => return true,
                                Ok(ProcessingCommand::Load(path)) => {
                                    run_load_file(
                                        path,
                                        result_tx,
                                        sample_rate,
                                        cached_params,
                                        original_mono,
                                        post_world_audio,
                                    );
                                    return false;
                                }
                                Ok(ProcessingCommand::ScanDirectory(prefix)) => {
                                    let entries = scan_directory_entries(&prefix);
                                    let _ = result_tx.send(ProcessingResult::DirectoryListing(prefix, entries));
                                    // Continue draining — fast I/O
                                }
                                Ok(ProcessingCommand::PrecheckAudio(path)) => {
                                    match precheck_audio_file(&path) {
                                        Ok(()) => {
                                            let _ = result_tx.send(ProcessingResult::AudioPrecheckDone(path));
                                        }
                                        Err(e) => {
                                            let _ = result_tx.send(ProcessingResult::AudioPrecheckFailed(path, e));
                                        }
                                    }
                                    // Continue draining — fast I/O
                                }
                                Err(_) => break,
                            }
                        }
                        if cached_params.is_some() {
                            run_resynthesize(
                                &lw,
                                &lf,
                                cached_params,
                                original_mono,
                                post_world_audio,
                                *sample_rate,
                                result_tx,
                            );
                        }
                        return false;
                    }
                    Ok(ProcessingCommand::Shutdown) => return true,
                    Err(_) => break,
                }
            }

            if let Some(ref cached) = post_world_audio {
                let final_audio = apply_fx_chain(cached, &latest_fx);
                let _ = result_tx.send(ProcessingResult::SynthesisDone(final_audio));
            }
        }
        ProcessingCommand::Shutdown => return true,
    }
    false
}

/// Scan directory entries matching a given input prefix.
fn scan_directory_entries(input: &str) -> Vec<String> {
    use std::env;
    use std::fs;

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

    matches.into_iter().map(|(_, path)| path).collect()
}

/// Check if a file appears to be a valid audio file by examining its magic bytes.
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

/// Run load (decode + analyze) for a file.
fn run_load_file(
    path: String,
    result_tx: &Sender<ProcessingResult>,
    sample_rate: &mut u32,
    cached_params: &mut Option<WorldParams>,
    original_mono: &mut Option<AudioData>,
    post_world_audio: &mut Option<AudioData>,
) {
    let _ = result_tx.send(ProcessingResult::Status("Decoding...".into()));
    let tx = result_tx.clone();
    match decoder::decode_file_with_progress(Path::new(&path), move |pct| {
        let _ = tx.send(ProcessingResult::Status(format!("Decoding... {pct}%")));
    }) {
        Ok(audio_data) => {
            let audio = Arc::new(audio_data.clone());
            let _ = result_tx.send(ProcessingResult::AudioReady(audio_data, path));
            run_analyze(
                &audio,
                result_tx,
                sample_rate,
                cached_params,
                original_mono,
                post_world_audio,
            );
        }
        Err(e) => {
            log::error!("load: failed — {e}");
            let _ = result_tx.send(ProcessingResult::Status(format!("Load error: {e}")));
        }
    }
}

/// Apply the effects chain, returning the original unchanged if effects are neutral.
fn apply_fx_chain(audio: &AudioData, params: &EffectsParams) -> AudioData {
    if params.is_neutral() {
        return audio.clone();
    }
    // H-7: Effects processing assumes mono input (single filter state).
    // WORLD always outputs mono, so this is safe. Document the precondition.
    debug_assert!(
        audio.channels == 1,
        "apply_fx_chain expects mono audio, got {} channels",
        audio.channels
    );
    let processed = effects::apply_effects(&audio.samples, audio.sample_rate, params);
    AudioData {
        samples: processed,
        sample_rate: audio.sample_rate,
        channels: audio.channels,
    }
}
