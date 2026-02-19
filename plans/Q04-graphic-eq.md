# Q04 — Graphic EQ Post-Effects Implementation Plan

**Phase:** P5-P6 Expansion (Post-Interactive File Picker)
**Scope:** 12-band parametric EQ + post-effects chain integration
**Status:** Ready for implementation

---

## Overview

Implement a **12-band graphic EQ** as a post-processing stage after WORLD synthesis. The EQ allows per-frequency gain adjustment (±6 dB) across standard audio bands (31 Hz – 16 kHz). The UI provides:
- **12 vertical bar columns** showing gain magnitude (boost = cyan above center, cut = red below)
- **Real-time adjustment** via arrow keys with debounced reapplication
- **Full integration** with existing effects slider panel
- **Comprehensive test coverage** for filter implementation and parameter validation

---

## Design Goals

1. **User Experience**
   - Visual feedback: bars extend above/below center line based on gain direction
   - Precise control: ±6 dB range, 0.1 dB step resolution
   - Responsive: immediate visual update on input, debounced DSP recompute (150 ms)
   - Focus-aware: highlight selected band when EQ has keyboard focus

2. **Implementation Quality**
   - Efficient: cascade biquad filters (fast IIR, O(n) processing)
   - Stable: proper filter coefficient calculation, numerical stability checks
   - Tested: unit tests for filter creation, parameter bounds, and frequency response
   - Documented: clear separation between UI (panel) and DSP (effects)

3. **Integration**
   - Works seamlessly with existing WORLD modifiers
   - Fits alongside other effects (gain, compression, reverb, filters)
   - Thread-safe: gains passed to processing thread via AppState
   - Hot-swappable: can be bypassed without stream rebuild

---

## Architecture

### Module Structure

```
src/dsp/effects.rs
├── apply_eq(samples, gains) → filtered output
├── create_eq_filters(gains) → [BiquadFilter; 12]
├── BiquadFilter { b0, b1, b2, a1, a2, z1, z2 }
└── validate_eq_gains(gains) → Result<(), String>

src/ui/eq_panel.rs
├── render(frame, area, gains, selected, focused)
├── EQ_FREQS: [&str; 12] = ["31", "63", ..., "16k"]
└── Visualization: columns + center line + frequency labels

src/app.rs (AppState)
└── eq_gains: [f64; 12]  // ±6 dB, default 0.0
```

### EQ Bands (12 frequencies)

| Band | Frequency | Typical Use |
|------|-----------|-------------|
| 0 | 31 Hz | Sub-bass |
| 1 | 63 Hz | Bass |
| 2 | 125 Hz | Low-mid |
| 3 | 250 Hz | Presence (low) |
| 4 | 500 Hz | Midrange |
| 5 | 1 kHz | Presence (mid) |
| 6 | 2 kHz | Presence (high) |
| 7 | 3.1 kHz | Presence (upper) |
| 8 | 4 kHz | Clarity |
| 9 | 6.3 kHz | Brilliance |
| 10 | 10 kHz | Air |
| 11 | 16 kHz | Sparkle |

### Processing Pipeline

```
Audio Buffer (f32)
  ↓
WORLD synthesis (or original pass-through)
  ↓
Effects chain:
  - EQ (12 cascaded biquad filters) ← NEW
  - Compression
  - Reverb
  - Gain
  ↓
Output (f32)
```

### Filter Specification

**Type:** Peaking EQ (IIR biquad)
**Order:** 2nd-order (5 coefficients per band)
**Cascade:** All 12 bands applied serially (stable up to ±12 dB total gain)
**Q Factor:** ~0.707 (Butterworth-like, ±6 dB shelf per band)

**Difference equation:**
```
y[n] = b0*x[n] + b1*x[n-1] + b2*x[n-2] - a1*y[n-1] - a2*y[n-2]
```

---

## Implementation Steps

### Step 1: EQ DSP Implementation (`src/dsp/effects.rs`)

