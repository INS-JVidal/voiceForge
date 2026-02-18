# P3 — WORLD Integration & Slider-Driven Resynthesis: Implementation Report

## Goal
Wire the 6 WORLD sliders to actual audio processing: on file load, analyze audio with WORLD vocoder; on slider change, modify WORLD parameters and resynthesize. The user hears slider adjustments change the audio in near-real-time.

## Prerequisite
P0–P2 complete — 15 passing tests. TUI has 6 WORLD sliders that display and adjust values but aren't connected to processing.

## What Was Built

### New Files (5)

**`src/dsp/mod.rs`** — Module declarations for `modifier`, `processing`, `world`.

**`src/dsp/world.rs`** — Thin wrappers around `world_sys::analyze` and `world_sys::synthesize` that accept/return `AudioData` instead of raw `f64` slices.
- `to_mono(audio: &AudioData) -> AudioData` — cheap f32 downmix by averaging channels per frame. Used by the processing thread to create a consistent mono baseline for the neutral-slider shortcut.
- `analyze(audio: &AudioData) -> WorldParams` — converts interleaved f32 PCM to mono f64, calls `world_sys::analyze`.
- `synthesize(params: &WorldParams, sample_rate: u32) -> AudioData` — calls `world_sys::synthesize`, converts f64 output back to mono f32 `AudioData`.

**`src/dsp/modifier.rs`** — `WorldSliderValues` struct and `apply(params, values) -> WorldParams` with 6 transformations:
- **Pitch shift** — Scales f0 by `2^(semitones/12)`. Unvoiced frames (f0=0) are left unchanged.
- **Pitch range** — Expands/compresses f0 around its mean for voiced frames. Clamps to ≥0.
- **Speed** — Resamples all parameter arrays (f0, spectrogram, aperiodicity) via linear interpolation. `speed > 1.0` = fewer frames (faster playback), `speed < 1.0` = more frames (slower). Temporal positions are regenerated from frame period.
- **Breathiness** — Increases aperiodicity values towards 1.0, clamped to [0, 1].
- **Formant shift** — Warps the spectrogram frequency axis by `2^(semitones/12)`. Each destination bin interpolates from source bins at the warped position. Out-of-range bins use the last bin value.
- **Spectral tilt** — Applies a dB-per-octave slope across frequency bins. Tilt is relative to bin 1 (lowest non-DC bin) to avoid log2(0). Spectrogram values are power spectra, so linear gain is squared.

Helper functions `resample_1d` and `resample_2d` provide linear interpolation for the speed transform. Both handle edge cases (empty data, single-element, new_len=1).

`WorldSliderValues::is_neutral()` checks all 6 fields against their defaults (using exact f64 comparison — safe because slider values come from deterministic arithmetic with rounded step sizes).

**`src/dsp/processing.rs`** — Background processing thread with channel-based communication.
- `ProcessingCommand` enum: `Analyze(Arc<AudioData>)`, `Resynthesize(WorldSliderValues)`, `Shutdown`.
- `ProcessingResult` enum: `AnalysisDone`, `SynthesisDone(AudioData)`, `Status(String)`.
- `ProcessingHandle` struct: owns `Sender<Command>`, `Receiver<Result>`, and `JoinHandle`. Methods: `spawn()`, `send()`, `try_recv()`, `shutdown()`. `Drop` impl sends `Shutdown` and joins the thread for panic-safe cleanup.
- Processing loop owns cached `WorldParams` (never crosses the channel), a mono copy of the original audio, and the sample rate. On `Analyze`: converts to mono, runs WORLD analysis, stores params + mono original. On `Resynthesize`: applies modifier, runs synthesis, returns `AudioData`.

**`tests/test_modifier.rs`** — 3 integration tests:
- **Neutral roundtrip** — default `WorldSliderValues` preserves energy within 0.8–1.2× ratio after WORLD analysis + synthesis of both original and modified params.
- **Pitch shift +12st** — f0 is doubled (ratio 1.9–2.1) for voiced frames.
- **Speed 2×** — frame count halved (±1 frame tolerance). All parameter dimensions consistent.

### Modified Files (7)

**`Cargo.toml`** — Added `crossbeam-channel = "0.5"`.

**`src/lib.rs`** — Added `pub mod dsp;`.

**`src/audio/playback.rs`** — Added `rebuild_stream(audio: Arc<AudioData>, state: &PlaybackState) -> Result<Stream>`. Creates a new cpal output stream reusing the existing `PlaybackState` atomics (playing and position are preserved across buffer swaps). Same device/config logic as `start_playback` but without allocating a new `PlaybackState`.

**`src/app.rs`** — Three additions:
- `Action::Resynthesize` variant — returned by input handler when WORLD sliders change.
- `processing_status: Option<String>` field — holds "Analyzing..." or "Processing..." for status bar display.
- `world_slider_values(&self) -> WorldSliderValues` helper — extracts the 6 WORLD slider values by index into a `WorldSliderValues` struct.

**`src/input/handler.rs`** — Left/Right key handlers now check `focus == PanelFocus::WorldSliders` and return `Some(Action::Resynthesize)` instead of `None`. The focus is captured before the mutable borrow on sliders to satisfy the borrow checker.

**`src/ui/status_bar.rs`** — When `app.processing_status` is `Some`, appends a yellow-colored status string after the file info spans (separated by │).

