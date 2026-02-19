# Plan: Spectrum Frequency Labels on X-axis

## Context

The spectrum graph currently shows no indication of what frequencies are displayed on the horizontal axis. The user wants frequency labels added. The mapping is quadratic (`bin = (bin_count-1) * t²`), not true log scale — this is important for computing where labels fall.

---

## Key Facts (from code exploration)

- **File to change**: `src/ui/spectrum.rs` — single file change
- **FFT_SIZE**: 2048, usable bins = 1024 (FFT_SIZE/2)
- **Mapping**: quadratic — `bin = round((bin_count-1) * t²)` where `t = col / (num_bars-1)`
- **Sample rate**: available from `app.file_info.as_ref().map(|f| f.sample_rate).unwrap_or(44100)`
- **Inner area height**: currently all rows used for bars (`inner_h` rows)

---

## Implementation Plan

### Step 1 — Compute bar heights with reserved label row

When `inner_h >= 2`, reserve the bottom row for frequency labels. Recalculate bar heights using `inner_h - 1` as the maximum height:

```rust
let bar_height = inner_h.saturating_sub(1) as f32;
let h = ((db + 80.0) / 80.0 * bar_height).clamp(0.0, bar_height);
```

This ensures bars don't overflow or clip when rendered to rows 1..=(inner_h - 1).

**Note on gradient consistency**: Keep the punk gradient calculation using the full `inner_h` for `row_ratio = level / inner_h`. This preserves color consistency across the visual height and prevents the gradient from shifting when label space is reserved.

### Step 2 — Compute column positions for key frequencies

For each target frequency `f`:
```
bin = f * FFT_SIZE / sample_rate
t = sqrt(bin / (bin_count - 1))
col = round(t * (num_bars - 1))
```

Label frequencies: **100 Hz, 500 Hz, 1 kHz, 5 kHz, 10 kHz, 20 kHz**
Short labels: `"100"`, `"500"`, `"1k"`, `"5k"`, `"10k"`, `"20k"`

### Step 3 — Build the label row with overlap prevention

- Start with a row of spaces (length = `num_bars`)
- Iterate through label frequencies in order
- For each (col, label) pair:
  - Check `col + label.len() <= num_bars` (won't overflow)
  - Track the last written label position (`last_col + last_label.len()`)
  - **Skip if** `col < last_col + last_label.len()` (would overlap with previous label)
  - Write label at `col` if both checks pass, update `last_col`
- Color: `Color::Rgb(120, 120, 120)` — muted gray so labels don't compete with bars

### Step 4 — Document the design in comments

Add a clear comment explaining:
1. The spectrum scale is **quadratic (t²)**, which approximates log scale perceptually but is not true log
2. The bottom row is reserved for frequency labels **only when `inner_h >= 2`**
3. Why: to keep the UI clean and organized without sacrificing bar visibility

---

## Critical Files

| File | Change |
|------|--------|
| `src/ui/spectrum.rs` | Reserve bottom row, compute label positions, render labels |

---

## Verification

```bash
cargo build                              # Clean build
cargo clippy --all-targets -- -D warnings  # Zero warnings
cargo test --all-targets                  # All tests pass
```

**Manual testing** (`cargo run <audio_file>`):

1. **Play audio and check spectrum display:**
   - ✓ Frequency labels visible at bottom row of spectrum
   - ✓ Labels show 100, 500, 1k, 5k, 10k, 20k at correct positions
   - ✓ No labels overlap with each other
   - ✓ Spectrum bars fill all available rows above labels (no clipping)
   - ✓ Punk gradient colors (deep violet → electric purple → neon pink) consistent top-to-bottom
   - ✓ No visual collision with transport bar (below) or slider panels (above)

2. **Edge cases:**
   - ✓ Resize terminal to minimum (40×12) — spectrum still renders, labels may be sparse or absent
   - ✓ Load very short audio (~1 sec) — FFT still computes, labels appear
   - ✓ High sample rate (96 kHz) — 20 kHz label visible; low rate (16 kHz) — 20 kHz label absent (expected)
