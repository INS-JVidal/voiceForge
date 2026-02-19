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
  → Effects chain (EQ, compression, reverb, filters)
  → playback (cpal)
  [Spectrum FFT visualization available but GPU rendering does not work reliably in WSL2]
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
- `src/dsp/effects.rs` — Effects chain: EQ (12-band), compression, reverb mix, gain, low/high cut filters
- `src/dsp/processing.rs` — `ProcessingHandle` (spawn/send/try_recv/shutdown), background thread with command drain and neutral-slider shortcut
- `src/audio/export.rs` — WAV export via hound crate
- `src/ui/spectrum.rs` — FFT-based spectrum visualization with frequency labels (GPU pixel rendering not functional in WSL2)
- `src/ui/` — ratatui layout, slider widget, spectrum visualization (with FFT), transport bar, status bar, file picker (scrollable 5-row window)
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
- **P5** — Scrollable file picker (5-row window with scroll indicators) ✓
- **P6+** — Effects chain (EQ, compression, reverb, filters), WAV export, FFT spectrum visualization ✓
- **WSL2 Known Issue:** GPU pixel spectrum rendering does not work reliably in WSL2; falls back to text-based visualization

### Test Count

56+ tests: 11 WORLD FFI + 7 effects + 6 spectrum + 10 decoder + 6 playback + 5 modifier + 3 export + 4 lib integration

## General Rules for Implementation

### Planning & Execution

When asked to implement a plan:
- **Start coding immediately.** Do NOT enter extended planning mode or repeatedly update plan files before writing code.
- **Keep planning brief** — under 2 minutes of analysis, then begin implementation.
- **For multi-phase requests** — implement one phase per session. Break "P5, P6, review, commit" into separate atomic requests.
  - ✓ Good: "Implement P5. After tests pass, commit and push. Do NOT start P6."
  - ✗ Avoid: "Implement P5 and P6, then review, then commit."
- **Commit working code early and often** — don't wait until end of session. If work is complete and tests pass, commit immediately.
- **If a plan already exists** (in `plans/` or `implementations/`), read it and execute without re-planning.

### Debugging & Problem Solving

- **Commit to one approach and see it through** before switching strategies.
- **Do not waffle** between multiple fix strategies. If the first approach fails:
  - Explain concisely why it failed
  - Propose ONE alternative
  - Implement and test that one
- **Iterate in a loop:** Change code → Run tests → Analyze failure → Adjust → Repeat (up to 10 times if necessary).
- **Document assumptions** in code comments when making non-obvious decisions.

## Build & Test

### Pre-Commit Validation

Before every commit, run:
```bash
cargo check              # Quick syntax check
cargo clippy --all-targets -- -D warnings    # Lint, fail on warnings
cargo test --all-targets # Run all tests
```

**Goal:** Zero warnings, all tests passing before commit. This catches buggy first implementations at the gate rather than in back-and-forth debugging cycles.

### Test-Driven Bug Fixes

When fixing a bug:
1. Write a **failing test** that reproduces the bug (it MUST fail initially)
2. Run `cargo test` to confirm it fails
3. Implement the minimal fix
4. Run `cargo test --all-targets` after each edit
5. Iterate until all tests pass (including the new regression test)
6. Run `cargo clippy --all-targets -- -D warnings`
7. Commit with message: `fix: [description]`

## Git & GitHub

### Repository Operations

- Use `gh` CLI directly for all GitHub operations (repo creation, pushing, PRs, etc.)
- **Do not ask for confirmation** on git operations — execute autonomously
- When creating a new remote:
  ```bash
  gh repo create USERNAME/REPO --public --source=. --remote=origin --push
  ```
- Always push after committing: `git push origin master`

### Commit Standards

- Use conventional commit format: `type: description`
  - `feat:` new feature
  - `fix:` bug fix
  - `refactor:` code restructuring
  - `test:` test additions or updates
  - `docs:` documentation
  - `ci:` CI/CD changes
- Commit message should explain the "why," not just the "what"
- Keep commits atomic — one logical change per commit

## Environment (WSL2-Specific)

### Audio Configuration

- WSL2 requires PulseAudio for cpal playback
- WSLg must be active for X11 forwarding
- Verify: `pactl info` should show `Server Name: pulseaudio`
- Configure: `echo -e "pcm.default pulse\nctl.default pulse" > ~/.asoundrc`

### Browser & GUI Operations

- `xdg-open` on native Linux may not work in WSL2
- For browser opening in WSL2, prefer: `wslview <URL>` or `powershell.exe Start-Process <URL>`
- **Known domains:**
  - Astro blog domain: `prompt-lucido.com` (NOT `promptlucido.com`)
  - GitHub account: `INS-JVidal`

### Terminal Multiplexers

- Some features (like terminal title get/set) may be intercepted by zellij or tmux
- If testing terminal queries, check `$ZELLIJ` and `$TMUX` env vars
- Behavior may differ in multiplexer vs bare terminal

### GPU/Graphics Limitations (WSL2)

- **Spectrum Visualization:** GPU pixel rendering (24-bit true color gradient) does not work reliably in WSL2 terminals
- **Workaround:** Application falls back to text-based frequency labels and ASCII visualization
- **Note:** Native Linux or other terminals may support full GPU rendering; this is WSL2-specific limitation
- Development continues with text-based rendering as baseline for WSL2 compatibility

## Token Limit & Session Management

- **Break mega-sessions into atomic units** — each focused on one plan phase or feature
- **Verify completion criteria before starting next phase:**
  - `cargo test --all-targets` passes
  - `cargo clippy --all-targets -- -D warnings` passes
  - One logical commit made and pushed
- **If near token limit** with more work remaining, stop, commit, and ask user to start a new session
- Target: ~90% of session for implementation, 10% for review/verification/commit
