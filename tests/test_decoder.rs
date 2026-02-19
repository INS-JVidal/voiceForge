use std::f32::consts::PI;
use std::path::{Path, PathBuf};

/// Generate a mono test WAV file at `path` using hound.
fn generate_test_wav(path: &Path, sample_rate: u32, duration_secs: f32, freq: f32) {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec).expect("failed to create WAV");
    let num_samples = (sample_rate as f32 * duration_secs) as usize;
    for i in 0..num_samples {
        let t = i as f32 / sample_rate as f32;
        let sample = (2.0 * PI * freq * t).sin() * 0.5;
        let amplitude = (sample * i16::MAX as f32) as i16;
        writer.write_sample(amplitude).expect("failed to write sample");
    }
    writer.finalize().expect("failed to finalize WAV");
}

fn test_wav_path(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/test_samples");
    std::fs::create_dir_all(&dir).expect("failed to create test_samples dir");
    dir.join(name)
}

#[test]
fn test_decoder_wav_basic() {
    let path = test_wav_path("test_basic_440hz.wav");
    generate_test_wav(&path, 44100, 0.5, 440.0);

    let audio = voiceforge::audio::decoder::decode_file(&path).expect("failed to decode WAV");

    assert_eq!(audio.sample_rate, 44100);
    assert_eq!(audio.channels, 1);
    assert!(!audio.samples.is_empty());

    // Duration should be ~0.5 seconds
    let dur = audio.duration_secs();
    assert!(
        (dur - 0.5).abs() < 0.05,
        "Expected duration ~0.5s, got {dur:.3}s"
    );

    // Verify samples are in valid range
    for &s in &audio.samples {
        assert!((-1.0..=1.0).contains(&s), "Sample out of range: {s}");
    }
}

#[test]
fn test_decoder_stereo_wav() {
    let path = test_wav_path("test_stereo.wav");

    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: 22050,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(&path, spec).expect("failed to create WAV");
    for i in 0..22050 {
        let t = i as f32 / 22050.0;
        let left = (2.0 * PI * 440.0 * t).sin() * 0.5;
        let right = (2.0 * PI * 880.0 * t).sin() * 0.3;
        writer
            .write_sample((left * i16::MAX as f32) as i16)
            .expect("write");
        writer
            .write_sample((right * i16::MAX as f32) as i16)
            .expect("write");
    }
    writer.finalize().expect("finalize");

    let audio = voiceforge::audio::decoder::decode_file(&path).expect("failed to decode");

    assert_eq!(audio.sample_rate, 22050);
    assert_eq!(audio.channels, 2);
    // 1 second * 2 channels * 22050 samples/sec = 44100 interleaved samples
    assert_eq!(audio.samples.len(), 44100);
    assert!((audio.duration_secs() - 1.0).abs() < 0.01);
}

#[test]
fn test_decoder_invalid_path() {
    let result = voiceforge::audio::decoder::decode_file(Path::new("/nonexistent/file.wav"));
    assert!(result.is_err());
}

#[test]
fn test_decoder_frame_count() {
    let path = test_wav_path("test_framecount_440hz.wav");
    generate_test_wav(&path, 44100, 0.5, 440.0);

    let audio = voiceforge::audio::decoder::decode_file(&path).expect("failed to decode WAV");

    // For mono, frame_count == samples.len()
    assert_eq!(audio.frame_count(), audio.samples.len());
    assert_eq!(audio.frame_count(), (44100.0 * 0.5) as usize);
}
