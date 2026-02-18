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
  → WORLD analysis (f0, sp, ap)
  → Modifier (ndarray ops on f0/sp/ap per slider values)
  → WORLD synthesis → f32 PCM
  → Effects chain (fundsp + pitch_shift) → playback (cpal) + FFT spectrum (rustfft)
```

WORLD analysis is **offline** (~2-5s per minute of audio). Results are cached; resynthesis runs only when sliders change.

### Threading Model

- **Main thread**: ratatui event loop (keyboard/mouse input, rendering)
- **Audio thread**: cpal output stream, reads from ring buffer, handles A/B switching and seek
- **Processing thread**: WORLD analysis/synthesis, modifier, effects, FFT

### Synchronization

- `Arc<AtomicBool>` — play/pause, A/B toggle, loop flag
- `Arc<Mutex<SliderState>>` — slider parameter values
- `ringbuf` — lock-free audio buffer (processing → playback)
- `crossbeam-channel` — UI commands → processing thread

### Key Modules

- `src/app.rs` — Central `AppState` holding all shared state
- `src/audio/` — decoder (symphonia), playback (cpal), ring buffer, WAV export (hound)
- `src/dsp/` — WORLD interface, parameter modifier, effects chain, spectrum FFT
- `src/ui/` — ratatui layout, custom slider widget, spectrum display, transport controls
- `src/input/` — keyboard/mouse event handler

## Important Design Decisions

- **ratatui 0.30**: crossterm is re-exported via `ratatui::crossterm` — no separate crossterm dependency needed
- **Two pitch shift controls**: WORLD pitch shift (formant-preserving, modifies f0) vs Effects pitch shift (phase vocoder, shifts everything including formants)
- **A/B comparison**: Two PCM buffers (original + processed) with shared seek position; `AtomicBool` toggles which buffer the audio thread reads from
- **Resynthesis caching**: Only resynthesize on slider release, not during drag. Process long files in chunks with progress indicator.

## Implementation Phases

The project follows phases P0–P8 defined in `plans/initial_plan.md`. P0 starts with WORLD FFI scaffolding and roundtrip test; P8 is polish. Refer to the plan for full details.
