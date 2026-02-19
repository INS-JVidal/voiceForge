use crate::audio::decoder::AudioData;
use world_sys::WorldParams;

/// Convert interleaved f32 PCM to mono f64 suitable for WORLD.
fn to_mono_f64(audio: &AudioData) -> Vec<f64> {
    let channels = audio.channels as usize;
    if channels == 0 {
        return Vec::new();
    }
    if channels == 1 {
        return audio.samples.iter().map(|&s| s as f64).collect();
    }
    // Downmix by averaging channels per frame.
    audio
        .samples
        .chunks_exact(channels)
        .map(|frame| {
            let sum: f64 = frame.iter().map(|&s| s as f64).sum();
            sum / channels as f64
        })
        .collect()
}

/// Convert mono f64 samples back to interleaved f32 AudioData.
fn from_mono_f64(samples: &[f64], sample_rate: u32) -> AudioData {
    AudioData {
        samples: samples.iter().map(|&s| s as f32).collect(),
        sample_rate,
        channels: 1,
    }
}

/// Downmix AudioData to mono (f32). If already mono, clones the data.
/// This is a cheap operation (no WORLD roundtrip) used to provide a
/// consistent mono baseline for the neutral-slider shortcut.
pub fn to_mono(audio: &AudioData) -> AudioData {
    let channels = audio.channels as usize;
    // L-2: Handle 0 channels — return empty mono instead of propagating 0-channel AudioData.
    if channels == 0 {
        return AudioData {
            samples: Vec::new(),
            sample_rate: audio.sample_rate,
            channels: 1,
        };
    }
    if channels == 1 {
        return audio.clone();
    }
    let samples = audio
        .samples
        .chunks_exact(channels)
        .map(|frame| {
            let sum: f32 = frame.iter().sum();
            sum / channels as f32
        })
        .collect();
    AudioData {
        samples,
        sample_rate: audio.sample_rate,
        channels: 1,
    }
}

/// Analyze audio using WORLD vocoder with progress callback. Converts to mono f64 internally.
/// The callback is called at 25%, 50%, 75%, and 100% completion.
///
/// # Errors
///
/// Returns an error if audio is empty or has zero channels.
pub fn analyze_with_progress<F>(
    audio: &AudioData,
    on_stage: F,
) -> Result<WorldParams, world_sys::WorldError>
where
    F: FnMut(u8),
{
    let mono = to_mono_f64(audio);
    // H-3: Guard against empty audio before the FFI call, which would panic.
    if mono.is_empty() {
        return Err(world_sys::WorldError::InvalidParams(
            "audio is empty (no samples or zero channels)".into(),
        ));
    }
    if audio.sample_rate == 0 {
        return Err(world_sys::WorldError::InvalidParams(
            "sample_rate must be positive".into(),
        ));
    }
    Ok(world_sys::analyze_with_progress(
        &mono,
        audio.sample_rate as i32,
        on_stage,
    ))
}

/// Analyze audio using WORLD vocoder. Converts to mono f64 internally.
///
/// # Errors
///
/// Returns an error if audio is empty or has zero channels.
pub fn analyze(audio: &AudioData) -> Result<WorldParams, world_sys::WorldError> {
    analyze_with_progress(audio, |_| {})
}

/// Synthesize audio from WORLD parameters. Returns mono AudioData.
///
/// # Errors
///
/// Returns an error if WORLD parameters are invalid or the output would be too large.
pub fn synthesize(params: &WorldParams, sample_rate: u32) -> Result<AudioData, world_sys::WorldError> {
    let samples = world_sys::synthesize(params, sample_rate as i32)?;
    Ok(from_mono_f64(&samples, sample_rate))
}
