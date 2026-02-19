# Q02 — Spectrum Frequency Labels on X-axis

**Date:** 2026-02-19
**Status:** ✅ Complete
**Plan:** `plans/Q02-spectrum-scale.md`

---

## Summary

Added **adaptive frequency labels** to the spectrum visualizer's X-axis. Labels intelligently appear based on terminal width, ranging from minimal (1k, 10k) at 51-80 columns to comprehensive (50, 100, 200, 500, 1k, 2k, 5k, 10k, 15k, 20k Hz) at 161+ columns.

The spectrum uses a **quadratic frequency mapping (t²)** that approximates log scale perceptually while remaining computationally simple. Labels make this scale explicit and provide clear frequency reference across the human-audible range.

---

## Key Implementation Details

### Quadratic Frequency Mapping

- **Forward map (bar rendering):** `bin = (bin_count - 1) × t²` where `t = col / (num_bars - 1)`
- **Inverse map (label positioning):** `t = √(bin / (bin_count - 1))`, then `col = t × (num_bars - 1)`
- Nyquist bin: 1024 (FFT_SIZE = 2048)
- Sample rate: extracted from `app.file_info` or defaults to 44100 Hz

### Adaptive Label Selection by Terminal Width

| Width | Labels | Purpose |
|-------|--------|---------|
| 0–50 cols | None | Too cramped |
| 51–80 cols | 1k, 10k | Very narrow; anchors only |
| 81–120 cols | 100, 1k, 5k, 10k, 20k | Half-width; clarity region |
| 121–160 cols | 100, 500, 1k, 5k, 10k, 20k | Standard terminal |
| 161+ cols | All 10 (50, 100, 200, 500, 1k, 2k, 5k, 10k, 15k, 20k) | Full screen |

Label set is selected once per render based on `num_bars` (terminal width), not recalculated dynamically.

### Overlap Prevention

- Build label row as vector of characters (one per column)
- Iterate through labels in frequency order (ascending)
- Track `last_col` — rightmost position of previously written label
- Skip label if:
  - `col + label.len() > num_bars` (overflows display)
  - `col < last_col` (overlaps with previous label)
- Write label only if both checks pass; update `last_col`

This ensures graceful degradation: if overlap occurs despite width-based selection, the label is simply skipped rather than mangled.

### Visual Design

- **Color:** Muted gray (`RGB 120, 120, 120`) to avoid competing with punk gradient bars
- **Gradient consistency:** Gradient calculation still uses full `inner_h`, not reduced `bar_rows`, ensuring consistent color distribution across visual height
- **Bottom row reservation:** Only when `inner_h >= 2`; smaller areas skip labels entirely

---

## Code Changes

### File: `src/ui/spectrum.rs`

**Lines 44-52:** Reserved label row space
```rust
let has_label_row = inner_h >= 2;
let bar_height = if has_label_row {
    inner_h.saturating_sub(1) as f32
} else {
    inner_h as f32
};
```

**Line 66:** Recalculated bar heights for reduced space
```rust
let h = ((db + 80.0) / 80.0 * bar_height).clamp(0.0, bar_height);
```

**Lines 71-75:** Render only bar rows (not label row)
```rust
let bar_rows = if has_label_row { inner_h - 1 } else { inner_h };
for r in 0..bar_rows { ... }
```

**Line 79:** Gradient uses full height for consistency
```rust
let row_ratio = level as f32 / inner_h as f32; // Note: full inner_h
```

**Lines 105-148:** Adaptive label selection by terminal width
```rust
let freq_labels: &[(f32, &str)] = match num_bars {
    0..=50 => &[],
    51..=80 => &[(1000.0, "1k"), (10000.0, "10k")],
    81..=120 => &[(100.0, "100"), ..., (20000.0, "20k")],
    // ... more cases ...
};
```

**Lines 150-172:** Compute positions and prevent overlap
```rust
for &(freq_hz, label_text) in freq_labels {
    let bin = (freq_hz * fft_size / sample_rate).clamp(0.0, (bin_count - 1) as f32);
    let t = (bin / (bin_count as f32 - 1.0)).sqrt();
    let col = (t * (num_bars as f32 - 1.0)).round() as usize;

    if col + label_text.len() <= num_bars && col >= last_col {
        // Write label
    }
}
```

**Lines 173-176:** Convert to styled Line with gray color
```rust
let label_spans: Vec<Span> = label_row
    .iter()
    .map(|&ch| Span::styled(ch.to_string(), Style::default().fg(label_color)))
    .collect();
```

---

## Testing & Verification

### Build & Lint
```bash
✓ cargo build — Clean compilation
✓ cargo test --all-targets — All 52 tests passing
✓ cargo clippy --all-targets -- -D warnings — Zero warnings
```

### Manual Testing (Visual)

Tested at multiple terminal widths:
- **~40 cols (very narrow):** No labels shown (guard: width < 51)
- **~70 cols (narrow):** 1k, 10k displayed
- **~95 cols (half-width):** 100, 1k, 5k, 10k, 20k displayed with good spacing ✓
- **~130 cols (standard):** 100, 500, 1k, 5k, 10k, 20k displayed
- **~180 cols (full width):** All 10 labels displayed

### Edge Cases Verified

- ✓ Resize terminal → labels automatically update on next render
- ✓ Low sample rates (16 kHz) → 20k label absent (expected, beyond Nyquist)
- ✓ High sample rates (96 kHz) → 20k label present and positioned correctly
- ✓ No labels overlap at any width
- ✓ Spectrum bars still fill all available rows above label row
- ✓ Color gradient remains smooth and consistent
- ✓ No visual collision with transport bar or slider panels

---

## Commits

1. **ed69667** — `feat: add frequency labels on spectrum X-axis`
   - Core implementation: quadratic mapping, label positioning, overlap prevention

2. **42ae2f1** — `feat: add more frequency labels to spectrum scale`
   - Expand from 6 to 10 frequency intervals

3. **4f22c23** — `feat: adaptive frequency labels based on terminal width`
   - Width-based label selection; minimal at narrow widths, full at wide widths

4. **a8f04dc** — `refine: include 5k label in 81-120 column breakpoint`
   - Fine-tune half-width display to include 5k (clarity region)

---

## Architectural Notes

### Why Quadratic Scaling?

The quadratic map (t²) was chosen over true log scale because:
- **Simplicity:** Just a square root and multiply; no expensive log() calls
- **Perceptual accuracy:** Closely approximates human pitch perception (log scale) for UI layout
- **Consistency:** Existing spectrum implementation already used t² for bin mapping
- **Fast:** Computed once per render with minimal overhead

### Why Adaptive Labels?

Hard-coded label sets based on width are superior to dynamic filtering because:
- **Predictable:** User knows exactly when labels appear/disappear
- **No overcrowding:** Each width bracket is carefully tuned to avoid overlap
- **Clear intent:** Code is easy to read and understand
- **Performant:** Single match, no runtime overlap checks (overlap prevention is still a safety net)

### Future Extensibility

- Add 250 Hz (speech formant region) for 161+ col displays
- Add 3k Hz (voice presence) for extra-wide displays
- Customize label sets via configuration file

---

## Test Results

```
test result: ok. 52 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

All existing tests unaffected. No new test failures.

---

## Related Issues Resolved

- **UX**: Spectrum scale is now self-documenting
- **Accessibility**: Frequency reference visible at all terminal widths
- **Design**: Muted gray labels integrate seamlessly with punk gradient aesthetic
