use std::sync::atomic::Ordering;

use voiceforge::audio::playback::PlaybackState;

#[test]
fn test_seek_by_secs_zero_sample_rate() {
    let state = PlaybackState::new();
    state.position.store(100, Ordering::Release);
    // sample_rate=0 should not panic; offset computes to 0 → position unchanged.
    state.seek_by_secs(5.0, 0, 2, 1000);
    assert_eq!(state.position.load(Ordering::Acquire), 100);
}

#[test]
fn test_seek_by_secs_zero_channels() {
    let state = PlaybackState::new();
    state.position.store(100, Ordering::Release);
    // channels=0 should not panic; offset computes to 0 → position unchanged.
    state.seek_by_secs(5.0, 44100, 0, 1000);
    assert_eq!(state.position.load(Ordering::Acquire), 100);
}

#[test]
fn test_seek_negative_from_zero() {
    let state = PlaybackState::new();
    state.position.store(0, Ordering::Release);
    // Negative seek from 0 should clamp to 0, not underflow.
    state.seek_by_secs(-10.0, 44100, 2, 1000);
    assert_eq!(state.position.load(Ordering::Acquire), 0);
}

#[test]
fn test_seek_large_positive_clamps() {
    let state = PlaybackState::new();
    state.position.store(0, Ordering::Release);
    let max_samples = 88200; // 1 second of stereo 44100
    state.seek_by_secs(999999.0, 44100, 2, max_samples);
    assert_eq!(state.position.load(Ordering::Acquire), max_samples);
}

#[test]
fn test_seek_large_negative_clamps() {
    let state = PlaybackState::new();
    state.position.store(44100, Ordering::Release);
    state.seek_by_secs(-999999.0, 44100, 2, 88200);
    assert_eq!(state.position.load(Ordering::Acquire), 0);
}

#[test]
fn test_seek_by_samples_negative_offset_from_zero() {
    let state = PlaybackState::new();
    state.position.store(0, Ordering::Release);
    state.seek_by_samples(-500, 1000);
    assert_eq!(state.position.load(Ordering::Acquire), 0);
}

#[test]
fn test_seek_by_samples_beyond_max() {
    let state = PlaybackState::new();
    state.position.store(500, Ordering::Release);
    state.seek_by_samples(isize::MAX, 1000);
    assert_eq!(state.position.load(Ordering::Acquire), 1000);
}

#[test]
fn test_current_time_zero_sample_rate() {
    let state = PlaybackState::new();
    state.position.store(44100, Ordering::Release);
    assert_eq!(state.current_time_secs(0, 2), 0.0);
}

#[test]
fn test_current_time_zero_channels() {
    let state = PlaybackState::new();
    state.position.store(44100, Ordering::Release);
    assert_eq!(state.current_time_secs(44100, 0), 0.0);
}

#[test]
fn test_toggle_playing_returns_new_state() {
    let state = PlaybackState::new();
    assert!(!state.playing.load(Ordering::Acquire)); // starts paused
    let new = state.toggle_playing();
    assert!(new); // now playing
    assert!(state.playing.load(Ordering::Acquire));
    let new2 = state.toggle_playing();
    assert!(!new2); // now paused
    assert!(!state.playing.load(Ordering::Acquire));
}
