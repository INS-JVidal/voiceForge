# Q05 — Smooth Gradient Slider Rendering Plan

**Phase:** P8 Enhancement (Post-Polish)
**Scope:** Upgrade WORLD parameter sliders to smooth gradient rendering with sub-pixel precision
**Status:** Ready for implementation

---

## Overview

Enhance the visual appearance of WORLD parameter sliders by replacing flat single-color bars with:
- **Per-character color gradients** (dark left → bright right)
- **Sub-pixel smooth leading edge** using Unicode block characters (▏▎▍▌▋▊▉█)
- **Context-aware color schemes** reflecting selection and focus state

The goal is to improve visual feedback, making slider position immediately apparent without reading numeric values while maintaining fast rendering performance.

---

## Design Goals

1. **Visual Enhancement**
   - Smooth gradient across slider length shows position at a glance
   - Sub-pixel partial blocks eliminate jumpy rendering as value changes
   - Context colors (cyan → blue → grey) clearly indicate interaction state
   - Professional appearance matches modern TUI design standards

2. **Implementation Quality**
   - Minimal code changes (single file: `src/ui/slider.rs`)
   - No API changes, no new dependencies
   - Fast: computed at render time, O(track_width) per slider
   - Backward compatible: all existing slider logic unchanged

3. **User Experience**
   - Gradient provides visual momentum and fills sense
   - Sub-pixel smoothness avoids visual stutter
   - Color feedback clarifies which slider will respond to keyboard input
   - Works seamlessly in 256-color and true-color terminals

---

## Architecture

### Rendering Strategy

**Current (Flat):**
```
Span::styled("█".repeat(filled), Style::default().fg(Color::Cyan))
```

**New (Gradient):**
```
for i in 0..filled {
    let t = i / track_width        // position [0.0, 1.0]
    let ch = block_char_for_pos(i) // character: ▏ → █
    let color = gradient_color(t, is_selected, focused)  // Rgb interpolation
    spans.push(Span::styled(ch, Style::default().fg(color)))
}
if has_partial {
    spans.push(Span::styled(partial_char, Style::default().fg(color)))
}
```

### Color Schemes

| State | From (Left) | To (Right) | Purpose |
|-------|-------------|-----------|---------|
| Focused + Selected | `Rgb(0, 60, 80)` | `Rgb(0, 220, 255)` | Bold cyan gradient shows active control |
| Focused + Unselected | `Rgb(0, 25, 60)` | `Rgb(50, 120, 190)` | Softer blue shows available slider |
| Unfocused | `Rgb(20, 35, 50)` | `Rgb(55, 85, 110)` | Dimmed grey-blue shows inactive |

### Block Characters

Eight Unicode block characters for smooth progression:

```
▏ (1/8) ▎ (2/8) ▍ (3/8) ▌ (4/8) ▋ (5/8) ▊ (6/8) ▉ (7/8) █ (8/8)
```

Distribution across filled portion:
- `total ≤ 1`: return `█`
- `total > 1`: map `i ∈ [0, total-1]` to index `⌊(i × 7) / (total - 1)⌋`

Partial block for fractional remainder:
- Compute remainder `frac = exact_position - floor(exact_position)`
- Index = `⌊frac × 8⌋` → partial block (empty for 0 or 8)

---

## Implementation Scope

### File Changes

**src/ui/slider.rs**
- Add `const BLOCK_CHARS: [&str; 8]`
- Add `fn block_char_for_pos(i: usize, total: usize) -> &'static str`
- Add `fn partial_block_char(frac: f64) -> &'static str`
- Add `fn gradient_color(t: f64, is_selected: bool, focused: bool) -> Color`
- Replace bar-rendering block (lines ~60–72) with gradient loop and span building

### No Changes Required
- `app.rs` — AppState, SliderDef remain unchanged
- `handler.rs` — Input handling unchanged
- `layout.rs` — Layout unchanged
- Any other file

---

## Visual Examples

### Pitch Shift at 25% (Focused, Selected)
```
▸ Pitch Shift
  [▏▎▍▌░░░░░░░░░░░░░░░░░] -1.5 st
```
4 full chars + 1 partial = 4.5 cells filled in 20-cell track
Colors: dark cyan → bright cyan

### Speed at 62% (Focused, Unselected)
```
  Speed
  [▏▎▎▍▍▌▌▋▋▊▊▉░░░░░░░░░░░░░░░] ×1.31
```
12 full chars + 1 partial = 12.5 cells filled
Colors: dark blue → mid blue

### Slider at 100% (Full Bright)
```
▸ Breathiness
  [▏▎▍▌▋▊▉█▏▎▍▌▋▊▉█▏▎▍▌█] ×3.0
```
All 20 cells filled, characters cycle through gradient
Colors: dark cyan left → bright cyan right

---

## Verification Checklist

### Code Quality
- [ ] `cargo check` passes
- [ ] `cargo clippy --all-targets -- -D warnings` — zero warnings
- [ ] `cargo test --all-targets` — all 62 tests pass
- [ ] `cargo build --release` succeeds

### Manual Testing
- [ ] Slider at 0% → no fill, only empty cells
- [ ] Slider at 100% → full bar with bright end color
- [ ] Slider at ~50% → gradient visible, dark left → light right
- [ ] Moving slider → gradient updates smoothly
- [ ] Tab focus → sliders change to blue when panel unfocused
- [ ] Up/Down select → active slider cyan, others blue
- [ ] Works in both 256-color and true-color terminals

### Commit Standards
- [ ] Conventional message: `feat: add smooth gradient slider rendering...`
- [ ] Includes rationale in commit body
- [ ] Single logical commit covering all changes

---

## Success Criteria

✓ Visual improvement: sliders render with smooth dark→bright gradient
✓ Sub-pixel smoothness: partial blocks eliminate jumpy rendering
✓ No regressions: all 62 tests pass, zero clippy warnings
✓ Single file change: only `src/ui/slider.rs` modified
✓ Performance: O(track_width) per slider, negligible overhead
✓ Compatibility: works in standard terminals, no new dependencies

---

## Notes

- Helper functions are private to slider.rs (no public API changes)
- Color interpolation uses linear RGB space (sufficient for visual smoothness)
- Partial blocks computed at render time (no caching needed)
- Block character distribution balances visual progression with position accuracy
