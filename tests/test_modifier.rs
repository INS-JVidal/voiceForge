use std::f64::consts::PI;

use voiceforge::dsp::modifier::{self, WorldSliderValues};

/// Generate a harmonic-rich test signal and analyze it with WORLD.
fn make_test_params() -> (world_sys::WorldParams, u32) {
    let sample_rate = 44100_u32;
    let duration_secs = 1.0;
    let freq = 440.0;
    let num_samples = (sample_rate as f64 * duration_secs) as usize;

    let audio: Vec<f64> = (0..num_samples)
        .map(|i| {
            let t = i as f64 / sample_rate as f64;
            let fundamental = (2.0 * PI * freq * t).sin();
            let h2 = 0.5 * (2.0 * PI * freq * 2.0 * t).sin();
            let h3 = 0.3 * (2.0 * PI * freq * 3.0 * t).sin();
            (fundamental + h2 + h3) * 0.4
        })
        .collect();

    let params = world_sys::analyze(&audio, sample_rate as i32);
    (params, sample_rate)
}

#[test]
fn test_modifier_neutral_roundtrip() {
    let (params, sample_rate) = make_test_params();
    let values = WorldSliderValues::default();
    assert!(values.is_neutral());

    let modified = modifier::apply(&params, &values);

    // Neutral should preserve everything exactly.
    assert_eq!(modified.f0.len(), params.f0.len());
    assert_eq!(modified.spectrogram.len(), params.spectrogram.len());

    // Synthesize both and compare energy.
    let original_audio = world_sys::synthesize(&params, sample_rate as i32).unwrap();
    let modified_audio = world_sys::synthesize(&modified, sample_rate as i32).unwrap();

    let min_len = original_audio.len().min(modified_audio.len());
    let orig_energy: f64 = original_audio[..min_len].iter().map(|x| x * x).sum();
    let mod_energy: f64 = modified_audio[..min_len].iter().map(|x| x * x).sum();

    let ratio = mod_energy / (orig_energy + 1e-10);
    assert!(
        (0.8..1.2).contains(&ratio),
        "Neutral modifier should preserve energy, ratio = {ratio:.4}"
    );
}

#[test]
fn test_modifier_pitch_shift_12st() {
    let (params, _sample_rate) = make_test_params();

    let values = WorldSliderValues {
        pitch_shift: 12.0,
        ..Default::default()
    };
    let modified = modifier::apply(&params, &values);

    // f0 should be doubled for voiced frames.
    let voiced_orig: Vec<f64> = params.f0.iter().copied().filter(|&f| f > 0.0).collect();
    let voiced_mod: Vec<f64> = modified.f0.iter().copied().filter(|&f| f > 0.0).collect();

    assert!(!voiced_orig.is_empty());
    assert_eq!(voiced_orig.len(), voiced_mod.len());

    let mean_orig = voiced_orig.iter().sum::<f64>() / voiced_orig.len() as f64;
    let mean_mod = voiced_mod.iter().sum::<f64>() / voiced_mod.len() as f64;

    let ratio = mean_mod / mean_orig;
    assert!(
        (1.9..2.1).contains(&ratio),
        "+12 semitones should double f0, ratio = {ratio:.4}"
    );
}

#[test]
fn test_modifier_speed_2x() {
    let (params, _sample_rate) = make_test_params();
    let orig_frame_count = params.f0.len();

    let values = WorldSliderValues {
        speed: 2.0,
        ..Default::default()
    };
    let modified = modifier::apply(&params, &values);

    // Frame count should be approximately halved.
    let expected = (orig_frame_count as f64 / 2.0).round() as usize;
    let diff = (modified.f0.len() as isize - expected as isize).unsigned_abs();
    assert!(
        diff <= 1,
        "Speed 2× should halve frame count: got {}, expected ~{expected}",
        modified.f0.len()
    );

    // All dimensions should match.
    assert_eq!(modified.spectrogram.len(), modified.f0.len());
    assert_eq!(modified.aperiodicity.len(), modified.f0.len());
    assert_eq!(modified.temporal_positions.len(), modified.f0.len());
}
