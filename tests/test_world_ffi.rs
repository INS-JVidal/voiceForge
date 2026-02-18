use std::f64::consts::PI;

#[test]
fn test_world_ffi_roundtrip() {
    let sample_rate = 44100;
    let duration_secs = 1.0;
    let freq = 440.0;
    let num_samples = (sample_rate as f64 * duration_secs) as usize;

    // Generate a harmonic-rich signal (simulates a simple voice-like waveform)
    let audio: Vec<f64> = (0..num_samples)
        .map(|i| {
            let t = i as f64 / sample_rate as f64;
            let fundamental = (2.0 * PI * freq * t).sin();
            let h2 = 0.5 * (2.0 * PI * freq * 2.0 * t).sin();
            let h3 = 0.3 * (2.0 * PI * freq * 3.0 * t).sin();
            let h4 = 0.15 * (2.0 * PI * freq * 4.0 * t).sin();
            (fundamental + h2 + h3 + h4) * 0.4
        })
        .collect();

    // Analyze
    let params = world_sys::analyze(&audio, sample_rate);

    // Verify f0 contains values near 440 Hz for voiced frames
    let voiced_f0: Vec<f64> = params.f0.iter().copied().filter(|&f| f > 0.0).collect();
    assert!(
        !voiced_f0.is_empty(),
        "Expected some voiced frames with non-zero f0"
    );
    let mean_f0: f64 = voiced_f0.iter().sum::<f64>() / voiced_f0.len() as f64;
    assert!(
        (mean_f0 - freq).abs() < 20.0,
        "Expected mean f0 near {freq} Hz, got {mean_f0:.1} Hz"
    );

    // Verify parameter dimensions
    assert_eq!(params.spectrogram.len(), params.f0.len());
    assert_eq!(params.aperiodicity.len(), params.f0.len());
    let sp_width = params.fft_size / 2 + 1;
    assert_eq!(params.spectrogram[0].len(), sp_width);

    // Synthesize from unmodified parameters
    let output = world_sys::synthesize(&params, sample_rate)
        .expect("synthesize should succeed with valid params");

    // Verify output length is reasonable (within one frame)
    let length_diff = (output.len() as isize - audio.len() as isize).unsigned_abs();
    assert!(
        length_diff < (sample_rate as usize / 10),
        "Output length {}, expected near {}, diff {}",
        output.len(),
        audio.len(),
        length_diff
    );

    // Check energy similarity (WORLD preserves energy but may shift phase)
    let min_len = audio.len().min(output.len());
    let input = &audio[..min_len];
    let output_slice = &output[..min_len];

    let input_rms = (input.iter().map(|x| x * x).sum::<f64>() / min_len as f64).sqrt();
    let output_rms = (output_slice.iter().map(|x| x * x).sum::<f64>() / min_len as f64).sqrt();
    let rms_ratio = output_rms / (input_rms + 1e-10);
    assert!(
        rms_ratio > 0.3 && rms_ratio < 3.0,
        "RMS energy ratio out of range: {rms_ratio:.4} (input_rms={input_rms:.4}, output_rms={output_rms:.4})"
    );

    // Use peak cross-correlation to handle phase shifts
    // Search for best alignment within one period
    let period_samples = (sample_rate as f64 / freq) as usize;
    let mut best_corr = f64::NEG_INFINITY;
    let check_len = min_len - period_samples;
    let input_energy: f64 = input[..check_len].iter().map(|x| x * x).sum();

    for lag in 0..period_samples {
        let cross: f64 = input[..check_len]
            .iter()
            .zip(output_slice[lag..lag + check_len].iter())
            .map(|(a, b)| a * b)
            .sum();
        let out_energy: f64 = output_slice[lag..lag + check_len]
            .iter()
            .map(|x| x * x)
            .sum();
        let corr = cross / (input_energy.sqrt() * out_energy.sqrt() + 1e-10);
        if corr > best_corr {
            best_corr = corr;
        }
    }

    assert!(
        best_corr > 0.7,
        "Peak cross-correlation too low: {best_corr:.4} (expected > 0.7)"
    );

    println!("Roundtrip test passed:");
    println!("  Mean f0: {mean_f0:.1} Hz (expected ~{freq} Hz)");
    println!("  RMS ratio: {rms_ratio:.4}");
    println!("  Peak correlation: {best_corr:.4}");
    println!(
        "  Input length: {}, Output length: {}",
        audio.len(),
        output.len()
    );
}