**Create `apply_eq()` function:**
```rust
pub fn apply_eq(samples: &[f32], gains: &[f64; 12]) -> Vec<f32> {
    let filters = create_eq_filters(gains);
    let mut state: [[f32; 2]; 12] = [[0.0; 2]; 12];  // z1, z2 for each band

    samples.iter().map(|&x| {
        let mut y = x;
        for (band_idx, filter) in filters.iter().enumerate() {
            y = apply_biquad(y, filter, &mut state[band_idx]);
        }
        y
    }).collect()
}
```

**Create `create_eq_filters()` function:**
- Calculate center frequency (Hz) for each band
- Compute biquad coefficients using peaking EQ formula
- Validate coefficients (NaN, Inf checks)
- Return `[BiquadFilter; 12]`

**Add `BiquadFilter` struct:**
```rust
pub struct BiquadFilter {
    pub b0: f32, pub b1: f32, pub b2: f32,
    pub a1: f32, pub a2: f32,
}
```

**Add `apply_biquad()` helper:**
- Direct Form II implementation
- Maintains z1, z2 (filter state) per band
- Returns filtered sample

### Step 2: UI Panel Implementation (`src/ui/eq_panel.rs`)

**Create `render()` function:**
- Draw 12 vertical columns (one per band)
- Calculate bar height from gain magnitude and available rows
- Render cyan bars (█) above center for boosts
- Render red bars (█) below center for cuts
- Show center line (0 dB) with marker (▸ if selected)
- Display frequency label at bottom
- Display gain value at top (e.g., "+2.5", "-1.0")
- Highlight selected band with bright foreground + background

**Rendering constraints:**
- Available display area: 6–8 rows (after borders)
- Range: -6 dB (bottom) to +6 dB (top)
- Column width: `display_width / 12`
- Minimum width requirement: ~12 characters

### Step 3: AppState Integration (`src/app.rs`)

**Add field:**
```rust
pub eq_gains: [f64; 12],  // Default: [0.0; 12]
```

**Add accessor:**
```rust
pub fn eq_params(&self) -> EqParams {
    EqParams {
        gains: self.eq_gains.map(|g| g as f32),
    }
}
```

### Step 4: Input Handler Integration (`src/input/handler.rs`)

**In `handle_normal()`, add EQ navigation (when EQ panel has focus):**
- `Up` / `Down`: Move selection to adjacent band (circular)
- `Left` / `Right`: Decrease/increase gain by ±0.1 dB (Shift: ±0.5 dB)
- `Home`: Jump to band 0
- `End`: Jump to band 11
- `d`: Reset selected band to 0.0 dB
- `Tab`: Cycle to next panel (EQ ↔ World Sliders ↔ Effects ↔ Transport)

**Action dispatch:**
- `Action::ReapplyEffects` on gain change (triggers recompute in processing thread)

### Step 5: Processing Thread Integration (`src/dsp/processing.rs`)

**In `apply_fx_chain()`, add EQ stage:**
```rust
// After WORLD synthesis, before compression
let after_eq = if use_effects {
    apply_eq(&buffer, &eq_gains)
} else {
    buffer
};
```

**Thread safety:**
- `eq_gains` read from `AppState` (atomic/immutable during processing)
- No mutex needed (single reader/writer pattern maintained)

### Step 6: Tests (`tests/test_effects.rs`)

**Minimum test coverage:**
1. **Filter Creation**
   - `test_eq_filter_creation_valid_gains()` — Create filters for [-6, 0, +6] dB
   - `test_eq_filter_coeff_bounds()` — Coefficients within [-10, 10] range

2. **Processing**
   - `test_eq_unity_gain()` — Zero dB all bands → output ≈ input
   - `test_eq_preserves_length()` — Output length matches input
   - `test_eq_boost_increases_energy()` — +6 dB boost → RMS increases
   - `test_eq_cut_decreases_energy()` — -6 dB cut → RMS decreases

3. **Parameter Validation**
   - `test_eq_bounds_enforcement()` — Gains outside ±6 dB rejected or clamped
   - `test_eq_nan_rejection()` — NaN gains rejected
   - `test_eq_stability()` — Large cuts don't cause clipping/saturation

