use std::panic;
use std::sync::Arc;
use std::thread;

use crossbeam_channel::{Receiver, Sender};

use crate::audio::decoder::AudioData;
use crate::dsp::effects::{self, EffectsParams};
use crate::dsp::modifier::{self, WorldSliderValues};
use crate::dsp::world;
use world_sys::WorldParams;

/// Commands sent from the main thread to the processing thread.
pub enum ProcessingCommand {
    Analyze(Arc<AudioData>),
    Resynthesize(WorldSliderValues, EffectsParams),
    ReapplyEffects(EffectsParams),
    Shutdown,
}

/// Results sent from the processing thread back to the main thread.
pub enum ProcessingResult {
    AnalysisDone(AudioData),
    SynthesisDone(AudioData),
    Status(String),
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

    /// Shut down the processing thread and wait for it to finish.
    pub fn shutdown(mut self) {
        let _ = self.cmd_tx.send(ProcessingCommand::Shutdown);
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for ProcessingHandle {
    fn drop(&mut self) {
        let _ = self.cmd_tx.send(ProcessingCommand::Shutdown);
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
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
    let _ = result_tx.send(ProcessingResult::Status("Analyzing...".into()));
    *sample_rate = audio.sample_rate;
    match world::analyze(audio) {
        Ok(params) => {
            *cached_params = Some(params);
            let mono = world::to_mono(audio);
            *original_mono = Some(mono.clone());
            *post_world_audio = Some(mono.clone());
            let _ = result_tx.send(ProcessingResult::AnalysisDone(mono));
            true
        }
        Err(e) => {
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
    let world_audio = if latest_world.is_neutral() {
        if let Some(ref mono) = original_mono {
            mono.clone()
        } else {
            return false;
        }
    } else {
        let _ = result_tx.send(ProcessingResult::Status("Processing...".into()));
        // M-11: Use if-let instead of unwrap() for structural safety.
        let params = match cached_params.as_ref() {
            Some(p) => p,
            None => return false,
        };
        let modified = modifier::apply(params, latest_world);
        match world::synthesize(&modified, sample_rate) {
            Ok(audio) => audio,
            Err(e) => {
                let _ = result_tx.send(ProcessingResult::Status(format!("Synthesis error: {e}")));
                return false;
            }
        }
    };

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
        ProcessingCommand::Analyze(audio) => {
            run_analyze(
                &audio, result_tx, sample_rate, cached_params, original_mono, post_world_audio,
            );
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
                    Ok(ProcessingCommand::Analyze(audio)) => {
                        run_analyze(
                            &audio,
                            result_tx,
                            sample_rate,
                            cached_params,
                            original_mono,
                            post_world_audio,
                        );
                        // H-2: Continue draining — don't drop the pending Resynthesize.
                        continue;
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
                                Ok(ProcessingCommand::Analyze(audio)) => {
                                    run_analyze(
                                        &audio,
                                        result_tx,
                                        sample_rate,
                                        cached_params,
                                        original_mono,
                                        post_world_audio,
                                    );
                                    // H-2: Continue draining.
                                    continue;
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
                    Ok(ProcessingCommand::Analyze(audio)) => {
                        run_analyze(
                            &audio,
                            result_tx,
                            sample_rate,
                            cached_params,
                            original_mono,
                            post_world_audio,
                        );
                        continue;
                    }
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
