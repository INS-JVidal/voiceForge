# P2 — TUI Skeleton with ratatui: Implementation Report

## Goal
Replace the minimal P1 CLI player with a full ratatui TUI featuring slider panels, spectrum placeholder, transport bar, status bar, keyboard navigation, and a file picker — while preserving the existing P1 audio playback.

## Prerequisite
P0 (WORLD FFI) and P1 (audio decoder + playback) complete — 15 passing tests.

## What Was Built

### New Files (10)

**`src/app.rs`** — Central application state and types.
- `AppMode` enum: `Normal`, `FilePicker`
- `PanelFocus` enum: `WorldSliders`, `EffectsSliders`, `Transport` — with `next()` cycler
- `Action` enum: `Quit`, `LoadFile(String)` — returned by input handler for side effects
- `FileInfo` struct: name, sample_rate, channels, duration_secs, total_samples
- `SliderDef` struct: label, min/max/value/default/step/unit — with `adjust(steps)` (clamped, drift-rounded) and `fraction()` for bar rendering
- `AppState` struct: all TUI + playback state in one place. 12 sliders (6 WORLD, 6 effects), playback state, file info, mode/focus tracking
- `focused_sliders_mut()` returns `Option` — `None` for Transport, preventing silent wrong-panel access

**`src/ui/mod.rs`** — Module declarations for all six UI submodules.

**`src/ui/spectrum.rs`** — Bordered placeholder block ("coming in P5"). Simplest widget, no dependencies.

**`src/ui/slider.rs`** — Renders a panel of 6 sliders with:
- Cyan border when focused, white otherwise
- `▸` indicator and bold cyan label on selected slider
- Unicode bar (`█`/`░`) with proportional fill from `SliderDef::fraction()`
- Yellow value display with unit suffix
- Narrow-terminal fallback (value-only when width < threshold)

**`src/ui/status_bar.rs`** — Single-line status: file info (name │ Hz │ Mono/Stereo │ m:ss), error messages in red, or "No file loaded — press 'o' to open" in gray.

**`src/ui/transport.rs`** — Transport bar showing:
- Play/pause icon (green ▶ / yellow ⏸) with bold styling
- Loop toggle indicator `[Loop: On/Off]`
- Seek bar (`─●─`) sized to fill remaining width after metadata
- Time display `m:ss/m:ss`
- A/B toggle indicator in magenta

**`src/ui/file_picker.rs`** — Centered popup overlay (60% width, 5 rows). Uses `ratatui::layout::Flex::Center` for positioning. `Clear` widget erases background behind popup. Yellow bordered block with text input prompt and cursor block.

**`src/ui/layout.rs`** — Orchestrates all widgets into the final layout:
```
┌─ WORLD Vocoder (50%) ─┐┌─ Effects (50%) ────────┐  ← Percentage(70) of Min(8)
│ 6 sliders              ││ 6 sliders              │
└────────────────────────┘└────────────────────────┘
┌─ Spectrum ────────────────────────────────────────┐  ← Min(3) remaining
│ Placeholder                                        │
└────────────────────────────────────────────────────┘
┌─ Transport ───────────────────────────────────────┐  ← Length(3)
│ ▶ Playing  [Loop: Off]  ──●── 1:23/3:45 [A/B]    │
└────────────────────────────────────────────────────┘
 File: song.wav │ 44100 Hz │ Stereo │ 3:45            ← Length(1)
```
Renders FilePicker overlay on top when `mode == FilePicker`.

**`src/input/mod.rs`** — Module declaration.

**`src/input/handler.rs`** — Pure input handler returning `Option<Action>`:
- Dispatches to `handle_normal()` or `handle_file_picker()` based on `AppMode`
- Normal mode: q/Esc quit, Space play/pause, Tab cycle focus, Up/Down slider navigation, Left/Right adjust (Shift for fine ±0.2 steps), r loop toggle, [/] seek ±5s, o open file picker
- File picker mode: Esc cancel, Enter submit, Backspace delete, Char append
- Slider adjustment uses `if let Some(sliders) = app.focused_sliders_mut()` — no-op when Transport focused

### Modified Files (2)

**`src/lib.rs`** — Added `pub mod app; pub mod input; pub mod ui;` alongside existing `pub mod audio`.

