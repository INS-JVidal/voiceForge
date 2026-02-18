# P3 — WORLD Integration & Slider-Driven Resynthesis

## Goal
Wire the WORLD vocoder into the app pipeline: on file load, analyze audio into f0/sp/ap parameters; when sliders change, apply modifications and resynthesize. The user should hear their slider adjustments change the audio.

## Prerequisite
P0 (WORLD FFI works), P1 (decoder/playback), P2 (TUI with sliders).

## Steps

### 3.1 Add dependencies
```toml
ndarray = "0.17"
crossbeam-channel = "0.5"
```

### 3.2 DSP world module — `src/dsp/world.rs`
High-level interface wrapping `world-sys`:
```rust
pub struct WorldAnalysis {
    pub f0: Vec<f64>,
    pub spectrogram: Vec<Vec<f64>>,   // or ndarray Array2<f64>
    pub aperiodicity: Vec<Vec<f64>>,  // or ndarray Array2<f64>
    pub fft_size: usize,
    pub frame_period: f64,
}

pub fn analyze(samples: &[f32], sample_rate: u32) -> WorldAnalysis { ... }
pub fn synthesize(params: &WorldAnalysis, sample_rate: u32) -> Vec<f32> { ... }
```
- Convert f32 → f64 for WORLD, f64 → f32 for output
- Use DIO + StoneMask for f0 by default

### 3.3 Modifier module — `src/dsp/modifier.rs`
Apply the 6 WORLD slider parameters to a `WorldAnalysis`:
```rust
pub struct WorldSliderValues {
    pub pitch_shift: f64,    // semitones
    pub pitch_range: f64,    // multiplier
    pub speed: f64,          // multiplier
    pub breathiness: f64,    // multiplier
    pub formant_shift: f64,  // semitones
    pub spectral_tilt: f64,  // dB/oct
}

pub fn apply(analysis: &WorldAnalysis, params: &WorldSliderValues) -> WorldAnalysis { ... }
```

Operations:
- **Pitch shift**: `f0[i] *= 2.0_f64.powf(pitch_shift / 12.0)` for all voiced frames
- **Pitch range**: scale f0 around its mean: `f0[i] = mean + (f0[i] - mean) * pitch_range`
- **Speed**: resample f0/sp/ap time axis (interpolate or decimate frames)
- **Breathiness**: `aperiodicity[i][j] *= breathiness` (clamp to [0, 1])
- **Formant shift**: warp spectrogram frequency axis by `2^(formant_shift/12)` using interpolation
- **Spectral tilt**: apply slope across frequency bins: `sp[i][j] *= 10^(tilt * log2(j/ref) / 20)`

### 3.4 Processing thread — `src/dsp/mod.rs` or dedicated module
Spawn a processing thread that:
1. Receives commands via `crossbeam-channel`:
   - `LoadFile(path)` → decode + WORLD analyze → store in shared state
   - `Resynthesize(slider_values)` → apply modifier → synthesize → update processed buffer
2. Sends status updates back (progress, completion)
3. Stores results in shared state accessible by the playback thread

### 3.5 Shared state for processed audio
Extend `AppState` (or use a separate shared struct):
```rust
pub struct ProcessedAudio {
    pub original_pcm: Vec<f32>,
    pub processed_pcm: Vec<f32>,
    pub analysis: Option<WorldAnalysis>,
    pub is_processing: bool,
}
```
Protected by `Arc<Mutex<...>>` or use channel-based updates.

### 3.6 Connect sliders to resynthesis
In input handler:
- When a slider value changes (on key release or after a debounce), send `Resynthesize` command to processing thread
- Show a "Processing..." indicator in the status bar while resynthesis is running
- Once complete, swap the playback buffer to the new processed audio

### 3.7 File load flow
When a file is loaded (CLI arg or `O` key):
1. Decode audio (symphonia) → original_pcm
2. Start playback of original immediately
3. Send to processing thread for WORLD analysis
4. Show "Analyzing..." with progress in status bar
5. Once analysis is complete, do initial resynthesis with default slider values
6. Processed buffer is now available

### 3.8 Integration test — `tests/test_modifier.rs`
1. Generate a synthetic audio signal
2. Analyze with WORLD
3. Apply pitch shift of +12 semitones
4. Synthesize
5. Verify the output fundamental frequency is approximately doubled
6. Test speed change: output length should change proportionally
7. Test with default values (all neutral): output ≈ input

## Human Test Checklist

- [ ] `cargo run -- voice_sample.wav` → file loads, "Analyzing..." appears, then clears when done
- [ ] Move Pitch Shift slider to +5 → after brief processing, audio plays back at higher pitch
- [ ] Move Pitch Shift slider to -5 → audio plays at lower pitch
- [ ] Move Speed slider to 1.5 → audio plays faster (shorter duration)
- [ ] Move Breathiness slider to 2.5 → audio sounds more breathy/whispery
- [ ] Move Formant Shift slider → voice timbre changes without pitch changing
- [ ] Reset all sliders to defaults → audio sounds like the original
- [ ] During resynthesis, status bar shows "Processing..." indicator
- [ ] `cargo test test_modifier` passes
- [ ] Large file (>30s) shows progress and doesn't freeze the UI during analysis

## Dependencies Introduced
- `ndarray` 0.17
- `crossbeam-channel` 0.5

## Risk Notes
- Resynthesis speed: for a 1-minute file, WORLD synthesis takes ~1-2s. If too slow, consider chunked processing or only resynthesizing on slider release (not during drag).
- Speed change alters buffer length — the playback position and seek bar need to account for the new duration.
- Formant shift via frequency axis warping requires careful interpolation to avoid artifacts at the edges of the spectrogram.
