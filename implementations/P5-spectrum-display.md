# P5 — Spectrum Display: Implementation Report

## Goal

Add a live FFT spectrum analyzer that animates during playback, reflecting whichever A/B buffer is active. The existing spectrum panel (placeholder since P2) becomes a real-time bar chart with log-frequency mapping and color-coded magnitude.

## Prerequisite

P4/P4b complete (18 tests). The spectrum area in `src/ui/spectrum.rs` rendered static placeholder text. All threading, audio buffer sharing (`Arc<RwLock<Arc<AudioData>>>`), and ~30 fps render loop infrastructure was already in place.

## What Was Built

### New Files (2)

**`src/dsp/spectrum.rs`** — FFT computation and audio window extraction:
- `const FFT_SIZE: usize = 2048` — single source of truth for FFT window size.
- `compute_spectrum(samples, fft_size) -> Vec<f32>` — applies Hann window, runs forward FFT, returns dB magnitudes for the first `fft_size / 2` bins. Returns empty `Vec` for `fft_size < 2`.
- `extract_window(audio, pos, size) -> Vec<f32>` — reads `size` mono samples from `AudioData` at interleaved position `pos`. Downmixes multi-channel audio by averaging. Zero-pads beyond buffer end. Handles 0-channel edge case.
- FFT plan cached via `thread_local!` — `FftPlanner::new()` and `plan_fft_forward()` run once; subsequent calls at the same size reuse the cached `Arc<dyn Fft<f32>>`.

**`tests/test_spectrum.rs`** — 6 tests:
- `test_spectrum_440hz_peak` — 440 Hz sine at 44100 Hz sample rate; peak lands within ±2 bins of expected bin ~20; peak is >20 dB above a far bin.
- `test_spectrum_silence_all_low` — all-zero input produces magnitudes near -80 dB.
- `test_spectrum_small_fft_size` — edge cases: `fft_size` 0, 1, and 2 all handled without panic.
- `test_extract_window_stereo_downmix` — stereo L=1.0/R=0.0 downmixes to 0.5.
- `test_extract_window_zero_pads_beyond_end` — window extends beyond buffer; trailing samples are 0.0.
- `test_extract_window_zero_channels` — 0-channel audio returns all zeros.

### Modified Files (5)

**`Cargo.toml`** — Added `rustfft = "6.4"` dependency.

**`src/dsp/mod.rs`** — Added `pub mod spectrum;`.

**`src/app.rs`** — Two additions:
- `spectrum_bins: Vec<f32>` field on `AppState` — stores the current frame's dB magnitudes (1024 bins for FFT_SIZE=2048). Empty when no audio is playing.
- Initialized to `Vec::new()` in `AppState::new()`.

**`src/main.rs`** — Two areas of change:

1. **Spectrum update before each render** — before `terminal.draw()`, if playback is active:
   - Reads position from `app.playback.position` (atomic, Acquire).
   - Acquires `audio_lock` via `try_read()` (non-blocking; skips update if write-locked during audio swap).
   - Calls `extract_window` to get 2048 mono samples at playback position.
   - Calls `compute_spectrum` to produce dB magnitudes.
   - Stores result in `app.spectrum_bins`.
   - When paused, bins are not updated — spectrum freezes at the last playing position.

2. **Spectrum cleared on file load** — `app.spectrum_bins.clear()` in the `Action::LoadFile` handler, preventing stale spectrum from the previous file.

**`src/ui/layout.rs`** — Changed `spectrum::render(frame, spectrum_area)` to `spectrum::render(frame, spectrum_area, app)` to pass app state.

**`src/ui/spectrum.rs`** — Complete rewrite from placeholder to real bar chart:
- Signature: `pub fn render(frame: &mut Frame, area: Rect, app: &AppState)`.
- Renders bordered block with title `" Spectrum "`.
- Early return with `"  No audio playing"` placeholder if `spectrum_bins` is empty or area too small.
- **Log-frequency bin mapping**: `bin = bin_count^(i / (num_bars - 1))` — exponential mapping from ~21 Hz (bin 1) to ~22 kHz (bin 1023). Produces perceptually uniform spacing across the frequency axis.
- **Vertical bars**: each column maps to one FFT bin. Height normalized from dB range [-80, 0] to [0, inner_height].
- **Sub-cell resolution**: Unicode block characters `▁▂▃▄▅▆▇█` for fractional rows.
- **Per-row color gradient**: Green (bottom 50%), Yellow (50–75%), Red (top 25%) — color is determined by row position, so tall bars transition from green to yellow to red.

## Key Design Decisions

### 1. Main-Thread Sampling, Not Callback Injection

The spectrum is computed on the main thread by reading the current playback position and audio buffer. This avoids touching the cpal audio callback (`write_audio_data`) and `CallbackContext`, keeping the hot audio path unchanged. The `try_read()` on the `RwLock` is non-blocking — if a swap is in progress, the spectrum simply shows the previous frame.

