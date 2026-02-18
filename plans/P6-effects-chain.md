# P6 — Effects Chain (Post-Processing)

## Goal
Implement the 6 effects sliders (right panel): gain, low cut, high cut, compressor, reverb mix, and phase vocoder pitch shift. These are applied as post-processing on the resynthesized PCM buffer, after WORLD synthesis and before playback.

## Prerequisite
P5 complete (full pipeline with spectrum display working).

## Steps

### 6.1 Add dependencies
```toml
fundsp = { version = "0.23", default-features = false, features = ["std"] }
pitch_shift = "1"
```

### 6.2 Effects module — `src/dsp/effects.rs`
```rust
pub struct EffectsParams {
    pub gain_db: f32,           // -12 to +12
    pub low_cut_hz: f32,       // 20 to 500
    pub high_cut_hz: f32,      // 2000 to 20000
    pub compressor_thresh_db: f32, // -40 to 0
    pub reverb_mix: f32,       // 0.0 to 1.0
    pub pitch_shift_semitones: f32, // -12 to +12
}

pub fn apply_effects(samples: &[f32], sample_rate: u32, params: &EffectsParams) -> Vec<f32> { ... }
```

### 6.3 Individual effects implementation

**Gain**: Simple amplitude scaling: `sample * 10.0_f32.powf(gain_db / 20.0)`

**Low Cut (High-pass filter)**: Use fundsp `highpass_hz(freq, q)` where q = 0.707 (Butterworth). Process entire buffer through the filter graph.

**High Cut (Low-pass filter)**: Use fundsp `lowpass_hz(freq, q)` similarly.

**Compressor/Limiter**: Use fundsp `limiter()` with the threshold parameter. Apply makeup gain proportional to threshold to maintain perceived loudness.

**Reverb**: Use fundsp `reverb_stereo()` or `reverb2_stereo()`. Mix wet/dry using `reverb_mix` parameter:
```
output = (1.0 - mix) * dry + mix * wet
```
For mono input, duplicate to stereo for reverb, then mix back to mono (or keep stereo if playback supports it).

**Pitch Shift (FX)**: Use `pitch_shift` crate's phase vocoder:
```rust
let factor = 2.0_f32.powf(semitones / 12.0);
pitch_shift::pitch_shift(factor, sample_rate, &input, &mut output);
```

### 6.4 Effects chain ordering
Apply in this order (matching typical audio signal flow):
1. Gain
2. High-pass (low cut)
3. Low-pass (high cut)
4. Compressor/Limiter
5. Pitch Shift (FX)
6. Reverb (applied last, after all other processing)

### 6.5 Integration into processing pipeline
Modify the processing thread:
1. WORLD modifier → WORLD synthesis → **effects chain** → processed_pcm buffer
2. When any effects slider changes, re-run only the effects chain (skip WORLD re-analysis/resynthesis if WORLD sliders haven't changed)
3. Cache the post-WORLD-synthesis buffer separately so effects can be re-applied quickly

### 6.6 Connect right panel sliders
The 6 effects sliders in the TUI right panel are already defined (P2). Wire them:
- Read current values from `AppState.sliders[6..12]`
- On change, send `ReapplyEffects` command to processing thread
- This should be fast (<100ms for typical files) since it's just DSP on PCM

### 6.7 Bypass behavior
When all effects are at default values (gain 0, low cut 20, high cut 20000, compressor 0, reverb 0, pitch FX 0), skip the effects chain entirely (passthrough) to avoid unnecessary processing and potential quality loss.

## Human Test Checklist

- [ ] Move Gain slider to +6 dB → audio is noticeably louder
- [ ] Move Gain slider to -12 dB → audio is very quiet
- [ ] Move Low Cut to 300 Hz → bass frequencies disappear, voice sounds thinner
- [ ] Move High Cut to 3000 Hz → audio sounds muffled (no high frequencies)
- [ ] Move Compressor threshold to -20 dB → dynamic range is reduced, quiet parts louder
- [ ] Move Reverb Mix to 0.5 → noticeable reverb/room effect on the audio
- [ ] Move Pitch Shift (FX) to +12 → chipmunk effect (everything shifts up including formants)
- [ ] Move Pitch Shift (FX) to -12 → deep/giant effect
- [ ] Compare WORLD Pitch Shift vs FX Pitch Shift: WORLD sounds natural, FX sounds "chipmunk"
- [ ] Combine multiple effects → they stack correctly
- [ ] A/B toggle still works: original has no effects, processed has both WORLD mods + effects
- [ ] Effects re-apply quickly when sliders change (no long wait like WORLD resynthesis)
- [ ] All defaults → audio sounds identical to WORLD-only output (no quality loss from passthrough)

## Dependencies Introduced
- `fundsp` 0.23
- `pitch_shift` 1

## Risk Notes
- fundsp processes samples in its own graph format. Need to convert between `Vec<f32>` and fundsp's processing pipeline. Check if batch/offline processing is straightforward or if fundsp expects streaming.
- Reverb on mono audio: fundsp reverb may expect stereo. May need mono→stereo→mono conversion.
- pitch_shift crate API: verify it supports offline (buffer) processing, not just streaming.
