use std::path::Path;

use voiceforge::audio::export::{default_export_path, export_wav};

fn sine_wave(freq: f32, sample_rate: u32, num_samples: usize) -> Vec<f32> {
    (0..num_samples)
        .map(|i| (2.0 * std::f32::consts::PI * freq * i as f32 / sample_rate as f32).sin())
        .collect()
}

#[test]
fn test_export_wav_creates_file() {
    let dir = std::env::temp_dir().join(format!("voiceforge_test_export_{}", line!()));
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("test_create.wav");
    let _ = std::fs::remove_file(&path);

    let samples = sine_wave(440.0, 44100, 44100);
    export_wav(&samples, 44100, 1, &path).expect("export should succeed");

    assert!(path.exists(), "WAV file should be created");

    // Read back and verify with hound
    let reader = hound::WavReader::open(&path).expect("should read back");
    let spec = reader.spec();
    assert_eq!(spec.sample_rate, 44100);
    assert_eq!(spec.channels, 1);
    assert_eq!(spec.bits_per_sample, 16);

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_dir(&dir);
}

#[test]
fn test_export_wav_sample_count() {
    let dir = std::env::temp_dir().join(format!("voiceforge_test_export_{}", line!()));
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("test_count.wav");
    let _ = std::fs::remove_file(&path);

    let samples = sine_wave(440.0, 44100, 8820); // 0.2s mono
    export_wav(&samples, 44100, 1, &path).expect("export should succeed");

    let reader = hound::WavReader::open(&path).expect("should read back");
    assert_eq!(reader.len() as usize, 8820);

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_dir(&dir);
}

#[test]
fn test_export_wav_stereo() {
    let dir = std::env::temp_dir().join(format!("voiceforge_test_export_{}", line!()));
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("test_stereo.wav");
    let _ = std::fs::remove_file(&path);

    // Interleaved stereo: 1000 frames × 2 channels = 2000 samples
    let samples = vec![0.5_f32; 2000];
    export_wav(&samples, 44100, 2, &path).expect("export should succeed");

    let reader = hound::WavReader::open(&path).expect("should read back");
    let spec = reader.spec();
    assert_eq!(spec.channels, 2);
    assert_eq!(reader.len() as usize, 2000);

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_dir(&dir);
}

#[test]
fn test_export_wav_empty() {
    let dir = std::env::temp_dir().join(format!("voiceforge_test_export_{}", line!()));
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("test_empty.wav");
    let _ = std::fs::remove_file(&path);

    export_wav(&[], 44100, 1, &path).expect("export of empty should succeed");

    let reader = hound::WavReader::open(&path).expect("should read back");
    assert_eq!(reader.len(), 0);

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_dir(&dir);
}

#[test]
fn test_export_wav_clamps_samples() {
    let dir = std::env::temp_dir().join(format!("voiceforge_test_export_{}", line!()));
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("test_clamp.wav");
    let _ = std::fs::remove_file(&path);

    // Samples outside [-1, 1] should be clamped
    let samples = vec![-2.0_f32, 2.0, 0.0];
    export_wav(&samples, 44100, 1, &path).expect("export should succeed");

    let reader = hound::WavReader::open(&path).expect("should read back");
    let read_samples: Vec<i16> = reader.into_samples::<i16>().map(|s| s.unwrap()).collect();
    assert_eq!(read_samples[0], -32767); // clamped to -1.0
    assert_eq!(read_samples[1], 32767); // clamped to +1.0
    assert_eq!(read_samples[2], 0);

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_dir(&dir);
}

#[test]
fn test_default_export_path_basic() {
    let path = default_export_path("/home/user/music/song.mp3");
    assert_eq!(path, "/home/user/music/song_processed.wav");
}

#[test]
fn test_default_export_path_collision() {
    let dir = std::env::temp_dir().join("voiceforge_test_export_collision");
    let _ = std::fs::create_dir_all(&dir);

    // Create a file that would be the default name
    let blocking = dir.join("song_processed.wav");
    std::fs::write(&blocking, b"dummy").unwrap();

    let source = dir.join("song.mp3");
    let result = default_export_path(&source.to_string_lossy());

    let expected = dir.join("song_processed_2.wav");
    assert_eq!(
        Path::new(&result),
        expected,
        "should append _2 when default exists"
    );

    let _ = std::fs::remove_file(&blocking);
    let _ = std::fs::remove_dir(&dir);
}