### 2. Cached FFT Plan via thread_local

`FftPlanner::new()` + `plan_fft_forward(2048)` compute twiddle factors and choose an algorithm. Rather than paying this cost every frame (~30 fps), the plan is cached in a `thread_local!` `RefCell<Option<CachedFft>>`. Subsequent calls with the same size return a cloned `Arc<dyn Fft<f32>>` — effectively free.

### 3. Named Constant for FFT Size

`pub const FFT_SIZE: usize = 2048` in `dsp/spectrum.rs` is the single source of truth. Used in `main.rs` (window extraction + FFT call) and `tests/test_spectrum.rs`. Eliminates the risk of mismatched window/FFT sizes.

### 4. Per-Row Color Instead of Per-Bar

The color gradient is based on row position (vertical height in the widget), not bar height. This means a tall bar shows green at its base, yellow in its middle, and red at its tip — matching the convention of hardware spectrum analyzers and audio meters.

### 5. Frozen Spectrum on Pause

When `playing` is `false`, `spectrum_bins` is not updated. The spectrum freezes at whatever was displayed at the moment of pause. This is intentional — it lets users inspect the frequency content at the pause point. Bins are explicitly cleared on file load to prevent stale display.

### 6. Hann Window

The Hann window (`0.5 * (1 - cos(2πn/N))`) reduces spectral leakage at the cost of slightly wider main lobes. This is the standard choice for audio spectrum analyzers — it provides a good balance between frequency resolution and sidelobe suppression.

## Architecture

```
Main event loop (~30 fps):
  if playing:
    pos = position.load(Acquire)
    guard = audio_lock.try_read()  [non-blocking]
    window = extract_window(guard, pos, 2048)  [mono downmix, zero-pad]
    spectrum_bins = compute_spectrum(window, 2048)  [Hann + cached FFT + dB]

  terminal.draw():
    spectrum::render(frame, area, app)
      if spectrum_bins empty → placeholder
      else:
        for each column: log-freq bin mapping → dB → height
        for each row: block char + color by row position
```

## Edge Cases Handled

| Scenario | Behavior |
|---|---|
| No file loaded | `spectrum_bins` is empty → placeholder text |
| File loaded, not yet playing | `spectrum_bins` stays empty until play starts |
| Paused | Spectrum frozen at last playing frame |
| A/B toggle during playback | Next frame reads from the new buffer via `audio_lock` |
| Audio swap in progress (`try_read` fails) | Spectrum shows previous frame (no flicker) |
| Position beyond buffer end | `extract_window` zero-pads → quiet spectrum |
| Very small terminal | Early return if `inner.width < 2` or `inner.height < 1` |
| New file loaded | `spectrum_bins.clear()` prevents stale display |
| 0-channel audio (edge case) | `extract_window` returns zeros |
| `fft_size < 2` | `compute_spectrum` returns empty `Vec` |

## Robustness Considerations

- **No panics in FFT path**: `fft_size < 2` returns early. Window division by `(fft_size - 1)` is safe since the guard ensures `fft_size >= 2`. Buffer indexing uses `.min(bin_count - 1)`.
- **Non-blocking RwLock**: `try_read()` never blocks the UI thread. Worst case: one stale frame.
- **TOCTOU between position read and buffer read**: position may advance between the two reads. For a visualization this is imperceptible (~1ms drift at worst).
- **Block character index clamping**: `idx.min(8)` prevents out-of-bounds on the 9-element `BLOCKS` array. `frac` is in `(0.0, 1.0)` so `(frac * 8.0).round()` is in `[0, 8]`.
- **Magnitude clamping**: dB values clamped to `[-80, 0]`; height clamped to `[0, inner_h]`.

## New Dependencies

| Crate | Version | Purpose |
|---|---|---|
| `rustfft` | 6.4 | FFT computation (pure Rust, no FFTW/system dependency) |

Transitive: `num-complex`, `num-integer`, `primal-check`, `strength_reduce`, `transpose`.

## Verification

- `cargo clippy --workspace` — zero new warnings
- `cargo test` — 24/24 pass (4 decoder + 11 WORLD FFI + 3 modifier + 6 spectrum)
- Manual: load audio → spectrum bars animate during playback; pause → frozen; A/B toggle → bars reflect active buffer; new file load → spectrum clears and restarts

## Test Count

24 tests: 4 decoder + 11 WORLD FFI + 3 modifier + 6 spectrum

## Resolved Placeholders

- Spectrum panel — real FFT bar chart replaces `"Spectrum visualization — coming in P5"` placeholder

## Remaining Placeholders for Future Phases

- `loop_enabled: bool` — wired to UI display but not yet to playback loop logic
- Effects sliders — displayed and adjustable but not wired to effects processing (P6)
- WAV export (P7)
- Polish, keybinds help overlay (P8)
