use std::sync::Arc;
use std::thread;

use crossbeam_channel::{Receiver, Sender};

use crate::audio::decoder::AudioData;
use crate::dsp::modifier::{self, WorldSliderValues};
use crate::dsp::world;
use world_sys::WorldParams;

/// Commands sent from the main thread to the processing thread.
pub enum ProcessingCommand {
    Analyze(Arc<AudioData>),
    Resynthesize(WorldSliderValues),
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
    // Always mono so channel count is consistent with WORLD synthesis output.
    let mut original_mono: Option<AudioData> = None;
    let mut sample_rate: u32 = 0;

    while let Ok(cmd) = cmd_rx.recv() {
        match cmd {
            ProcessingCommand::Analyze(audio) => {
                let _ = result_tx.send(ProcessingResult::Status("Analyzing...".into()));
                sample_rate = audio.sample_rate;
                let params = world::analyze(&audio);
                cached_params = Some(params);
                // Store a mono f32 version for the neutral shortcut (no WORLD artifacts).
                let mono = world::to_mono(&audio);
                original_mono = Some(mono.clone());
                let _ = result_tx.send(ProcessingResult::AnalysisDone(mono));
            }
            ProcessingCommand::Resynthesize(values) => {
                if cached_params.is_none() {
                    continue;
                }

                // Drain any queued Resynthesize commands — only process the latest.
                let mut latest = values;
                loop {
                    match cmd_rx.try_recv() {
                        Ok(ProcessingCommand::Resynthesize(newer)) => latest = newer,
                        Ok(ProcessingCommand::Shutdown) => return,
                        Ok(ProcessingCommand::Analyze(audio)) => {
                            // New file loaded while resynthesizing — run analysis instead.
                            // The main thread will auto-send Resynthesize after AnalysisDone.
                            let _ = result_tx
                                .send(ProcessingResult::Status("Analyzing...".into()));
                            sample_rate = audio.sample_rate;
                            let params = world::analyze(&audio);
                            cached_params = Some(params);
                            let mono = world::to_mono(&audio);
                            original_mono = Some(mono.clone());
                            let _ = result_tx.send(ProcessingResult::AnalysisDone(mono));
                            // Skip the stale resynthesize; main will send a fresh one.
                            continue;
                        }
                        Err(_) => break,
                    }
                }

                // Neutral sliders → return mono original to avoid WORLD roundtrip artifacts.
                if latest.is_neutral() {
                    if let Some(ref mono) = original_mono {
                        let _ = result_tx.send(ProcessingResult::SynthesisDone(mono.clone()));
                        continue;
                    }
                }

                let _ = result_tx.send(ProcessingResult::Status("Processing...".into()));
                let params = cached_params.as_ref().unwrap();
                let modified = modifier::apply(params, &latest);
                match world::synthesize(&modified, sample_rate) {
                    Ok(audio) => {
                        let _ = result_tx.send(ProcessingResult::SynthesisDone(audio));
                    }
                    Err(e) => {
                        let _ = result_tx.send(ProcessingResult::Status(format!("Synthesis error: {e}")));
                    }
                }
            }
            ProcessingCommand::Shutdown => break,
        }
    }
}
