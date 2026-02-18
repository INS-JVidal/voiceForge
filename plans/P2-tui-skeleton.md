# P2 — TUI Skeleton with ratatui

## Goal
Replace the CLI player from P1 with a full ratatui TUI. Render the target layout (slider panels, spectrum placeholder, transport bar, status bar) with working keyboard navigation. Audio playback from P1 continues to work inside the TUI.

## Prerequisite
P1 complete (audio decode + playback works).

## Steps

### 2.1 Add dependencies
```toml
ratatui = "0.30"
# crossterm is re-exported via ratatui::crossterm — no separate dep
```

### 2.2 Application state — `src/app.rs`
```rust
pub enum AppMode { Normal, FilePicker, Help }
pub enum PanelFocus { WorldSliders, EffectsSliders, Transport }

pub struct AppState {
    pub mode: AppMode,
    pub focus: PanelFocus,
    pub selected_slider: usize,
    pub sliders: Vec<SliderDef>,       // Name, min, max, value, step, unit
    pub file_info: Option<FileInfo>,   // Name, sample_rate, channels, duration
    pub playback: PlaybackState,       // playing, position, loop_enabled
    pub should_quit: bool,
}
```
Define all 12 sliders (6 WORLD + 6 Effects) with their ranges/defaults from the plan.

### 2.3 Layout — `src/ui/layout.rs`
Split the terminal into the planned regions:
```
┌──────────────────────────────────────────┐
│  [WORLD Sliders]  │  [Effects Sliders]   │  ← top half, 2 columns
├──────────────────────────────────────────┤
│  [Spectrum placeholder]                  │  ← middle band
├──────────────────────────────────────────┤
│  [Transport: Play/Pause/Seek/A-B]       │  ← transport bar
├──────────────────────────────────────────┤
│  [Status: file info, sample rate, etc]  │  ← bottom status line
└──────────────────────────────────────────┘
```
Use `ratatui::layout::Layout` with vertical splits, then horizontal split for slider columns.

### 2.4 Custom slider widget — `src/ui/slider.rs`
Render a horizontal slider:
```
 Pitch Shift   ──────●──────  +2.3 st
```
- Label left-aligned
- Track using Unicode block chars or `─` with `●` for thumb position
- Current value + unit right-aligned
- Highlight the selected slider (different color/bold)

### 2.5 Transport widget — `src/ui/transport.rs`
Render:
```
 [▶ Play]  [↺ Loop: Off]  ──────●──── 1:23 / 3:45   [A/B: Original]
```
- Show play/pause state
- Seek bar with position
- A/B toggle indicator (placeholder — A/B switching is P4)

### 2.6 Status bar — `src/ui/status_bar.rs`
```
 File: song.wav  │  44100 Hz  │  Stereo  │  3:45
```

### 2.7 Spectrum placeholder — `src/ui/spectrum.rs`
Render an empty bordered block titled "Spectrum" with a message like "Spectrum visualization (coming in P5)". This reserves the layout space.

### 2.8 Input handler — `src/input/handler.rs`
Map keys to actions:
- `Q` / `Esc` → quit
- `Space` → play/pause
- `Tab` → cycle panel focus (WorldSliders → EffectsSliders → Transport)
- `↑/↓` → navigate slider selection within focused panel
- `←/→` → adjust selected slider value by step
- `Shift+←/→` → fine adjust (0.1× step)
- `R` → toggle loop
- `[/]` → seek ±5s
- `O` → switch to FilePicker mode (stub for now)

### 2.9 Main event loop — `src/main.rs`
1. Initialize terminal (alternate screen, raw mode)
2. If file path given as CLI arg, decode and start playback
3. Loop:
   - Poll for crossterm events (with timeout for ~30fps render)
   - Dispatch to input handler
   - Update AppState
   - Render UI
4. On quit: restore terminal, drop audio stream

### 2.10 File picker stub — `src/ui/file_picker.rs`
Simple text input: press `O`, type a file path, press `Enter` to load. Minimal implementation — just enough to load files without restarting the app.

## Human Test Checklist

- [ ] `cargo run` shows the TUI with all 4 layout regions visible
- [ ] `cargo run -- path/to/song.wav` shows file info in status bar and plays audio
- [ ] 12 sliders are displayed (6 WORLD left, 6 Effects right) with correct labels and default values
- [ ] `↑/↓` moves slider selection highlight; `←/→` adjusts values within defined ranges
- [ ] `Shift+←/→` adjusts in smaller increments
- [ ] `Tab` switches focus between left panel and right panel
- [ ] `Space` toggles play/pause; transport bar updates (▶ / ⏸)
- [ ] `[/]` seeks; seek bar position updates
- [ ] `R` toggles loop indicator
- [ ] `Q` exits cleanly (terminal restored, no garbled output)
- [ ] Resizing the terminal re-renders layout proportionally
- [ ] `O` opens path input; typing a path and pressing Enter loads and plays the new file

## Dependencies Introduced
- `ratatui` 0.30

## Notes
- Slider value changes don't affect audio yet — that's P3.
- A/B toggle shows in transport but doesn't function yet — that's P4.
- Spectrum area is a placeholder — that's P5.
