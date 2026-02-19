use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream, StreamConfig};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};

use super::decoder::AudioData;

/// Shared playback state between the main thread and the audio callback.
#[derive(Debug)]
pub struct PlaybackState {
    /// Whether audio is currently playing (vs paused).
    pub playing: Arc<AtomicBool>,
    /// Current sample position in the interleaved buffer.
    pub position: Arc<AtomicUsize>,
    /// Handle to the audio data in the running stream's callback.
    /// Allows swapping audio without rebuilding the cpal stream.
    pub audio_lock: Option<Arc<RwLock<Arc<AudioData>>>>,
    /// Live gain multiplier (f32 stored as bits). Applied in the audio callback
    /// for instant (~5ms) feedback without buffer swap.
    pub live_gain: Arc<AtomicU32>,
    /// Whether playback should loop back to the start when it reaches the end.
    pub loop_enabled: Arc<AtomicBool>,
}

impl Default for PlaybackState {
    fn default() -> Self {
        Self {
            playing: Arc::new(AtomicBool::new(false)),
            position: Arc::new(AtomicUsize::new(0)),
            audio_lock: None,
            live_gain: Arc::new(AtomicU32::new(1.0_f32.to_bits())),
            loop_enabled: Arc::new(AtomicBool::new(false)),
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
        !self.playing.fetch_xor(true, Ordering::Release)
    }

    /// Seek by a signed sample offset, clamped to [0, max_samples].
    pub fn seek_by_samples(&self, offset: isize, max_samples: usize) {
        // Use Acquire/Release to synchronize with the audio callback thread.
        let current = self.position.load(Ordering::Acquire);
        let new_pos = (current as isize).saturating_add(offset).clamp(0, max_samples as isize) as usize;
        self.position.store(new_pos, Ordering::Release);
    }

    /// Seek by seconds. `channels` is needed to convert to interleaved sample offset.
    pub fn seek_by_secs(&self, secs: f64, sample_rate: u32, channels: u16, max_samples: usize) {
        // #4: Clamp the float product before casting to isize to prevent overflow.
        let raw = secs * sample_rate as f64 * channels as f64;
        let offset = raw.clamp(isize::MIN as f64, isize::MAX as f64) as isize;
        self.seek_by_samples(offset, max_samples);
    }

    /// Current playback time in seconds.
    #[must_use]
    pub fn current_time_secs(&self, sample_rate: u32, channels: u16) -> f64 {
        let pos = self.position.load(Ordering::Acquire);
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
///
/// `audio` is behind an `RwLock` so the main thread can atomically swap in new
/// `AudioData` (e.g. on file reload) without invalidating the callback reference.
struct CallbackContext {
    audio: Arc<RwLock<Arc<AudioData>>>,
    playing: Arc<AtomicBool>,
    position: Arc<AtomicUsize>,
    device_channels: u16,
    live_gain: Arc<AtomicU32>,
    loop_enabled: Arc<AtomicBool>,
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

    let mut state = PlaybackState::new();
    let audio_lock = Arc::new(RwLock::new(audio));

    let ctx = CallbackContext {
        playing: Arc::clone(&state.playing),
        position: Arc::clone(&state.position),
        device_channels: config.channels,
        audio: Arc::clone(&audio_lock),
        live_gain: Arc::clone(&state.live_gain),
        loop_enabled: Arc::clone(&state.loop_enabled),
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

    state.playing.store(true, Ordering::Release);
    state.audio_lock = Some(audio_lock);

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
    state: &mut PlaybackState,
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

    let audio_lock = Arc::new(RwLock::new(audio));

    let ctx = CallbackContext {
        playing: Arc::clone(&state.playing),
        position: Arc::clone(&state.position),
        device_channels: config.channels,
        audio: Arc::clone(&audio_lock),
        live_gain: Arc::clone(&state.live_gain),
        loop_enabled: Arc::clone(&state.loop_enabled),
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

    state.audio_lock = Some(audio_lock);

    Ok(stream)
}

/// Swap the audio buffer in a running stream's RwLock.
/// Glitch-free — the audio callback picks up the new data on its next read-lock acquisition.
pub fn swap_audio(audio_lock: &Arc<RwLock<Arc<AudioData>>>, new_audio: Arc<AudioData>) {
    let mut guard = audio_lock.write().expect("audio lock poisoned");
    *guard = new_audio;
}

fn write_audio_data<T: cpal::SizedSample + cpal::FromSample<f32>>(
    output: &mut [T],
    ctx: &CallbackContext,
) {
    let silence = T::from_sample(0.0f32);

    // Use Acquire to synchronize with main-thread Release stores.
    if !ctx.playing.load(Ordering::Acquire) {
        for sample in output.iter_mut() {
            *sample = silence;
        }
        return;
    }

    // Acquire the read lock; if poisoned or contended during reload, output silence.
    let audio_guard = match ctx.audio.try_read() {
        Ok(guard) => guard,
        Err(_) => {
            for sample in output.iter_mut() {
                *sample = silence;
            }
            return;
        }
    };

    let samples = &audio_guard.samples;
    let ac = audio_guard.channels as usize;
    let dc = ctx.device_channels as usize;
    let total_samples = samples.len();

    // #10: Explicit guard — ac and dc must be positive for modulo/division.
    if ac == 0 || dc == 0 {
        for sample in output.iter_mut() {
            *sample = silence;
        }
        return;
    }

    let mut pos = ctx.position.load(Ordering::Acquire);
    let gain = f32::from_bits(ctx.live_gain.load(Ordering::Relaxed));
    let looping = ctx.loop_enabled.load(Ordering::Relaxed);

    for frame in output.chunks_mut(dc) {
        if pos >= total_samples {
            if looping && total_samples > 0 {
                pos = 0;
            } else {
                for sample in frame.iter_mut() {
                    *sample = silence;
                }
                continue;
            }
        }

        for (dev_ch, sample) in frame.iter_mut().enumerate() {
            let src_ch = dev_ch % ac;
            let idx = pos + src_ch;
            let val = if idx < total_samples {
                samples[idx]
            } else {
                0.0f32
            };
            *sample = T::from_sample(val * gain);
        }
        pos += ac;
    }

    ctx.position.store(pos, Ordering::Release);
}
