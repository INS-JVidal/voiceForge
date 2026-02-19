# P6b — Live Gain Control: Implementation Report

## Goal

Move gain from the offline effects chain (processing thread, 80ms debounce + processing time) into the cpal audio callback for instant (~5ms) feedback. Other effects remain on the processing thread.

## Prerequisite

P6 complete (35 tests). Gain was applied as the first step of `apply_effects()` in the processing thread. Changes to the gain slider followed the same 80ms debounce → buffer swap path as all other effects, meaning audible feedback took 80–500ms depending on buffer size and effects chain cost.

## What Was Changed

### `src/audio/playback.rs` — 5 changes

1. Added `AtomicU32` to imports.
2. Added `live_gain: Arc<AtomicU32>` to `PlaybackState` — defaults to `1.0f32.to_bits()` (unity gain).
3. Added `live_gain: Arc<AtomicU32>` to `CallbackContext`.
4. Wired `live_gain` into `CallbackContext` in both `start_playback` and `rebuild_stream` (cloned from `PlaybackState`).
5. In `write_audio_data`: load `live_gain` once per callback (`Ordering::Relaxed`), multiply each sample by gain before `T::from_sample()` conversion.

```rust
let gain = f32::from_bits(ctx.live_gain.load(Ordering::Relaxed));
// ...per-sample:
*sample = T::from_sample(val * gain);
```

### `src/app.rs` — 1 change

Added `Action::LiveGain(f32)` variant to the `Action` enum. Carries the pre-computed linear multiplier (not dB).

### `src/input/handler.rs` — 2 changes

1. Extracted `effects_slider_action()` helper to replace duplicated `match focus` blocks in Left/Right handlers.
2. `effects_slider_action()` detects gain slider (index 0) and returns `Action::LiveGain(linear)` with `10^(gain_db/20)` conversion. Other effects sliders still return `Action::ReapplyEffects`.

### `src/main.rs` — 2 changes

1. **`Action::LiveGain` handler**: Stores the linear gain into the atomic via `to_bits()` with `Ordering::Relaxed`. No debounce — value is heard on the next audio callback (~5ms).
2. **`load_file` gain restore**: After `app.playback = state` (which creates a fresh `PlaybackState` with `live_gain = 1.0`), restores the gain from the current slider value so gain is preserved across file loads.

### `src/dsp/effects.rs` — 3 changes

1. Removed the `apply_gain` call from `apply_effects()` — gain is no longer part of the processing-thread chain.
2. Removed `gain_db == 0.0` check from `is_neutral()` — gain no longer affects whether the effects chain needs to run.
3. Made `apply_gain` `pub` — available for future WAV export (P7) and direct testing.

### `tests/test_effects.rs` — 3 changes

1. Added `apply_gain` to imports.
2. Rewrote `test_effects_gain_plus_6db` and `test_effects_gain_minus_12db` to call `apply_gain` directly (no longer testing via `apply_effects`).
3. Updated `test_effects_is_neutral` to assert that `gain_db: 1.0` IS neutral (gain is excluded from neutral check).

## Key Design Decisions

### 1. AtomicU32 for f32 Gain

`Arc<AtomicU32>` stores the f32 linear gain as raw bits via `to_bits()`/`from_bits()`. This is lock-free, wait-free, and has zero allocation. `Ordering::Relaxed` is used for both store and load — the audio callback sees the new value within a cache coherency round-trip (sub-microsecond on x86, low microseconds on ARM). No memory fence needed because there is no dependent data to synchronize.

### 2. Gain Applies to Both A and B

The `live_gain` atomic is read in the audio callback, which runs regardless of whether the A (original) or B (processed) buffer is active. This means gain acts as a **monitoring control** — it adjusts the output level of whatever the user is hearing. This is consistent with how gain works in professional audio tools (the gain knob on a mixing console affects the monitored signal, not the source).

### 3. Gain Preserved Across File Loads

When `load_file` creates a new `PlaybackState` (via `start_playback`), `live_gain` defaults to 1.0 (unity). The gain slider value is restored immediately after `app.playback = state`, so the user's gain setting persists across file loads.

### 4. No Clipping/Saturation

At +12 dB, the linear gain is ~3.98×. If the source audio is near 0 dBFS, output samples exceed [-1.0, 1.0]. This is standard behavior — cpal's `FromSample` clamps for integer formats (i16/u16), and f32 output relies on the audio driver/DAC for clipping. No software limiter is added to avoid masking the gain effect.

### 5. gain_db Retained in EffectsParams

`EffectsParams.gain_db` is still populated and sent to the processing thread (but ignored by `apply_effects`). This preserves the field for future WAV export (P7), which will need to bake gain into the exported buffer via `apply_gain()`.

## Architecture After Change

```
Gain slider change:
  handler → Action::LiveGain(linear) → main thread
    → AtomicU32 store (Relaxed) → heard next callback (~5ms)

Other effects slider change:
  handler → Action::ReapplyEffects → 80ms debounce → processing thread
    → apply_effects (without gain) → SynthesisDone → buffer swap

Audio callback (write_audio_data):
  gain = f32::from_bits(live_gain.load(Relaxed))
  for each sample:
    output = T::from_sample(buffer[pos] * gain)
```

## Edge Cases Handled

| Scenario | Behavior |
|---|---|
| Gain at 0 dB (default) | `linear = 1.0` — multiplication is identity |
| Gain at +12 dB | `linear ≈ 3.98` — may clip on integer output; standard behavior |
| Gain at -12 dB | `linear ≈ 0.25` — quiet but audible |
| File load with gain ≠ 0 dB | Gain restored from slider immediately after new PlaybackState |
| A/B toggle with gain active | Gain applies to both sides (monitoring control) |
| rebuild_stream fallback | Reuses existing PlaybackState including live_gain Arc |
| All effects neutral + gain active | `is_neutral() = true` → effects chain bypassed; gain still applied in callback |

## Robustness Considerations

- **No NaN/Inf risk**: The gain slider range is [-12, +12] dB. `10^(x/20)` for `x ∈ [-12, 12]` produces values in `[0.25, 3.98]` — always finite positive f32. `to_bits()`/`from_bits()` is a lossless round-trip for all finite f32 values.
- **No lock contention**: `AtomicU32` with `Relaxed` ordering has zero contention cost. The audio callback never blocks.
- **No panics in callback**: `from_bits()` never panics. Multiplication never panics. `T::from_sample()` never panics.

## No New Dependencies

Pure Rust atomics. No crates added.

## Verification

- `cargo clippy --workspace` — zero warnings
- `cargo test` — 35/35 pass (unchanged count; 3 tests updated, 0 added/removed)
- Manual checklist: gain slider → heard instantly during playback (~5ms), other effects → 80ms+ delay (unchanged), A/B toggle → gain applies to both, file load → gain preserved

## Test Count

35 tests: 4 decoder + 11 WORLD FFI + 3 modifier + 6 spectrum + 11 effects (3 updated)

## Resolved Placeholders

- Gain slider latency — reduced from ~80–500ms to ~5ms (audio callback speed)

## Remaining Placeholders for Future Phases

- `loop_enabled: bool` — wired to UI display but not yet to playback loop logic
- WAV export (P7) — will need to call `apply_gain()` explicitly since gain is excluded from `apply_effects()`
- Polish, keybinds help overlay (P8)