**`src/main.rs`** — Full rewrite from raw crossterm CLI to ratatui TUI:
- `TerminalGuard` RAII struct restores terminal on drop (including panics)
- `Terminal<CrosstermBackend<Stdout>>` for ratatui rendering
- `cpal::Stream` stored as `Option` in `main()` (not in `AppState` — it's not `Send`)
- Optional file load from CLI arg; graceful error display in status bar
- 30 fps event loop: `draw()` → `poll(33ms)` → `handle_key_event()` → match `Action`
- `load_file()` helper: decode → build `FileInfo` → start playback → replace `AppState.playback`

## Key Design Decisions

1. **`Action` enum for side effects** — `handle_key_event` mutates `AppState` directly for UI state (focus, sliders, mode) but returns `Action` for operations that need resources outside AppState (quit, file loading with cpal::Stream). Keeps the handler testable.

2. **`cpal::Stream` stays in `main()`** — cpal streams are `!Send`, so they can't live in `AppState` which gets borrowed across the draw closure boundary. Stored as `Option<Stream>` in main; replaced wholesale on new file load (old stream dropped automatically).

3. **`focused_sliders_mut() -> Option`** — Instead of returning a wrong panel (the original code returned `world_sliders` for the Transport case with an "unreachable in practice" comment), the method returns `None` for Transport. Callers use `if let Some(...)` which naturally no-ops.

4. **Slider selection is per-focus, single index** — `selected_slider` is a single `usize` shared across panels. On Tab, it's clamped to the new panel's slider count. This is simpler than storing per-panel selection and sufficient since both panels have 6 sliders.

5. **No new dependencies** — Everything uses `ratatui 0.30` (already in Cargo.toml). crossterm is re-exported via `ratatui::crossterm` per project convention.

## Slider Definitions

| Panel | Slider | Range | Default | Step | Unit |
|-------|--------|-------|---------|------|------|
| WORLD | Pitch Shift | ±12 | 0.0 | 0.5 | st |
| WORLD | Pitch Range | 0.2–3.0 | 1.0 | 0.1 | × |
| WORLD | Speed | 0.5–2.0 | 1.0 | 0.05 | × |
| WORLD | Breathiness | 0.0–3.0 | 0.0 | 0.1 | × |
| WORLD | Formant Shift | ±5 | 0.0 | 0.5 | st |
| WORLD | Spectral Tilt | ±6 | 0.0 | 0.5 | dB/oct |
| Effects | Gain | ±12 | 0.0 | 0.5 | dB |
| Effects | Low Cut | 20–500 | 20.0 | 10.0 | Hz |
| Effects | High Cut | 2k–20k | 20000.0 | 500.0 | Hz |
| Effects | Compressor | -40–0 | 0.0 | 1.0 | dB |
| Effects | Reverb Mix | 0.0–1.0 | 0.0 | 0.05 | (none) |
| Effects | Pitch Shift FX | ±12 | 0.0 | 0.5 | st |

## Key Bindings

| Key | Mode | Action |
|-----|------|--------|
| q / Esc | Normal | Quit |
| Space | Normal | Toggle play/pause |
| Tab | Normal | Cycle focus (World → Effects → Transport) |
| Up/Down | Normal | Navigate slider selection |
| Left/Right | Normal | Adjust slider ±1 step |
| Shift+Left/Right | Normal | Fine adjust ±0.2 step |
| r | Normal | Toggle loop |
| [ / ] | Normal | Seek ±5s |
| o | Normal | Enter FilePicker mode |
| Esc | FilePicker | Cancel, return to Normal |
| Enter | FilePicker | Load typed path |
| Backspace | FilePicker | Delete last char |
| Char(c) | FilePicker | Append to input |

## Code Review Findings (post-implementation)

Two issues found and fixed during review:

1. **`focused_sliders_mut()` Transport arm** — Originally returned `&mut self.world_sliders` with a comment "unreachable in practice". Changed to `Option<&mut Vec<SliderDef>>` returning `None`. Handler call sites updated to `if let Some(sliders)`.

2. **`transport.rs` redundant `meta_str`** — A `format!()` string was allocated only for `.len()`, then the same content was reconstructed inline. Refactored to use a single `loop_str` variable for both length calculation and rendering.

No other issues: zero clippy warnings, no unused imports/fields/variants, consistent naming, all pub exports used.

## Verification

- `cargo clippy --workspace` — zero warnings
- `cargo test --workspace` — 15/15 pass (4 decoder + 11 WORLD FFI, all unchanged)
- `cargo run` — TUI renders with empty state, "No file loaded" in status bar, q quits cleanly
- `cargo run -- assets/test_samples/test_stereo.wav` — file info shown, playback works, all keys functional

## Placeholders for Future Phases

- `ab_original: bool` — A/B comparison toggle placeholder (P4)
- `loop_enabled: bool` — wired to UI display but not yet to playback loop logic (P3/P4)
- `SliderDef.default` — stored but not yet used for reset-to-default (P8 polish)
- `audio_data: Option<Arc<AudioData>>` — stored for future WORLD analysis (P3)
- Spectrum panel — placeholder text, real FFT visualization in P5
- Slider values — displayed and adjustable but not yet wired to WORLD/effects processing (P3/P6)
