# Plan: Punk GPU Pixel Spectrum Visualizer (P5)

## Context

Upgrade the spectrum visualizer from Unicode block characters to true per-pixel RGB rendering
via a graphics protocol (iTerm2/Sixel/Kitty auto-selected). Displayed on WezTerm + WSL2.
Visual design: smooth pixel columns with a deep violet → electric purple → neon pink
amplitude-based gradient. Falls back to braille Unicode on unsupported terminals.

---

## Visual Design

**Per-pixel amplitude bars** with smooth vertical gradient:
- Bottom (0.0) → `#3D0066` deep violet
- Mid (0.5) → `#CC00FF` electric purple
- Top (1.0) → `#FF0099` neon pink
- Background → `#000000` black

Each bar column is 1 pixel wide (or a few px for visibility). Height = amplitude fraction × pixel height. Color per pixel is interpolated by its vertical position within the column.

**Protocol priority on WezTerm**: iTerm2 (auto-detected via `TERM_PROGRAM=WezTerm`).

---

## Frequency Resolution & Log Scale

**Keep current settings** (unchanged):
- FFT size: `FFT_SIZE = 2048` in `src/dsp/spectrum.rs`
- Frequency resolution: ~21.5 Hz/bin at 44100 Hz
- Log bin mapping: `bin_idx = (num_bins as f64).powf(t)` where `t = col / (num_cols - 1)`
- No changes to `src/dsp/spectrum.rs`.

---

## New Cargo.toml Dependencies

```toml
ratatui-image = { version = "10", default-features = false, features = ["crossterm", "image-defaults"] }
image = { version = "0.25", default-features = false, features = ["png"] }
```

No system dependencies. Pure Rust path via `icy_sixel` (transitive dep of ratatui-image).

---

## Implementation Plan

### Files to change:
- `Cargo.toml` — add two crates
- `src/app.rs` — add picker + protocol fields
- `src/main.rs` — init Picker; drive spectrum_to_image each frame
- `src/ui/spectrum.rs` — pixel renderer + StatefulImage widget
- `src/ui/layout.rs` — update render call signature

---

### Step 1 — `Cargo.toml`
Add `ratatui-image` and `image` as above.

---

### Step 2 — `src/app.rs`
Add two fields to `AppState` (after `status_message`):

```rust
use ratatui_image::picker::Picker;
use ratatui_image::protocol::StatefulProtocol;

pub struct AppState {
    // ... existing fields ...
    pub spectrum_picker: Option<Picker>,
    pub spectrum_state: Option<Box<dyn StatefulProtocol>>,
}
```
Initialize both to `None` in `AppState::new()`.

---

### Step 3 — `src/main.rs` — Picker init (before render loop)

After `enable_raw_mode()` + `stdout().execute(EnterAlternateScreen)`:

```rust
// One-time terminal query: detects font size + best graphics protocol
let picker = ratatui_image::picker::Picker::from_query_stdio().ok();
app.spectrum_picker = picker;
```

---

### Step 4 — `src/main.rs` — drive spectrum image (inside existing spectrum update block ~line 86)

After the existing `app.spectrum_bins = compute_spectrum(...)` call:

```rust
if let Some(ref picker) = app.spectrum_picker {
    let font_size = picker.font_size();  // (cell_width_px, cell_height_px)
    let px_w = spectrum_area_width * font_size.0 as u16;  // total pixel width
    let px_h = spectrum_area_height * font_size.1 as u16; // total pixel height
    let img = spectrum::spectrum_to_image(&app.spectrum_bins, px_w as u32, px_h as u32);
    app.spectrum_state = Some(picker.new_resize_protocol(img.into()));
}
```

`spectrum_area_width` and `spectrum_area_height` (in character cells) must be passed in or computed from the terminal size. Use `terminal.size()` (already available in main loop) and apply the same layout split as `layout::render`.

---

### Step 5 — `src/ui/spectrum.rs` — pixel renderer

#### New function: `spectrum_to_image`

