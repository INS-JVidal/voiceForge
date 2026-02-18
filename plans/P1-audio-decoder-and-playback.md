# P1 — Audio Decoder & Playback Engine

## Goal
Load audio files (WAV/MP3/FLAC/OGG) into an f32 PCM buffer and play them back through the system audio output with play/pause/seek controls via keyboard.

## Prerequisite
P0 complete (project scaffold exists, `cargo build` works).

## Steps

### 1.1 Add dependencies to root Cargo.toml
```toml
cpal = "0.17"
symphonia = { version = "0.5", features = ["mp3", "wav", "pcm", "flac"] }
```

### 1.2 Audio decoder module — `src/audio/decoder.rs`
```rust
pub struct AudioData {
    pub samples: Vec<f32>,    // Interleaved PCM
    pub sample_rate: u32,
    pub channels: u16,
    pub duration_secs: f64,
}

pub fn decode_file(path: &Path) -> Result<AudioData, DecoderError> { ... }
```
- Use symphonia to open file, probe format, decode all packets
- Convert to f32 PCM
- Support WAV, MP3, FLAC (OGG if symphonia supports it with features)
- If stereo, keep interleaved; store channel count

### 1.3 Audio playback module — `src/audio/playback.rs`
- Open cpal default output device and stream
- Accept an `Arc<AudioData>` and shared playback state:
  - `Arc<AtomicBool>` for playing/paused
  - `Arc<AtomicUsize>` for current sample position (seek)
- The cpal callback reads samples from the buffer at current position
- Advance position each callback; stop at end (or loop if flag set)

### 1.4 Minimal CLI player — `src/main.rs`
Temporary CLI (no TUI yet):
1. Accept file path as command-line argument
2. Decode the file
3. Print file info (sample rate, channels, duration)
4. Start playback
5. Listen for keyboard input on stdin:
   - `Space` → toggle play/pause
   - `[` / `]` → seek back/forward 5 seconds
   - `q` → quit
6. Use crossterm raw mode for key detection (via `ratatui::crossterm`)

### 1.5 Integration test — `tests/test_decoder.rs`
1. Include a small test WAV file in `assets/test_samples/` (generate with hound or include a short file)
2. Test: decode WAV → verify sample_rate, channels, non-empty samples
3. Test: decode with invalid path → returns error
4. (Optional) If MP3 test sample available, test MP3 decoding

## Human Test Checklist

- [ ] `cargo run -- path/to/song.wav` plays audio through speakers/headphones
- [ ] Press Space → audio pauses; press Space again → audio resumes from same position
- [ ] Press `]` → audio jumps forward ~5 seconds
- [ ] Press `[` → audio jumps backward ~5 seconds
- [ ] Press `q` → program exits cleanly (terminal restored)
- [ ] Try with an MP3 file → also plays correctly
- [ ] `cargo test test_decoder` passes
- [ ] File info printed to terminal shows correct sample rate and duration

## Dependencies Introduced
- `cpal` 0.17
- `symphonia` 0.5

## Risk Notes
- WSL2 audio: cpal uses ALSA backend on Linux. Ensure `libasound2-dev` is installed. If no audio device is found, cpal will error — handle gracefully with a clear message.
- Sample rate mismatch: if file sample rate differs from device sample rate, audio will play at wrong speed. For now this is acceptable; resampling can be added later.
