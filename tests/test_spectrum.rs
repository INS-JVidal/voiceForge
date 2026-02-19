use voiceforge::dsp::spectrum::{compute_spectrum, extract_window, FFT_SIZE};

#[test]
fn test_spectrum_440hz_peak() {
    let sr = 44100.0f32;
    let samples: Vec<f32> = (0..FFT_SIZE)
        .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sr).sin())
        .collect();
    let magnitudes = compute_spectrum(&samples, FFT_SIZE);
    let expected = (440.0 * FFT_SIZE as f32 / sr).round() as usize; // ≈ 20
    let peak = magnitudes
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
        .map(|(i, _)| i)
        .unwrap();
    assert!(
        (peak as isize - expected as isize).abs() <= 2,
        "peak at bin {peak}, expected ~{expected}"
    );
    assert!(
        magnitudes[peak] > magnitudes[200] + 20.0,
        "peak not significantly above far bin"
    );
}

#[test]
fn test_spectrum_silence_all_low() {
    let samples = vec![0.0f32; FFT_SIZE];
    let magnitudes = compute_spectrum(&samples, FFT_SIZE);
    assert_eq!(magnitudes.len(), FFT_SIZE / 2);
    for &db in &magnitudes {
        assert!(db <= -79.0, "silent signal should be near -80 dB, got {db}");
    }
}

#[test]
fn test_spectrum_small_fft_size() {
    assert!(compute_spectrum(&[], 0).is_empty());
    assert!(compute_spectrum(&[], 1).is_empty());
    let m = compute_spectrum(&[1.0, 0.0], 2);
    assert_eq!(m.len(), 1);
}

#[test]
fn test_extract_window_stereo_downmix() {
    use voiceforge::audio::decoder::AudioData;
    // Stereo: L=1.0, R=0.0 repeated
    let audio = AudioData {
        samples: vec![1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0],
        sample_rate: 44100,
        channels: 2,
    };
    let window = extract_window(&audio, 0, 4);
    assert_eq!(window.len(), 4);
    for &s in &window {
        assert!((s - 0.5).abs() < 1e-6, "stereo downmix should be 0.5, got {s}");
    }
}

#[test]
fn test_extract_window_zero_pads_beyond_end() {
    use voiceforge::audio::decoder::AudioData;
    let audio = AudioData {
        samples: vec![1.0, 1.0],
        sample_rate: 44100,
        channels: 1,
    };
    let window = extract_window(&audio, 0, 4);
    assert_eq!(window.len(), 4);
    assert!((window[0] - 1.0).abs() < 1e-6);
    assert!((window[1] - 1.0).abs() < 1e-6);
    assert_eq!(window[2], 0.0);
    assert_eq!(window[3], 0.0);
}

#[test]
fn test_extract_window_zero_channels() {
    use voiceforge::audio::decoder::AudioData;
    let audio = AudioData {
        samples: vec![],
        sample_rate: 44100,
        channels: 0,
    };
    let window = extract_window(&audio, 0, 4);
    assert_eq!(window, vec![0.0; 4]);
}
