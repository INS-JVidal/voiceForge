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

fn processing_loop(cmd_rx: Receiver<ProcessingCommand>, result_tx: Sender<ProcessingResult>) {
    let mut cached_params: Option<WorldParams> = None;
    // Mono version of original audio for neutral-slider shortcut.
    let mut original_mono: Option<AudioData> = None;
    // Post-WORLD synthesis cache for fast effects re-application.
    let mut post_world_audio: Option<AudioData> = None;
    let mut sample_rate: u32 = 0;

    while let Ok(cmd) = cmd_rx.recv() {
        match cmd {
            ProcessingCommand::Analyze(audio) => {
                let _ = result_tx.send(ProcessingResult::Status("Analyzing...".into()));
                sample_rate = audio.sample_rate;
                let params = world::analyze(&audio);
                cached_params = Some(params);
                let mono = world::to_mono(&audio);
                original_mono = Some(mono.clone());
                post_world_audio = Some(mono.clone());
                let _ = result_tx.send(ProcessingResult::AnalysisDone(mono));
            }
            ProcessingCommand::Resynthesize(values, fx_params) => {
                if cached_params.is_none() {
                    continue;
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
                            // Resynthesize will re-apply effects anyway; just take latest params.
                            latest_fx = newer_fx;
                        }
                        Ok(ProcessingCommand::Shutdown) => return,
                        Ok(ProcessingCommand::Analyze(audio)) => {
                            let _ = result_tx
                                .send(ProcessingResult::Status("Analyzing...".into()));
                            sample_rate = audio.sample_rate;
                            let params = world::analyze(&audio);
                            cached_params = Some(params);
                            let mono = world::to_mono(&audio);
                            original_mono = Some(mono.clone());
                            post_world_audio = Some(mono.clone());
                            let _ = result_tx.send(ProcessingResult::AnalysisDone(mono));
                            continue;
                        }
                        Err(_) => break,
                    }
                }

                // Neutral WORLD sliders → use mono original to avoid WORLD artifacts.
                let world_audio = if latest_world.is_neutral() {
                    if let Some(ref mono) = original_mono {
                        mono.clone()
                    } else {
                        continue;
                    }
                } else {
                    let _ = result_tx.send(ProcessingResult::Status("Processing...".into()));
                    let params = cached_params.as_ref().unwrap();
                    let modified = modifier::apply(params, &latest_world);
                    match world::synthesize(&modified, sample_rate) {
                        Ok(audio) => audio,
                        Err(e) => {
                            let _ = result_tx
                                .send(ProcessingResult::Status(format!("Synthesis error: {e}")));
                            continue;
                        }
                    }
                };

                // Cache post-WORLD audio, then apply effects.
                post_world_audio = Some(world_audio.clone());
                let final_audio = apply_fx_chain(&world_audio, &latest_fx);
                let _ = result_tx.send(ProcessingResult::SynthesisDone(final_audio));
            }
            ProcessingCommand::ReapplyEffects(fx_params) => {
                // Drain queued ReapplyEffects, keeping the latest.
                let mut latest_fx = fx_params;
                loop {
                    match cmd_rx.try_recv() {
                        Ok(ProcessingCommand::ReapplyEffects(newer)) => {
                            latest_fx = newer;
                        }
                        Ok(ProcessingCommand::Resynthesize(world_vals, fx_vals)) => {
                            // Full resynthesis supersedes effects-only.
                            if handle_resynthesize_inline(
                                &cmd_rx,
                                &result_tx,
                                world_vals,
                                fx_vals,
                                &mut cached_params,
                                &mut original_mono,
                                &mut post_world_audio,
                                &mut sample_rate,
                            ) {
                                return; // Shutdown was received
                            }
                            continue;
                        }
                        Ok(ProcessingCommand::Shutdown) => return,
                        Ok(ProcessingCommand::Analyze(audio)) => {
                            let _ = result_tx
                                .send(ProcessingResult::Status("Analyzing...".into()));
                            sample_rate = audio.sample_rate;
                            let params = world::analyze(&audio);
                            cached_params = Some(params);
                            let mono = world::to_mono(&audio);
                            original_mono = Some(mono.clone());
                            post_world_audio = Some(mono.clone());
                            let _ = result_tx.send(ProcessingResult::AnalysisDone(mono));
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
            ProcessingCommand::Shutdown => break,
        }
    }
}

/// Apply the effects chain, returning the original unchanged if effects are neutral.
fn apply_fx_chain(audio: &AudioData, params: &EffectsParams) -> AudioData {
    if params.is_neutral() {
        return audio.clone();
    }
    let processed = effects::apply_effects(&audio.samples, audio.sample_rate, params);
    AudioData {
        samples: processed,
        sample_rate: audio.sample_rate,
        channels: audio.channels,
    }
}

/// Handle an inline Resynthesize encountered while draining ReapplyEffects.
/// Returns `true` if a Shutdown command was consumed and the caller should exit.
#[allow(clippy::too_many_arguments)]
fn handle_resynthesize_inline(
    cmd_rx: &Receiver<ProcessingCommand>,
    result_tx: &Sender<ProcessingResult>,
    world_vals: WorldSliderValues,
    fx_vals: EffectsParams,
    cached_params: &mut Option<WorldParams>,
    original_mono: &mut Option<AudioData>,
    post_world_audio: &mut Option<AudioData>,
    sample_rate: &mut u32,
) -> bool {
    if cached_params.is_none() {
        return false;
    }

    // Drain further queued commands.
    let mut latest_world = world_vals;
    let mut latest_fx = fx_vals;
    loop {
        match cmd_rx.try_recv() {
            Ok(ProcessingCommand::Resynthesize(w, fx)) => {
                latest_world = w;
                latest_fx = fx;
            }
            Ok(ProcessingCommand::ReapplyEffects(fx)) => {
                latest_fx = fx;
            }
            Ok(ProcessingCommand::Shutdown) => return true,
            Ok(ProcessingCommand::Analyze(audio)) => {
                let _ = result_tx.send(ProcessingResult::Status("Analyzing...".into()));
                *sample_rate = audio.sample_rate;
                let params = world::analyze(&audio);
                *cached_params = Some(params);
                let mono = world::to_mono(&audio);
                *original_mono = Some(mono.clone());
                *post_world_audio = Some(mono.clone());
                let _ = result_tx.send(ProcessingResult::AnalysisDone(mono));
                return false;
            }
            Err(_) => break,
        }
    }

    let world_audio = if latest_world.is_neutral() {
        if let Some(ref mono) = original_mono {
            mono.clone()
        } else {
            return false;
        }
    } else {
        let _ = result_tx.send(ProcessingResult::Status("Processing...".into()));
        let params = cached_params.as_ref().unwrap();
        let modified = modifier::apply(params, &latest_world);
        match world::synthesize(&modified, *sample_rate) {
            Ok(audio) => audio,
            Err(e) => {
                let _ = result_tx.send(ProcessingResult::Status(format!("Synthesis error: {e}")));
                return false;
            }
        }
    };

    *post_world_audio = Some(world_audio.clone());
    let final_audio = apply_fx_chain(&world_audio, &latest_fx);
    let _ = result_tx.send(ProcessingResult::SynthesisDone(final_audio));
    false
}
