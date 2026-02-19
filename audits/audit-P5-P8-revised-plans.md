# P5–P8 Plan Alignment Review

**Date:** 2026-02-19
**Trigger:** Review plans P5-P8 against codebase after P4 + P4b completion

This is a read-only review — no implementation. Identifies what each plan gets right,
naming errors, missing pieces, and conflicts per file.

---

## Current Codebase State (reference)

**`AppState` key fields:** `world_sliders: Vec<SliderDef>`, `effects_sliders: Vec<SliderDef>`,
`audio_data: Option<Arc<AudioData>>`, `original_audio: Option<Arc<AudioData>>`,
`loop_enabled: bool`, `ab_original: bool`, `mode: AppMode` (`Normal` | `FilePicker`)

**`Action` variants:** `Quit`, `LoadFile(String)`, `Resynthesize`, `ToggleAB`

**`ProcessingCommand` variants:** `Analyze(Arc<AudioData>)`, `Resynthesize(WorldSliderValues)`, `Shutdown`

**`Cargo.toml`:** `world-sys`, `cpal`, `crossbeam-channel`, `symphonia`, `ratatui`
— `hound = "3.5"` is in `[dev-dependencies]` only

**Active key bindings:** Space=play/pause, Tab=cycle panels, ↑↓=select slider,
←→=adjust slider / seek±5s (Transport), `[`/`]`=seek±5s, Home/End=jump start/end,
`r`=loop toggle, `a`=A/B toggle, `o`=open file picker, q/Esc=quit

---

## P5 — Spectrum Display

**Status: ALIGNED** — architecture correct, only additive work needed.

| File | Change |
|------|--------|
| `Cargo.toml` | Add `rustfft = "6.4"` to `[dependencies]` |
| `src/dsp/spectrum.rs` | **Create** — `SpectrumData` + `compute_spectrum()` |
| `src/dsp/mod.rs` | Add `pub mod spectrum;` |
| `src/app.rs` | Add `spectrum_window: Option<Arc<Mutex<Vec<f32>>>>` for callback tap |
| `src/audio/playback.rs` | Tap 2048 samples in callback into `Arc<Mutex<Vec<f32>>>`; expose via `PlaybackState` |
| `src/ui/spectrum.rs` | Replace placeholder; accept `AppState` (or spectrum data) param |
| `src/ui/layout.rs` | Pass app/spectrum data to `spectrum::render()` |
| `tests/test_spectrum.rs` | **Create** — 440 Hz sine → peak at bin ≈ 20 |

No naming errors in the plan.

---

## P6 — Effects Chain

**Status: MOSTLY ALIGNED** — one naming error, several missing enum variants.

### Naming error in plan text:
- `AppState.sliders[6..12]` → **actual**: `AppState.effects_sliders` (a separate `Vec<SliderDef>`)

### What needs adding:

| File | Change |
|------|--------|
| `Cargo.toml` | Add `fundsp = { version = "0.23", default-features = false, features = ["std"] }` and `pitch_shift = "1"` |
| `src/app.rs` | Add `Action::ReapplyEffects` variant |
| `src/dsp/processing.rs` | Add `ProcessingCommand::ReapplyEffects(EffectsParams)` variant; cache post-WORLD raw PCM |
| `src/dsp/effects.rs` | **Create** — `EffectsParams` struct + `apply_effects()` |
| `src/dsp/mod.rs` | Add `pub mod effects;` |
| `src/input/handler.rs` | Effects slider Left/Right returns `None` → change to `Some(Action::ReapplyEffects)` |
| `src/main.rs` | Handle `Action::ReapplyEffects` → send `ProcessingCommand::ReapplyEffects` |

---

## P7 — WAV Export

**Status: MOSTLY ALIGNED** — one naming error, `hound` in wrong dep section.

### Naming error in plan text:
- `processed_pcm` buffer → **actual**: `app.audio_data: Option<Arc<AudioData>>`

### What needs adding:

| File | Change |
|------|--------|
| `Cargo.toml` | Move `hound = "3.5"` from `[dev-dependencies]` → `[dependencies]` |
| `src/app.rs` | Add `AppMode::Save`, `Action::SaveFile(String)` |
| `src/audio/export.rs` | **Create** — `export_wav()` |
| `src/audio/mod.rs` | Add `pub mod export;` |
| `src/input/handler.rs` | Add `'s'` key binding → `AppMode::Save` |
| `src/main.rs` | Handle `Action::SaveFile` → call `export_wav()`, update `status_message` |
| `src/ui/` | Save dialog overlay (can reuse `file_picker` pattern) |

---

## P8 — Polish

**Status: KEY BINDING CONFLICTS — plan edits required before implementation.**

### Key binding errors in plan:

| Plan says | Actual | Severity |
|-----------|--------|----------|
| Help: `Tab` → "Switch A/B" | `Tab` cycles panels; A/B is `'a'` | **WRONG** |
| 8.5: `A` → "Advanced settings" | `'a'` is already A/B toggle | **CONFLICT** — pick new key (e.g., `'v'`) |
| Help missing `Home`/`End` | Added in P4b | **MISSING** |
| Help missing Transport `←/→` | Added in P4b | **MISSING** |
| 8.2: debounce "300ms" | Already implemented at 150ms | **STALE** |

### Partially / already done:

| P8 item | Status |
|---------|--------|
| 8.2 Resynthesis debounce | Done — 150ms in `main.rs:RESYNTH_DEBOUNCE` |
| 8.8 Terminal restore | Partial — `TerminalGuard` RAII covers panic Drop; `std::panic::set_hook` not installed |

### Gap: loop not enforced

`loop_enabled` field and `'r'` binding exist, but `write_audio_data()` in `playback.rs` never
checks it — playback never actually loops. P8 should add an explicit sub-task:
- Add `loop_enabled: Arc<AtomicBool>` to `CallbackContext`
- When `pos >= total_samples && loop_enabled` → reset `pos = 0` instead of outputting silence

### Corrected help screen for P8:

```
┌─ Help ──────────────────────────────┐
│  Space      Play / Pause            │
│  Tab        Cycle panels            │
│  ↑/↓        Select slider           │
│  ←/→        Adjust slider / Seek±5s │
│  Shift+←/→  Fine adjust (0.2 step)  │
│  [/]        Seek ±5s                │
│  Home/End   Jump to start/end       │
│  a          Toggle A/B comparison   │
│  r          Toggle loop             │
│  s          Save processed WAV      │
│  o          Open file               │
│  ?          This help               │
│  q/Esc      Quit                    │
│                                     │
│  Press any key to close             │
└─────────────────────────────────────┘
```

---

## Summary

| Plan | Status | New files | Naming errors | Conflicts |
|------|--------|-----------|---------------|-----------|
| P5 Spectrum | Ready | `src/dsp/spectrum.rs`, `tests/test_spectrum.rs` | None | None |
| P6 Effects | Ready (after P5) | `src/dsp/effects.rs` | `sliders[6..12]` → `effects_sliders` | None |
| P7 Export | Ready (after P6) | `src/audio/export.rs` | `processed_pcm` → `audio_data` | `hound` in dev-deps |
| P8 Polish | **Needs plan edits** | — | Help screen: Tab≠A/B, missing P4b bindings | `A` key conflicts with A/B toggle |

**Required plan corrections before implementing P8:**
1. Fix help screen: Tab=cycle panels (not A/B), `a`=A/B toggle
2. Choose a non-conflicting key for advanced settings (not `A`)
3. Add Home/End and Transport ←/→ to help screen
4. Correct debounce from 300ms → 150ms (or remove — already done)
5. Add loop enforcement as explicit sub-task in P8
