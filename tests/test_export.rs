use std::path::Path;

use tempfile::TempDir;
use voiceforge::audio::export::{default_export_path, export_wav};

fn sine_wave(freq: f32, sample_rate: u32, num_samples: usize) -> Vec<f32> {
    (0..num_samples)
        .map(|i| (2.0 * std::f32::consts::PI * freq * i as f32 / sample_rate as f32).sin())
        .collect()
}

#[test]
fn test_export_wav_creates_file() {
    // L-9: Use tempfile for automatic, panic-safe cleanup.
    let dir = TempDir::new().expect("failed to create temp dir");
    let path = dir.path().join("test_create.wav");

    let samples = sine_wave(440.0, 44100, 44100);
    export_wav(&samples, 44100, 1, &path).expect("export should succeed");

    assert!(path.exists(), "WAV file should be created");

    let reader = hound::WavReader::open(&path).expect("should read back");
    let spec = reader.spec();
    assert_eq!(spec.sample_rate, 44100);
    assert_eq!(spec.channels, 1);
    assert_eq!(spec.bits_per_sample, 16);
}

#[test]
fn test_export_wav_sample_count() {
    let dir = TempDir::new().expect("failed to create temp dir");
    let path = dir.path().join("test_count.wav");

    let samples = sine_wave(440.0, 44100, 8820); // 0.2s mono
    export_wav(&samples, 44100, 1, &path).expect("export should succeed");

    let reader = hound::WavReader::open(&path).expect("should read back");
    assert_eq!(reader.len() as usize, 8820);
}

#[test]
fn test_export_wav_stereo() {
    let dir = TempDir::new().expect("failed to create temp dir");
    let path = dir.path().join("test_stereo.wav");

    // Interleaved stereo: 1000 frames × 2 channels = 2000 samples
    let samples = vec![0.5_f32; 2000];
    export_wav(&samples, 44100, 2, &path).expect("export should succeed");

    let reader = hound::WavReader::open(&path).expect("should read back");
    let spec = reader.spec();
    assert_eq!(spec.channels, 2);
    assert_eq!(reader.len() as usize, 2000);
}

#[test]
fn test_export_wav_empty() {
    let dir = TempDir::new().expect("failed to create temp dir");
    let path = dir.path().join("test_empty.wav");

    export_wav(&[], 44100, 1, &path).expect("export of empty should succeed");

    let reader = hound::WavReader::open(&path).expect("should read back");
    assert_eq!(reader.len(), 0);
}

#[test]
fn test_export_wav_clamps_samples() {
    let dir = TempDir::new().expect("failed to create temp dir");
    let path = dir.path().join("test_clamp.wav");

    // Samples outside [-1, 1] should be clamped
    let samples = vec![-2.0_f32, 2.0, 0.0];
    export_wav(&samples, 44100, 1, &path).expect("export should succeed");

    let reader = hound::WavReader::open(&path).expect("should read back");
    let read_samples: Vec<i16> = reader.into_samples::<i16>().map(|s| s.unwrap()).collect();
    assert_eq!(read_samples[0], -32767); // clamped to -1.0
    assert_eq!(read_samples[1], 32767); // clamped to +1.0
    assert_eq!(read_samples[2], 0);
}

#[test]
fn test_default_export_path_basic() {
    let path = default_export_path("/home/user/music/song.mp3");
    assert_eq!(path, "/home/user/music/song_processed.wav");
}

#[test]
fn test_default_export_path_collision() {
    let dir = TempDir::new().expect("failed to create temp dir");

    // Create a file that would be the default name
    let blocking = dir.path().join("song_processed.wav");
    std::fs::write(&blocking, b"dummy").unwrap();

    let source = dir.path().join("song.mp3");
    let result = default_export_path(&source.to_string_lossy());

    let expected = dir.path().join("song_processed_2.wav");
    assert_eq!(
        Path::new(&result),
        expected,
        "should append _2 when default exists"
    );
}
