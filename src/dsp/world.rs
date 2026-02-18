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
    if channels <= 1 {
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

/// Analyze audio using WORLD vocoder. Converts to mono f64 internally.
pub fn analyze(audio: &AudioData) -> WorldParams {
    let mono = to_mono_f64(audio);
    world_sys::analyze(&mono, audio.sample_rate as i32)
}

/// Synthesize audio from WORLD parameters. Returns mono AudioData.
pub fn synthesize(params: &WorldParams, sample_rate: u32) -> AudioData {
    let samples = world_sys::synthesize(params, sample_rate as i32);
    from_mono_f64(&samples, sample_rate)
}