#[test]
fn test_world_ffi_clone_params() {
    let audio: Vec<f64> = (0..4410)
        .map(|i| (2.0 * PI * 440.0 * i as f64 / 44100.0).sin())
        .collect();
    let params = world_sys::analyze(&audio, 44100);
    let cloned = params.clone();
    assert_eq!(params.f0, cloned.f0);
    assert_eq!(params.fft_size, cloned.fft_size);
}

// --- Input validation ---

#[test]
#[should_panic(expected = "audio must not be empty")]
fn test_world_ffi_analyze_empty_audio() {
    let _ = world_sys::analyze(&[], 44100);
}

#[test]
#[should_panic(expected = "sample_rate must be positive")]
fn test_world_ffi_analyze_zero_sample_rate() {
    let _ = world_sys::analyze(&[0.0; 100], 0);
}

#[test]
#[should_panic(expected = "sample_rate must be positive")]
fn test_world_ffi_analyze_negative_sample_rate() {
    let _ = world_sys::analyze(&[0.0; 100], -1);
}

#[test]
fn test_world_ffi_synthesize_zero_sample_rate() {
    let audio: Vec<f64> = (0..4410)
        .map(|i| (2.0 * PI * 440.0 * i as f64 / 44100.0).sin())
        .collect();
    let params = world_sys::analyze(&audio, 44100);
    assert!(world_sys::synthesize(&params, 0).is_err());
}

// --- WorldParams consistency validation (now returns Err, not panic) ---

#[test]
fn test_world_ffi_synthesize_empty_f0() {
    let params = world_sys::WorldParams {
        f0: vec![],
        temporal_positions: vec![],
        spectrogram: vec![],
        aperiodicity: vec![],
        fft_size: 1024,
        frame_period: 5.0,
    };
    assert!(world_sys::synthesize(&params, 44100).is_err());
}

#[test]
fn test_world_ffi_synthesize_mismatched_spectrogram() {
    let params = world_sys::WorldParams {
        f0: vec![440.0; 10],
        temporal_positions: vec![0.0; 10],
        spectrogram: vec![vec![0.0; 513]; 5], // 5 rows, should be 10
        aperiodicity: vec![vec![0.0; 513]; 10],
        fft_size: 1024,
        frame_period: 5.0,
    };
    assert!(world_sys::synthesize(&params, 44100).is_err());
}

#[test]
fn test_world_ffi_synthesize_mismatched_aperiodicity() {
    let params = world_sys::WorldParams {
        f0: vec![440.0; 10],
        temporal_positions: vec![0.0; 10],
        spectrogram: vec![vec![0.0; 513]; 10],
        aperiodicity: vec![vec![0.0; 513]; 3], // 3 rows, should be 10
        fft_size: 1024,
        frame_period: 5.0,
    };
    assert!(world_sys::synthesize(&params, 44100).is_err());
}

#[test]
fn test_world_ffi_synthesize_wrong_spectrogram_width() {
    let params = world_sys::WorldParams {
        f0: vec![440.0; 10],
        temporal_positions: vec![0.0; 10],
        spectrogram: vec![vec![0.0; 100]; 10], // should be 513
        aperiodicity: vec![vec![0.0; 513]; 10],
        fft_size: 1024,
        frame_period: 5.0,
    };
    assert!(world_sys::synthesize(&params, 44100).is_err());
}

#[test]
fn test_world_ffi_synthesize_mismatched_temporal_positions() {
    let params = world_sys::WorldParams {
        f0: vec![440.0; 10],
        temporal_positions: vec![0.0; 5], // 5, should be 10
        spectrogram: vec![vec![0.0; 513]; 10],
        aperiodicity: vec![vec![0.0; 513]; 10],
        fft_size: 1024,
        frame_period: 5.0,
    };
    assert!(world_sys::synthesize(&params, 44100).is_err());
}