**`src/main.rs`** — Substantial rework to wire everything together:
- Spawns `ProcessingHandle` at startup.
- On file load (CLI arg or file picker), sends `ProcessingCommand::Analyze(Arc<AudioData>)`.
- Event loop polls `processing.try_recv()` each frame (drains all pending results):
  - `AnalysisDone` → clears status, auto-sends `Resynthesize` with current slider values.
  - `SynthesisDone(audio)` → adjusts playback position for channel count changes, updates `file_info` (channels, total_samples, duration_secs), clamps position to buffer bounds, rebuilds cpal stream.
  - `Status(msg)` → sets `app.processing_status` for status bar display.
- Debounce timer (150ms) for `Action::Resynthesize` — resets on each slider key press, fires after 150ms idle.
- `processing.shutdown()` called on clean exit; `Drop` impl handles panic exit.

## Key Design Decisions

### 1. Processing Thread Owns WorldParams
`WorldParams` (containing the full spectrogram + aperiodicity matrices) is never sent across the channel. The processing thread keeps it in local state. Only the synthesized `AudioData` (a flat `Vec<f32>`) crosses back to the main thread. This avoids cloning large 2D arrays.

### 2. Consistent Mono Output
WORLD operates on mono audio and synthesizes mono output. To avoid channel count flip-flopping between neutral (original stereo) and non-neutral (mono synthesis), the processing thread stores a mono downmix of the original. The neutral-slider shortcut returns this mono copy rather than the original stereo. The main thread handles the one-time stereo→mono position adjustment on first `SynthesisDone`.

### 3. Channel-Aware Position Adjustment
When `SynthesisDone` delivers audio with a different channel count than the current `file_info.channels`, the playback position is converted: `frame = old_pos / old_channels`, `new_pos = frame * new_channels`. This prevents time jumps when transitioning from the initial stereo playback to mono WORLD output.

### 4. Debounce 150ms
Rapid keyboard slider adjustments don't spam the processing thread. The timer resets on each key press; the `Resynthesize` command is only sent after 150ms of no slider input.

### 5. Stale Command Drain
Before processing a `Resynthesize`, the processing thread drains the command channel via `try_recv()`, keeping only the latest `Resynthesize` values. This prevents a backlog of stale synthesis jobs when WORLD processing is slow. If an `Analyze` command arrives during the drain (user loaded a new file), it's processed immediately — the stale resynthesize is discarded since `AnalysisDone` will trigger a fresh one from main.

### 6. Neutral Sliders Skip WORLD Synthesis
When all 6 sliders are at their default values, the processing thread returns the mono original without running WORLD synthesis. This avoids introducing WORLD vocoder artifacts when the user hasn't made any adjustments.

### 7. Restart cpal Stream on Buffer Swap
`rebuild_stream` creates a new cpal output stream with the new audio buffer, reusing the existing `PlaybackState` atomics. The old stream is dropped (assignment replaces `Option<Stream>`). The sub-millisecond gap is inaudible after the processing delay.

## Architecture

```
main thread                     processing thread
───────────                     ─────────────────
load_file() ──Analyze(audio)──► world::analyze()
                                  stores WorldParams + mono original
              ◄──AnalysisDone──
auto-sends ──Resynthesize(v)──► modifier::apply()
                                  world::synthesize()
              ◄──SynthesisDone── AudioData (mono)
adjust position
update file_info
rebuild_stream()

slider change (debounced 150ms)
              ──Resynthesize(v)──► drain stale commands
                                   check neutral → return mono original
                                   or: modify + synthesize
              ◄──SynthesisDone──   same handling as above
```

## Robustness Considerations

- **Thread panic safety**: `ProcessingHandle::Drop` sends `Shutdown` and joins the thread. If the processing thread panics, `join()` captures it. Channels disconnect gracefully — `try_recv()` returns `None`, `send()` silently fails. The UI continues working (status may show stale "Analyzing..." but won't crash).
- **No file loaded**: `Resynthesize` commands are silently ignored when `cached_params` is `None`.
- **Empty audio edge cases**: `to_mono` handles 0-channel audio (returns empty vec). `apply_speed` handles empty f0 (returns early). `resample_1d`/`resample_2d` handle empty and single-element inputs.
- **Position clamping**: After every `SynthesisDone`, position is clamped to `[0, max_samples]` to prevent out-of-bounds reads in the audio callback.
- **Playback error recovery**: If `rebuild_stream` fails (e.g. audio device disconnected), the error is displayed in the status bar. The app continues running — the user can adjust sliders and try again.

## New Dependency

| Crate | Version | Purpose |
|-------|---------|---------|
| crossbeam-channel | 0.5 | Multi-producer/single-consumer channels for processing thread communication |

## Verification

- `cargo clippy --workspace` — zero warnings
- `cargo test` — 18/18 pass (4 decoder + 11 WORLD FFI + 3 modifier)
- Manual: `cargo run -- assets/test_samples/speech_like_5s.wav` — "Analyzing..." appears in status bar, clears when done; adjusting Pitch Shift shows "Processing...", then audio plays at shifted pitch; Speed changes update duration in transport bar; all sliders at defaults plays clean original audio

## Resolved P2 Placeholders

- `audio_data: Option<Arc<AudioData>>` — now used for WORLD analysis on file load
- WORLD slider values — now wired to modifier + resynthesis pipeline
- `SliderDef.default` — used by `WorldSliderValues::is_neutral()` (slider defaults match struct defaults)

## Remaining Placeholders for Future Phases

- `ab_original: bool` — A/B comparison toggle (P4)
- `loop_enabled: bool` — wired to UI display but not yet to playback loop logic (P4)
- Effects sliders — displayed and adjustable but not wired to effects processing (P6)
- Spectrum panel — placeholder text, real FFT visualization (P5)
