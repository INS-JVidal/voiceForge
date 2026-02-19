use voiceforge::dsp::effects::{apply_effects, apply_gain, EffectsParams};

fn sine_wave(freq: f32, sample_rate: u32, num_samples: usize) -> Vec<f32> {
    (0..num_samples)
        .map(|i| (2.0 * std::f32::consts::PI * freq * i as f32 / sample_rate as f32).sin())
        .collect()
}

fn rms(samples: &[f32]) -> f32 {
    (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
}

#[test]
fn test_effects_neutral_passthrough() {
    let input = sine_wave(440.0, 44100, 4096);
    let params = EffectsParams::default();
    assert!(params.is_neutral());
    let output = apply_effects(&input, 44100, &params);
    assert_eq!(output, input);
}

#[test]
fn test_effects_gain_plus_6db() {
    // Gain is now applied live in the audio callback via apply_gain, not via apply_effects.
    let mut buf = sine_wave(440.0, 44100, 4096);
    let input_rms = rms(&buf);
    apply_gain(&mut buf, 6.0);
    let ratio = rms(&buf) / input_rms;
    // +6 dB ≈ 2x amplitude
    assert!(
        (ratio - 2.0).abs() < 0.1,
        "gain ratio {ratio}, expected ~2.0"
    );
}

#[test]
fn test_effects_gain_minus_12db() {
    let mut buf = sine_wave(440.0, 44100, 4096);
    let input_rms = rms(&buf);
    apply_gain(&mut buf, -12.0);
    let ratio = rms(&buf) / input_rms;
    // -12 dB ≈ 0.25x amplitude
    assert!(
        (ratio - 0.25).abs() < 0.03,
        "gain ratio {ratio}, expected ~0.25"
    );
}

#[test]
fn test_effects_lowcut_attenuates_bass() {
    let sr = 44100;
    // 100 Hz tone through 500 Hz highpass — should be heavily attenuated
    let input = sine_wave(100.0, sr, 44100);
    let params = EffectsParams {
        low_cut_hz: 500.0,
        ..Default::default()
    };
    let output = apply_effects(&input, sr, &params);
    // Skip the first 1000 samples for filter settling
    let rms_in = rms(&input[1000..]);
    let rms_out = rms(&output[1000..]);
    assert!(
        rms_out < rms_in * 0.3,
        "100 Hz should be attenuated by 500 Hz highpass: in={rms_in}, out={rms_out}"
    );
}

#[test]
fn test_effects_highcut_attenuates_treble() {
    let sr = 44100;
    // 8000 Hz tone through 2000 Hz lowpass — should be heavily attenuated
    let input = sine_wave(8000.0, sr, 44100);
    let params = EffectsParams {
        high_cut_hz: 2000.0,
        ..Default::default()
    };
    let output = apply_effects(&input, sr, &params);
    let rms_in = rms(&input[1000..]);
    let rms_out = rms(&output[1000..]);
    assert!(
        rms_out < rms_in * 0.3,
        "8000 Hz should be attenuated by 2000 Hz lowpass: in={rms_in}, out={rms_out}"
    );
}

#[test]
fn test_effects_compressor_reduces_dynamics() {
    let sr = 44100;
    // M-13: Test that the compressor actually reduces the dynamic range
    // between a loud signal and a quiet signal.
    let loud: Vec<f32> = sine_wave(440.0, sr, 44100)
        .iter()
        .map(|s| s * 0.8) // ~-2 dBFS
        .collect();
    let quiet: Vec<f32> = sine_wave(440.0, sr, 44100)
        .iter()
        .map(|s| s * 0.1) // ~-20 dBFS
        .collect();
    let params = EffectsParams {
        compressor_thresh_db: -20.0,
        ..Default::default()
    };
    let loud_out = apply_effects(&loud, sr, &params);
    let quiet_out = apply_effects(&quiet, sr, &params);

    // Skip the first 2000 samples for filter/envelope settling
    let input_ratio = rms(&loud[2000..]) / rms(&quiet[2000..]);
    let output_ratio = rms(&loud_out[2000..]) / rms(&quiet_out[2000..]);

    assert!(
        output_ratio < input_ratio,
        "compressor should reduce dynamic range: input ratio {input_ratio:.2}, output ratio {output_ratio:.2}"
    );
}

#[test]
fn test_effects_pitch_shift_up() {
    let sr = 44100;
    let input = sine_wave(440.0, sr, 44100);
    let params = EffectsParams {
        pitch_shift_semitones: 12.0,
        ..Default::default()
    };
    let output = apply_effects(&input, sr, &params);
    // +12 semitones = 2x frequency → buffer should be ~half length
    let ratio = output.len() as f32 / input.len() as f32;
    assert!(
        (ratio - 0.5).abs() < 0.01,
        "buffer length ratio {ratio}, expected ~0.5"
    );
}

#[test]
fn test_effects_pitch_shift_down() {
    let sr = 44100;
    let input = sine_wave(440.0, sr, 44100);
    let params = EffectsParams {
        pitch_shift_semitones: -12.0,
        ..Default::default()
    };
    let output = apply_effects(&input, sr, &params);
    // -12 semitones = 0.5x frequency → buffer should be ~double length
    let ratio = output.len() as f32 / input.len() as f32;
    assert!(
        (ratio - 2.0).abs() < 0.01,
        "buffer length ratio {ratio}, expected ~2.0"
    );
}

#[test]
fn test_effects_reverb_differs_from_dry() {
    let sr = 44100;
    let input = sine_wave(440.0, sr, 44100);
    let params = EffectsParams {
        reverb_mix: 0.5,
        ..Default::default()
    };
    let output = apply_effects(&input, sr, &params);
    assert_eq!(output.len(), input.len());
    // Reverb should produce a different signal
    let diff: f32 = input
        .iter()
        .zip(output.iter())
        .map(|(a, b)| (a - b).abs())
        .sum::<f32>()
        / input.len() as f32;
    assert!(diff > 0.01, "reverb output should differ from dry: avg diff = {diff}");
}

#[test]
fn test_effects_empty_input() {
    let params = EffectsParams {
        gain_db: 6.0,
        ..Default::default()
    };
    let output = apply_effects(&[], 44100, &params);
    assert!(output.is_empty());
}

#[test]
fn test_effects_is_neutral() {
    assert!(EffectsParams::default().is_neutral());
    // Gain is applied live in audio callback — it does NOT affect is_neutral.
    assert!(EffectsParams {
        gain_db: 1.0,
        ..Default::default()
    }
    .is_neutral());
    assert!(!EffectsParams {
        reverb_mix: 0.1,
        ..Default::default()
    }
    .is_neutral());
}

#[test]
fn test_eq_params_is_neutral_default() {
    use voiceforge::dsp::effects::EqParams;
    let eq = EqParams::default();
    assert!(eq.is_neutral());
}

#[test]
fn test_effects_params_neutral_with_eq() {
    let params = EffectsParams::default();
    assert!(params.is_neutral());

    // EQ with non-zero gains should make params non-neutral
    let mut params_with_eq = EffectsParams::default();
    params_with_eq.eq.gains[0] = 3.0;
    assert!(!params_with_eq.is_neutral());
}

#[test]
fn test_eq_boost_at_1khz() {
    use voiceforge::dsp::effects::{apply_effects, EffectsParams};

    let sr = 44100;
    // 1 kHz sine wave
    let input = sine_wave(1000.0, sr, 44100);
    let input_rms = rms(&input[2000..]);

    // Create params with +6 dB at 1 kHz band (band 5)
    let mut params = EffectsParams::default();
    params.eq.gains[5] = 6.0;

    let output = apply_effects(&input, sr, &params);
    let output_rms = rms(&output[2000..]);

    let ratio = output_rms / input_rms;
    // +6 dB ≈ 2x amplitude
    assert!(
        (ratio - 2.0).abs() < 0.3,
        "1 kHz boost ratio {ratio}, expected ~2.0"
    );
}

#[test]
fn test_eq_boost_isolation_far_band() {
    use voiceforge::dsp::effects::{apply_effects, EffectsParams};

    let sr = 44100;
    // 10 kHz tone
    let input = sine_wave(10000.0, sr, 44100);
    let input_rms = rms(&input[2000..]);

    // Boost 63 Hz band (band 1) by +6 dB
    let mut params = EffectsParams::default();
    params.eq.gains[1] = 6.0;

    let output = apply_effects(&input, sr, &params);
    let output_rms = rms(&output[2000..]);

    let ratio = output_rms / input_rms;
    // Boosting 63 Hz should minimally affect 10 kHz (ratio 0.85-1.15)
    assert!(
        (ratio - 1.0).abs() < 0.15,
        "63 Hz boost should not significantly alter 10 kHz: ratio {ratio}, expected ~1.0"
    );
}
