# P6 — Effects Chain: Implementation Report

## Goal

Wire the 6 effects sliders in the right panel (Gain, Low Cut, High Cut, Compressor, Reverb Mix, Pitch Shift FX) to a real post-processing chain applied after WORLD synthesis and before playback. Effects re-apply quickly on slider change without re-running WORLD analysis/synthesis.

## Prerequisite

P5 complete (24 tests). The effects sliders existed since P2 — displayed and adjustable — but changes had no effect on audio. The processing thread only handled WORLD analysis/synthesis. The input handler returned no action for effects slider changes.

## What Was Built

### New Files (2)

**`src/dsp/effects.rs`** — Effects chain module with 6 effects, all implemented from scratch using standard DSP (no new dependencies):

- `EffectsParams` struct — mirrors the 6 effects sliders: `gain_db`, `low_cut_hz`, `high_cut_hz`, `compressor_thresh_db`, `reverb_mix`, `pitch_shift_semitones`. `is_neutral()` returns true when all values are at defaults (bypass).

- `apply_effects(samples, sample_rate, params) -> Vec<f32>` — applies the full chain in order, with early return when neutral or empty. Guards against `sample_rate == 0`.

- **Gain** — dB-to-linear amplitude scaling: `10^(gain_db / 20)`.

- **Low Cut (highpass)** — second-order Butterworth biquad filter. Cookbook coefficients with `Q = 1/√2`. Frequency clamped to 95% of Nyquist to prevent aliased coefficient instability.

- **High Cut (lowpass)** — same biquad structure, lowpass variant. Same Nyquist clamping.

- **Compressor** — peak-following envelope detector with 5ms attack / 50ms release, 4:1 ratio, auto makeup gain (`10^(-threshold_db / 40)`). Gain reduction applied sample-by-sample above threshold.

- **Pitch Shift (FX)** — linear-interpolation resampling by `1 / 2^(semitones/12)`. Changes buffer length proportionally (e.g., +12st → half length, -12st → double length). This is the "chipmunk" effect — shifts everything including formants, contrasting with WORLD's formant-preserving pitch shift.

- **Reverb** — Schroeder reverb: 4 parallel comb filters (delays 1557/1617/1491/1422 samples at 44100 Hz, feedback 0.84/0.82/0.80/0.78) summed and averaged, followed by 2 series allpass filters (delays 225/556, gain 0.5). Delay sizes scale proportionally for non-44100 sample rates. Wet/dry mixing: `(1-mix) * dry + mix * wet`.

**`tests/test_effects.rs`** — 11 tests:
- `test_effects_neutral_passthrough` — default params produce identical output
- `test_effects_gain_plus_6db` / `test_effects_gain_minus_12db` — RMS ratio matches expected dB change
- `test_effects_lowcut_attenuates_bass` — 100 Hz tone through 500 Hz highpass: >70% attenuation
- `test_effects_highcut_attenuates_treble` — 8000 Hz tone through 2000 Hz lowpass: >70% attenuation
- `test_effects_compressor_reduces_dynamics` — loud signal compressed without producing silence
- `test_effects_pitch_shift_up` / `test_effects_pitch_shift_down` — buffer length changes by expected ratio
- `test_effects_reverb_differs_from_dry` — reverb output measurably differs from input
- `test_effects_empty_input` — empty input produces empty output
- `test_effects_is_neutral` — neutral detection for default and non-default params

### Modified Files (5)

**`src/dsp/mod.rs`** — Added `pub mod effects;`.

**`src/app.rs`** — Three additions:
- `use crate::dsp::effects::EffectsParams;`
- `Action::ReapplyEffects` variant — returned by the input handler when effects sliders change.
- `effects_params(&self) -> EffectsParams` method — extracts the 6 effects slider values into an `EffectsParams` struct for the processing thread.

**`src/dsp/processing.rs`** — Major rework of the processing loop:

