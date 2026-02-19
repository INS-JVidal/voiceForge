# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**VoiceForge** is a terminal-based voice modulation workbench written in Rust. It uses the WORLD vocoder (C++ via FFI) to decompose audio into pitch/spectral/aperiodicity parameters, lets users manipulate them via TUI sliders, applies post-processing effects, and provides real-time A/B comparison with spectrum visualization.

License: GPL-3.0-or-later

## Build Commands

```bash
# System dependencies (Ubuntu/WSL2)
sudo apt install build-essential cmake libasound2-dev pkg-config

# WSL2 audio (required for playback — WSLg must be active)
sudo apt install libasound2-plugins pulseaudio-utils
echo -e "pcm.default pulse\nctl.default pulse" > ~/.asoundrc
# Verify: pactl info → "Server Name: pulseaudio"

cargo build                # Debug build
cargo build --release      # Release build
cargo run                  # Run the TUI app
cargo test                 # Run all tests
cargo test test_world_ffi  # Run a single test
cargo clippy               # Lint
cargo fmt                  # Format
```

## Architecture

### Workspace Layout

- **Root crate (`voiceforge`)** — TUI app with audio pipeline
- **`crates/world-sys/`** — FFI bindings to vendored C++ WORLD vocoder (MIT). `build.rs` compiles C++ via `cc` crate. `world-src/` contains vendored sources from `github.com/mmorise/World`.

### Data Pipeline

```
Audio file → symphonia decoder → f32 PCM
  → WORLD analysis (f0, sp, ap)           [processing thread, on file load]
  → Modifier (Vec<Vec<f64>> ops per slider values)
  → WORLD synthesis → f32 PCM             [processing thread, on slider change]
  → playback (cpal)
  [future: → Effects chain → FFT spectrum]
```

WORLD analysis is **offline** (~2-5s per minute of audio). Results are cached in the processing thread; resynthesis runs only when sliders change (debounced 150ms). Neutral sliders skip WORLD synthesis and return a mono downmix of the original.

### Threading Model

- **Main thread**: ratatui event loop (keyboard/mouse input, rendering at ~30fps)
- **Audio thread**: cpal output stream callback, reads from `Arc<RwLock<Arc<AudioData>>>`
- **Processing thread**: WORLD analysis/synthesis, parameter modifier. Communicates via `crossbeam-channel`. Owns cached `WorldParams` (never crosses the channel boundary).

### Synchronization

- `Arc<AtomicBool>` — play/pause (Acquire/Release ordering)
- `Arc<AtomicUsize>` — playback position in interleaved samples (Acquire/Release ordering)
- `Arc<RwLock<Arc<AudioData>>>` — audio callback reads current buffer; main thread swaps on resynthesis
- `crossbeam-channel` — `ProcessingCommand` / `ProcessingResult` between main and processing threads

### Key Modules

- `src/app.rs` — Central `AppState`, `Action` enum, `SliderDef`, `FileInfo`, `WorldSliderValues` helper
- `src/audio/decoder.rs` — symphonia-based file decoder → `AudioData` (interleaved f32 PCM)
- `src/audio/playback.rs` — cpal output stream, `PlaybackState` (atomics + `audio_lock`), `start_playback`, `rebuild_stream`, `swap_audio`
- `src/dsp/world.rs` — f32↔f64 conversion, mono downmix (`to_mono`), thin wrappers around `world_sys::analyze`/`synthesize`
- `src/dsp/modifier.rs` — `WorldSliderValues` struct, `apply()` with 6 transforms (pitch shift, pitch range, speed, breathiness, formant shift, spectral tilt)
- `src/dsp/processing.rs` — `ProcessingHandle` (spawn/send/try_recv/shutdown), background thread with command drain and neutral-slider shortcut
- `src/ui/` — ratatui layout, slider widget, spectrum placeholder, transport bar, status bar, file picker
- `src/input/handler.rs` — keyboard event handler, returns `Option<Action>`. Key bindings: `q`/`Esc` quit, `Space` play/pause, `Tab` cycle focus, `Up`/`Down` select slider, `Left`/`Right` adjust slider or seek (Transport), `[`/`]` seek ±5s, `Home`/`End` jump to start/end, `a` A/B toggle, `r` loop toggle, `o` open file
- `crates/world-sys/` — FFI bindings; `analyze()` panics on invalid input, `synthesize()` returns `Result<Vec<f64>, WorldError>`

## Important Design Decisions

- **ratatui 0.30**: crossterm is re-exported via `ratatui::crossterm` — no separate crossterm dependency needed
- **Two pitch shift controls**: WORLD pitch shift (formant-preserving, modifies f0) vs Effects pitch shift (phase vocoder, shifts everything including formants)
- **A/B comparison**: `'a'` key toggles between original mono and processed audio. Both buffers stored in `AppState` (`original_audio` + `audio_data`). Toggle swaps the `Arc<AudioData>` inside the stream's `RwLock` via `swap_audio()` — O(1), glitch-free, no stream rebuild. Position scaled proportionally when buffer lengths differ (speed slider).
- **Consistent mono output**: WORLD always produces mono. The processing thread stores a mono downmix of the original for the neutral-slider shortcut. Main thread adjusts playback position on channel count changes (stereo→mono on first resynthesis).
- **Debounced resynthesis**: 150ms debounce on slider changes. Processing thread drains stale `Resynthesize` commands, keeping only the latest.
- **Buffer swap via RwLock**: `swap_audio()` replaces the `Arc<AudioData>` inside the stream's `RwLock` — glitch-free, O(1). `rebuild_stream` is only used as a fallback if `audio_lock` is unavailable. Both `start_playback` and `rebuild_stream` expose the `audio_lock` handle in `PlaybackState`.
- **world_sys error handling**: `synthesize()` returns `Result` (allocation guard, param validation). `analyze()` panics on invalid input (programmer error). Processing thread sends status message on synthesis failure.

## Implementation Phases

The project follows phases P0–P8 defined in `plans/initial_plan.md`. Reports in `implementations/`. Audits in `audits/`.

- **P0** — WORLD FFI scaffolding and roundtrip test ✓
- **P1** — Audio decoder (symphonia) and cpal playback ✓
- **P1b** — WSL2 audio fix ✓
- **P2** — TUI skeleton with ratatui (sliders, transport, status bar, file picker) ✓
- **P3** — WORLD integration and slider-driven resynthesis (6 transforms, processing thread, debounce) ✓
- **P3b** — Audit integration corrections (API compat fixes after P0–P2 security audit merge) ✓
- **P4** — A/B comparison toggle (`'a'` key, RwLock swap, proportional position scaling) ✓
- **P4b** — Enhanced seek navigation (Home/End, Transport arrows) + status_message visibility fix ✓
- P5–P8 — Remaining (spectrum FFT, effects chain, WAV export, polish)

### Test Count

18 tests: 4 decoder + 11 WORLD FFI + 3 modifier