---

## Data Flow

```
User Input (keyboard)
  ↓ [Up/Down/Left/Right/d/Home/End]
  ↓ [in handle_normal() when EQ focused]
  ↓
AppState.eq_gains[selected_band] ← updated
  ↓
Action::ReapplyEffects
  ↓ [main thread → processing thread via channel]
  ↓
ProcessingCommand::ApplyFx { eq_gains, ... }
  ↓
processing_loop() receives command
  ↓
apply_fx_chain(buffer, eq_gains, ...)
  ↓
apply_eq(buffer, eq_gains) ← 12 cascaded biquads
  ↓
Output buffer (stored in AppState)
  ↓ [via RwLock swap]
  ↓
Audio callback reads and plays
```

---

## Testing & Verification Checklist

- [ ] `cargo build` — Compiles without warnings
- [ ] `cargo clippy --all-targets -- -D warnings` — Zero warnings
- [ ] `cargo test --all-targets` — All tests pass (including 7 EQ tests)
- [ ] Manual: Adjust EQ band Up/Down → visual feedback immediate
- [ ] Manual: Verify bars extend above center for +dB, below for -dB
- [ ] Manual: Change EQ with audio playing → sound changes smoothly (no artifacts)
- [ ] Manual: All 12 bands respond correctly across range
- [ ] Manual: Tab cycles through all panels including EQ
- [ ] Manual: Home/End jump to first/last band
- [ ] Manual: `d` resets selected band to 0 dB
- [ ] Manual: Verify no clipping at extreme boost combinations

---

## Known Constraints & Assumptions

1. **12 bands only** — Fixed design (not user-configurable)
2. **Cascade order** — Bands applied in frequency order (31 Hz → 16 kHz)
3. **Biquad precision** — Uses f32 coefficients; stable for ±6 dB per band (±12 dB total in cascade)
4. **No multitrack EQ** — Single EQ chain, not per-band independent processing
5. **Processing thread** — EQ recompute debounced at 150 ms (same as WORLD slider debounce)

---

## Success Criteria

✓ **Functional**
- 12 bands adjust independently via UI
- Gain changes audible (verified by listening)
- Processing thread applies EQ correctly

✓ **Robust**
- All edge cases tested (extreme gains, empty buffers, parameter bounds)
- Zero clippy warnings
- All tests passing

✓ **Integrated**
- Fits seamlessly in effects chain (before/after other effects per design)
- Panel renders correctly at various terminal widths
- No stream rebuild needed (reuses existing effect reapplication pipeline)

✓ **Documented**
- Code comments explain filter math
- UI panel behavior documented in help
- Implementation report captures design decisions

---

## Commits Expected

1. `feat: implement 12-band graphic EQ (DSP + filtering)`
2. `feat: add EQ panel UI with vertical bar visualization`
3. `feat: integrate EQ into effects chain and processing thread`
4. `test: add comprehensive EQ tests (filter creation, processing, validation)`
5. `fix: render negative EQ gains (cut bars below center)` ← Bug fix after initial implementation
6. `docs: add Q04 EQ implementation report`

---

## Timeline

**Estimate:** 2–3 hours total
- DSP implementation: ~45 min (filter math, biquad cascade)
- UI implementation: ~30 min (panel rendering, bar logic)
- Integration: ~20 min (AppState, handler, processing thread)
- Testing: ~30 min (7 tests, manual verification)
- Documentation: ~15 min

---

## Related Files

- `src/dsp/effects.rs` — Main EQ implementation
- `src/ui/eq_panel.rs` — Panel UI
- `src/app.rs` — AppState field + accessor
- `src/input/handler.rs` — Keyboard input
- `src/dsp/processing.rs` — Effects chain integration
- `tests/test_effects.rs` — Unit tests
- `plans/Q04-graphic-eq.md` — This file
- `implementations/Q04-graphic-eq-implementation.md` — Final report

