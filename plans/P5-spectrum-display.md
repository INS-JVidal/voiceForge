# P5 — Real-Time Spectrum Display

## Goal
Show a live FFT spectrum visualization in the TUI that animates during playback, reflecting whichever buffer (original or processed) is currently active.

## Prerequisite
P4 complete (A/B playback works).

## Steps

### 5.1 Add dependency
```toml
rustfft = "6.4"
```

### 5.2 Spectrum computation — `src/dsp/spectrum.rs`
```rust
pub struct SpectrumData {
    pub magnitudes_db: Vec<f32>,  // dB values per frequency bin
    pub bin_count: usize,
}

pub fn compute_spectrum(samples: &[f32], fft_size: usize) -> SpectrumData { ... }
```
- Use `rustfft` with window size 2048 (or 1024 for faster updates)
- Apply a Hann window before FFT
- Compute magnitude: `20.0 * log10(|bin|)` for each bin
- Return only the first half of bins (real FFT symmetry)
- Normalize/clamp to a reasonable dB range (e.g., -80 dB to 0 dB)

### 5.3 Spectrum tap from playback
- During the cpal callback (or from the playback position tracker), copy the current window of samples (2048) into a shared buffer
- Use a lock-free mechanism: `Arc<Mutex<Vec<f32>>>` for the latest window, or a ring buffer snapshot
- The main thread reads this window each render frame and computes FFT

### 5.4 Spectrum widget — `src/ui/spectrum.rs`
Replace the placeholder with a real bar chart:
- Use `ratatui::widgets::BarChart` or a custom widget with Unicode block characters (`▁▂▃▄▅▆▇█`)
- Map frequency bins to ~40-80 visual bars (group adjacent bins)
- Use logarithmic frequency scale: more bars for low frequencies, fewer for high
- Color gradient: green (low) → yellow (mid) → red (high magnitude)
- Label X-axis with approximate frequencies: 20Hz, 200Hz, 1kHz, 5kHz, 20kHz

### 5.5 Update rate
- Compute FFT every 2-3 render frames (~20-30 FPS spectrum update, assuming ~60 FPS render loop)
- Skip FFT computation when paused (show last frame frozen)

### 5.6 Integration test — `tests/test_spectrum.rs`
1. Generate a 440 Hz sine wave at 44100 Hz
2. Run `compute_spectrum()` on a 2048-sample window
3. Verify the peak magnitude bin corresponds to ~440 Hz (bin index ≈ 440 * 2048 / 44100 ≈ 20)
4. Verify bins far from 440 Hz have significantly lower magnitude

## Human Test Checklist

- [ ] Load and play an audio file → spectrum bars animate in real time
- [ ] Bars respond to the audio content (voice/music shows characteristic frequency shapes)
- [ ] Pause playback → spectrum freezes
- [ ] Resume playback → spectrum animates again
- [ ] Press `Tab` to switch A/B → spectrum changes to reflect the other buffer's content
- [ ] Frequency labels are visible on the spectrum display
- [ ] With Pitch Shift applied, spectrum peak visibly shifts when toggling A/B
- [ ] `cargo test test_spectrum` passes
- [ ] UI remains responsive during spectrum rendering (no lag)

## Dependencies Introduced
- `rustfft` 6.4

## Notes
- Logarithmic frequency grouping is important for a useful display — linear would waste most bars on inaudible high frequencies.
- The Hann window prevents spectral leakage artifacts in the display.
