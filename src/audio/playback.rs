use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream, StreamConfig};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use super::decoder::AudioData;

/// Shared playback state between the main thread and the audio callback.
#[derive(Debug)]
pub struct PlaybackState {
    /// Whether audio is currently playing (vs paused).
    pub playing: Arc<AtomicBool>,
    /// Current sample position in the interleaved buffer.
    pub position: Arc<AtomicUsize>,
}

impl Default for PlaybackState {
    fn default() -> Self {
        Self {
            playing: Arc::new(AtomicBool::new(false)),
            position: Arc::new(AtomicUsize::new(0)),
        }
    }
}

impl PlaybackState {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Toggle play/pause. Returns the new playing state.
    pub fn toggle_playing(&self) -> bool {
        // fetch_xor is atomic — no TOCTOU race with the audio callback.
        !self.playing.fetch_xor(true, Ordering::Relaxed)
    }

    /// Seek by a signed sample offset, clamped to [0, max_samples].
    pub fn seek_by_samples(&self, offset: isize, max_samples: usize) {
        let current = self.position.load(Ordering::Relaxed);
        let new_pos = (current as isize + offset).clamp(0, max_samples as isize) as usize;
        self.position.store(new_pos, Ordering::Relaxed);
    }

    /// Seek by seconds. `channels` is needed to convert to interleaved sample offset.
    pub fn seek_by_secs(&self, secs: f64, sample_rate: u32, channels: u16, max_samples: usize) {
        let offset = (secs * sample_rate as f64 * channels as f64) as isize;
        self.seek_by_samples(offset, max_samples);
    }

    /// Current playback time in seconds.
    #[must_use]
    pub fn current_time_secs(&self, sample_rate: u32, channels: u16) -> f64 {
        let pos = self.position.load(Ordering::Relaxed);
        if sample_rate == 0 || channels == 0 {
            return 0.0;
        }
        pos as f64 / (sample_rate as f64 * channels as f64)
    }
}

/// Error starting audio playback.
#[derive(Debug)]
pub struct PlaybackError(String);

impl std::fmt::Display for PlaybackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "playback error: {}", self.0)
    }
}

impl std::error::Error for PlaybackError {}

/// Context shared with the audio callback closure.
struct CallbackContext {
    audio: Arc<AudioData>,
    playing: Arc<AtomicBool>,
    position: Arc<AtomicUsize>,
    total_samples: usize,
    audio_channels: u16,
    device_channels: u16,
}

/// Start audio playback on the default output device.
///
/// Returns the cpal `Stream` (must be kept alive for playback to continue)
/// and the shared `PlaybackState` for controlling playback.
pub fn start_playback(
    audio: Arc<AudioData>,
) -> Result<(Stream, PlaybackState), PlaybackError> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| PlaybackError("no output audio device found".into()))?;

    let supported_config = device
        .default_output_config()
        .map_err(|e| PlaybackError(format!("no supported output config: {e}")))?;

    let sample_format = supported_config.sample_format();
    let config: StreamConfig = supported_config.into();

    let state = PlaybackState::new();

    let ctx = CallbackContext {
        playing: Arc::clone(&state.playing),
        position: Arc::clone(&state.position),
        total_samples: audio.samples.len(),
        audio_channels: audio.channels,
        device_channels: config.channels,
        audio,
    };

    let stream = match sample_format {
        SampleFormat::F32 => build_stream::<f32>(&device, &config, ctx)?,
        SampleFormat::I16 => build_stream::<i16>(&device, &config, ctx)?,
        SampleFormat::U16 => build_stream::<u16>(&device, &config, ctx)?,
        other => {
            return Err(PlaybackError(format!(
                "unsupported sample format: {other:?}"
            )));
        }
    };

    stream
        .play()
        .map_err(|e| PlaybackError(format!("failed to start stream: {e}")))?;

    state.playing.store(true, Ordering::Relaxed);

    Ok((stream, state))
}

fn build_stream<T: cpal::SizedSample + cpal::FromSample<f32>>(
    device: &cpal::Device,
    config: &StreamConfig,
    ctx: CallbackContext,
) -> Result<Stream, PlaybackError> {
    let stream = device
        .build_output_stream(
            config,
            move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
                write_audio_data(data, &ctx);
            },
            |err| eprintln!("audio stream error: {err}"),
            None,
        )
        .map_err(|e| PlaybackError(format!("failed to build stream: {e}")))?;

    Ok(stream)
}

/// Rebuild the cpal output stream with new audio data, reusing the existing
/// `PlaybackState` atomics (position and playing state are preserved).
pub fn rebuild_stream(
    audio: Arc<AudioData>,
    state: &PlaybackState,
) -> Result<Stream, PlaybackError> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| PlaybackError("no output audio device found".into()))?;

    let supported_config = device
        .default_output_config()
        .map_err(|e| PlaybackError(format!("no supported output config: {e}")))?;

    let sample_format = supported_config.sample_format();
    let config: StreamConfig = supported_config.into();

    let ctx = CallbackContext {
        playing: Arc::clone(&state.playing),
        position: Arc::clone(&state.position),
        total_samples: audio.samples.len(),
        audio_channels: audio.channels,
        device_channels: config.channels,
        audio,
    };

    let stream = match sample_format {
        SampleFormat::F32 => build_stream::<f32>(&device, &config, ctx)?,
        SampleFormat::I16 => build_stream::<i16>(&device, &config, ctx)?,
        SampleFormat::U16 => build_stream::<u16>(&device, &config, ctx)?,
        other => {
            return Err(PlaybackError(format!(
                "unsupported sample format: {other:?}"
            )));
        }
    };

    stream
        .play()
        .map_err(|e| PlaybackError(format!("failed to start stream: {e}")))?;

    Ok(stream)
}

fn write_audio_data<T: cpal::SizedSample + cpal::FromSample<f32>>(
    output: &mut [T],
    ctx: &CallbackContext,
) {
    if !ctx.playing.load(Ordering::Relaxed) {
        for sample in output.iter_mut() {
            *sample = T::from_sample(0.0f32);
        }
        return;
    }

    let mut pos = ctx.position.load(Ordering::Relaxed);
    let ac = ctx.audio_channels as usize;
    let dc = ctx.device_channels as usize;
    let samples = &ctx.audio.samples;

    if ac == 0 || dc == 0 {
        for sample in output.iter_mut() {
            *sample = T::from_sample(0.0f32);
        }
        return;
    }

    for frame in output.chunks_mut(dc) {
        if pos >= ctx.total_samples {
            for sample in frame.iter_mut() {
                *sample = T::from_sample(0.0f32);
            }
            continue;
        }

        for (dev_ch, sample) in frame.iter_mut().enumerate() {
            let src_ch = dev_ch % ac;
            let idx = pos + src_ch;
            let val = if idx < ctx.total_samples {
                samples[idx]
            } else {
                0.0f32
            };
            *sample = T::from_sample(val);
        }
        pos += ac;
    }

    ctx.position.store(pos, Ordering::Relaxed);
}