```rust
use image::{RgbaImage, Rgba};

pub fn spectrum_to_image(bins: &[f32], width: u32, height: u32) -> RgbaImage {
    let mut img = RgbaImage::from_pixel(width, height, Rgba([0, 0, 0, 255]));

    let num_bars = width as usize;
    let bin_count = bins.len();
    if num_bars == 0 || bin_count == 0 || height == 0 { return img; }

    for col in 0..num_bars {
        // Log-frequency mapping (same as current render fn)
        let t = col as f64 / (num_bars - 1).max(1) as f64;
        let bin = (bin_count as f64).powf(t).min((bin_count - 1) as f64) as usize;

        // Amplitude fraction [0.0, 1.0] from dB value in [-80, 0]
        let db = bins[bin].clamp(-80.0, 0.0);
        let amp = (db + 80.0) / 80.0;  // 0.0 = silence, 1.0 = peak

        let filled_px = (amp as f64 * height as f64).round() as u32;

        for row in 0..filled_px {
            // frac: 0.0 at bottom, 1.0 at top of filled portion
            let frac = row as f64 / height as f64;
            let color = punk_color(frac);
            let y = height - 1 - row;  // render bottom-up
            img.put_pixel(col as u32, y, color);
        }
    }
    img
}

fn punk_color(frac: f64) -> Rgba<u8> {
    let (r, g, b) = if frac < 0.5 {
        let t = frac * 2.0;
        // deep violet #3D0066 → electric purple #CC00FF
        (lerp(0x3D, 0xCC, t), lerp(0x00, 0x00, t), lerp(0x66, 0xFF, t))
    } else {
        let t = (frac - 0.5) * 2.0;
        // electric purple #CC00FF → neon pink #FF0099
        (lerp(0xCC, 0xFF, t), lerp(0x00, 0x00, t), lerp(0xFF, 0x99, t))
    };
    Rgba([r, g, b, 255])
}

fn lerp(a: u8, b: u8, t: f64) -> u8 {
    (a as f64 + (b as f64 - a as f64) * t).round() as u8
}
```

#### Updated `render` function

```rust
pub fn render(frame: &mut Frame, area: Rect, app: &mut AppState) {
    if let Some(ref mut state) = app.spectrum_state {
        // GPU pixel path
        let widget = ratatui_image::StatefulImage::new(None);
        frame.render_stateful_widget(widget, area, state);
    } else {
        // Fallback: existing braille/Unicode render (kept as-is)
        render_unicode_fallback(frame, area, app);
    }
}
```

Keep existing render logic renamed to `render_unicode_fallback` for graceful degradation on terminals without graphics protocol support.

---

### Step 6 — `src/ui/layout.rs`

Update `spectrum::render(frame, spectrum_area, app)` call to pass `&mut app`:
```rust
spectrum::render(frame, spectrum_area, &mut app);
```

---

### Step 7 — Reset on file load

In `main.rs`, when `Action::LoadFile` fires, reset the spectrum state:
```rust
app.spectrum_state = None;
app.spectrum_bins.clear();
```

---

## Critical Files

| File | Change |
|------|--------|
| `Cargo.toml` | Add ratatui-image + image |
| `src/app.rs` | Add `spectrum_picker` + `spectrum_state` to AppState |
| `src/main.rs` | Init Picker before loop; call spectrum_to_image each frame |
| `src/ui/spectrum.rs` | Add `spectrum_to_image()`, `punk_color()`, update `render()` |
| `src/ui/layout.rs` | Pass `&mut app` to spectrum::render |

---

## Verification

```bash
cargo build                                  # Compiles clean
cargo clippy --all-targets -- -D warnings    # Zero warnings
cargo test --all-targets                     # All 42 tests pass

# Manual in WezTerm on WSL2:
cargo run
# Load an audio file
# Expected:
# - Spectrum shows smooth pixel-rendered columns
# - Color gradient: dark violet at base → bright pink at peaks
# - High-energy frequencies pulse neon pink
# - Terminal resize → spectrum adapts (StatefulImage handles this)
# - Pause → spectrum freezes on last frame
# - A/B toggle → spectrum updates

# Fallback test (plain terminal without graphics protocol):
TERM=dumb cargo run
# Expected: existing Unicode block bar chart renders instead
```