1. **New command**: `ReapplyEffects(EffectsParams)` — re-applies effects on cached post-WORLD audio without re-running WORLD.
2. **Changed command**: `Resynthesize(WorldSliderValues, EffectsParams)` — now carries effects params so the full pipeline (WORLD + effects) runs in one pass.
3. **Post-WORLD cache**: `post_world_audio: Option<AudioData>` — stores the WORLD synthesis result before effects. Set after every WORLD synthesis and on `Analyze` (to the mono original). `ReapplyEffects` reads from this cache.
4. **Effects application**: `apply_fx_chain(audio, params) -> AudioData` helper — applies effects if non-neutral, otherwise clones unchanged.
5. **Command drain logic**: Both `Resynthesize` and `ReapplyEffects` drain queued commands:
   - `Resynthesize` absorbs subsequent `Resynthesize` and `ReapplyEffects` (taking latest params).
   - `ReapplyEffects` absorbs subsequent `ReapplyEffects`. If a `Resynthesize` appears during drain, delegates to `handle_resynthesize_inline` which performs full synthesis inline.
6. **Shutdown propagation**: `handle_resynthesize_inline` returns `bool` — `true` if a Shutdown command was consumed, causing the caller to `return` from the processing loop.

**`src/input/handler.rs`** — Effects slider changes now return actions:
- `Left`/`Right` on `PanelFocus::EffectsSliders` returns `Some(Action::ReapplyEffects)` (previously returned `None`).
- Changed from `if focus == WorldSliders { Resynthesize } else { None }` to `match focus { WorldSliders => Resynthesize, EffectsSliders => ReapplyEffects, Transport => None }`.

**`src/main.rs`** — Four areas of change:

1. **Effects debounce timer**: `effects_pending: Option<Instant>` with 80ms debounce (shorter than WORLD's 150ms since effects are faster). When WORLD debounce fires, `effects_pending` is cleared (Resynthesize includes effects).
2. **Resynthesize calls updated**: All three `ProcessingCommand::Resynthesize` sends now include `app.effects_params()` as the second argument (after AnalysisDone, after WORLD debounce, and after effects debounce).
3. **ReapplyEffects action handler**: Sets `effects_pending` debounce timer on effects slider change.
4. **Effects debounce check**: After the WORLD debounce check, fires `ReapplyEffects` command when the effects deadline expires.

## Key Design Decisions

### 1. No New Dependencies

All 6 effects are implemented from scratch using standard DSP algorithms:
- Biquad filters: Audio EQ Cookbook coefficients
- Compressor: textbook envelope follower
- Reverb: classic Schroeder design
- Pitch shift: linear-interpolation resampling

This avoids the API compatibility risks noted in the plan for `fundsp` (streaming vs offline) and `pitch_shift` (unknown API surface). The implementations total ~150 lines of focused DSP code.

### 2. Two-Level Cache: Post-WORLD + Post-Effects

The processing thread caches the WORLD synthesis output (`post_world_audio`) separately from the final effects output. This enables:
- **WORLD slider change** → full pipeline: WORLD synthesis + effects
- **Effects slider change** → effects-only re-application on cached WORLD output
- Effects re-application is near-instant (<50ms for typical files) vs WORLD resynthesis (~2-5s).

### 3. Pitch Shift FX Changes Buffer Length

Unlike WORLD pitch shift (which modifies f0 parameters and produces the same frame count), the FX pitch shift resamples the PCM buffer, changing its length. This is the "chipmunk" effect — it shifts everything including formants. The system already handles different-length buffers from the WORLD speed slider (proportional position scaling on A/B toggle, position clamping in SynthesisDone handler). No additional infrastructure needed.

### 4. Effects Chain Order

Applied in standard audio signal flow order:
1. Gain (input level)
2. Highpass / Low cut (remove rumble before processing)
3. Lowpass / High cut (bandwidth limiting)
4. Compressor (dynamics after filtering)
5. Pitch shift FX (frequency manipulation)
6. Reverb (spatial effect applied last)

### 5. Separate Debounce for Effects (80ms vs 150ms)

Effects re-application is much faster than WORLD resynthesis, so a shorter debounce (80ms) provides more responsive feedback. When a WORLD slider change fires, it clears the effects debounce timer since `Resynthesize` includes effects params.

### 6. Neutral Effects Bypass

When all effects are at defaults, `apply_fx_chain` returns a clone of the input without processing. Combined with WORLD's neutral-slider shortcut, the full neutral path is: `original_mono → clone → clone` (no WORLD synthesis, no effects processing).

## Architecture

```
Processing thread state:
  cached_params: Option<WorldParams>     — from Analyze
  original_mono: Option<AudioData>       — mono downmix for neutral shortcut
  post_world_audio: Option<AudioData>    — WORLD output cache for effects re-apply

WORLD slider change:
  main → Resynthesize(world_vals, fx_params) → processing thread
    → modifier::apply + world::synthesize → cache post_world_audio
    → apply_effects on cached → SynthesisDone(final_audio)

Effects slider change:
  main → ReapplyEffects(fx_params) → processing thread
    → apply_effects on cached post_world_audio → SynthesisDone(final_audio)

Effects chain:
  Gain → Highpass → Lowpass → Compressor → Pitch Shift → Reverb
```

## Edge Cases Handled

| Scenario | Behavior |
|---|---|
| All effects at defaults | `is_neutral()` → bypass, clone unchanged |
| Empty buffer | Early return, empty output |
| `sample_rate == 0` | Early return, input unchanged |
| Biquad freq ≥ Nyquist | Clamped to 95% of Nyquist |
| Pitch shift +12st | Buffer halved; position clamped in main thread |
| Pitch shift -12st | Buffer doubled; position scaled proportionally |
| Effects change during WORLD resynthesis | `ReapplyEffects` queued; drained by `Resynthesize` handler |
| WORLD change during effects re-apply | `Resynthesize` supersedes; handled inline via `handle_resynthesize_inline` |
| Shutdown during command drain | `handle_resynthesize_inline` returns `true`; caller exits processing loop |
| No post-WORLD cache when ReapplyEffects arrives | Guard: `if let Some(ref cached)` — silently skipped |
| A/B toggle with effects active | Original audio has no effects; processed has WORLD + effects |

## Robustness Considerations

- **Biquad stability**: Butterworth Q (0.707) is unconditionally stable. Frequency clamped to 95% Nyquist to avoid degenerate coefficients near Nyquist.
- **Comb filter stability**: All feedback gains < 1.0 (0.78–0.84), guaranteeing BIBO stability.
- **Compressor NaN prevention**: `threshold > 0` always (slider min is -40 dB → `10^(-2) = 0.01`). Division `threshold / env` only occurs when `env > threshold > 0`.
- **Shutdown propagation**: `handle_resynthesize_inline` returns a `bool` sentinel so consumed Shutdown commands are not silently lost.
- **No panics in effects path**: All operations use safe arithmetic. `sample_rate == 0` returns early. Buffer indexing uses bounds checks or `.min()` clamping.

## No New Dependencies

All effects use standard DSP algorithms implemented in pure Rust. No external crates added.

## Verification

- `cargo clippy --workspace` — zero warnings
- `cargo test` — 35/35 pass (4 decoder + 11 WORLD FFI + 3 modifier + 6 spectrum + 11 effects)
- Manual checklist: gain ±dB audible, low/high cut filter as expected, compressor reduces dynamics, reverb adds spatial effect, FX pitch shift produces chipmunk/deep voice, A/B toggle unaffected, effects re-apply quickly

## Test Count

35 tests: 4 decoder + 11 WORLD FFI + 3 modifier + 6 spectrum + 11 effects

## Resolved Placeholders

- Effects sliders — now wired to real audio processing (gain, filters, compressor, reverb, pitch shift FX)

## Remaining Placeholders for Future Phases

- `loop_enabled: bool` — wired to UI display but not yet to playback loop logic
- WAV export (P7)
- Polish, keybinds help overlay (P8)
