# P8 — Polish & Release Readiness

## Goal
Final quality pass: mouse support, improved slider UX, error handling, help screen, and overall polish to make the app feel complete and robust.

## Prerequisite
P7 complete (all core features working: decode, analyze, modify, effects, A/B, spectrum, export).

## Steps

### 8.1 Mouse support
- Click on a slider → select it and set value based on click position
- Click and drag slider thumb → adjust value continuously
- Click on seek bar → seek to that position
- Click transport buttons (Play/Pause/Loop) → toggle state
- Use `crossterm::event::MouseEvent` (enabled via `ratatui::crossterm`)

### 8.2 Slider UX improvements
- Debounce resynthesis: only trigger WORLD resynthesis 300ms after the last slider change (not on every keystroke during rapid adjustment)
- Visual feedback during resynthesis: dim the slider or show a spinner next to it
- Slider value snapping: double-tap a slider to reset to default
- Show both the numeric value and a visual indicator of position

### 8.3 Error handling
- Audio device not found → clear error message, app still launches (shows TUI but playback disabled)
- Unsupported file format → error in status bar, previous file stays loaded
- WORLD analysis failure → error message, original playback continues
- File I/O errors (export, file picker) → status bar messages, no crashes
- Graceful panic handler: restore terminal on panic so the user doesn't get a broken terminal

### 8.4 Help screen
- Press `?` or `H` → overlay panel showing all keybindings
- Layout:
  ```
  ┌─ Help ──────────────────────────┐
  │  Space    Play / Pause          │
  │  Tab      Switch A/B            │
  │  ↑/↓     Select slider          │
  │  ←/→     Adjust slider          │
  │  Shift+← Fine adjust            │
  │  [/]      Seek ±5s              │
  │  R        Toggle loop            │
  │  S        Save processed WAV    │
  │  O        Open file             │
  │  A        Advanced settings     │
  │  ?        This help             │
  │  Q/Esc   Quit                   │
  │                                  │
  │  Press any key to close         │
  └──────────────────────────────────┘
  ```
- Dismiss with any key press

### 8.5 Advanced settings panel
- Press `A` → toggle an overlay or expandable panel showing WORLD analysis settings:
  - Pitch algorithm: DIO / Harvest (radio selection)
  - f0 floor: 50-200 Hz
  - f0 ceiling: 400-1200 Hz
  - Frame period: 1.0-10.0 ms
- Changing these triggers a full re-analysis (slow operation, show progress)

### 8.6 Visual polish
- Consistent color scheme across all widgets
- Border styles: rounded borders for panels
- Active/inactive panel visual distinction (brighter border when focused)
- Processing progress bar for WORLD analysis (not just text)
- Smooth seek bar animation

### 8.7 Startup behavior
- No file argument → show welcome message in the spectrum area: "Press O to open a file or pass a path as argument"
- Invalid file argument → show error, stay in app (don't exit)

### 8.8 Terminal restore safety
Install a panic hook that restores the terminal:
```rust
let original_hook = std::panic::take_hook();
std::panic::set_hook(Box::new(move |panic| {
    // restore terminal
    let _ = ratatui::crossterm::terminal::disable_raw_mode();
    let _ = ratatui::crossterm::execute!(std::io::stdout(), LeaveAlternateScreen);
    original_hook(panic);
}));
```

## Human Test Checklist

- [ ] Click on a slider with mouse → selects it and sets value at click position
- [ ] Drag a slider → value changes smoothly
- [ ] Click on seek bar → playback jumps to that position
- [ ] Rapidly press ←/→ on a WORLD slider → resynthesis triggers once (debounced), not per keypress
- [ ] Press `?` → help overlay appears with all keybindings listed
- [ ] Press any key → help overlay dismisses
- [ ] Press `A` → advanced settings panel appears with analysis options
- [ ] Change pitch algorithm to Harvest → re-analysis runs (may be slower than DIO)
- [ ] Run with no arguments → welcome message shown, `O` opens file picker
- [ ] Run with invalid file → error shown, app stays open
- [ ] Force a panic (e.g., in debug mode) → terminal is properly restored
- [ ] Try to play when no audio device available → error message, no crash
- [ ] Try to open a non-audio file (e.g., .txt) → error message, previous state preserved
- [ ] Overall: app feels responsive, looks consistent, no visual glitches on resize

## Dependencies Introduced
None new.

## Notes
- This phase is broader and less strictly defined than previous phases. Prioritize items that affect usability most: error handling (8.3), terminal safety (8.8), and mouse support (8.1).
- The advanced settings panel (8.5) triggers expensive re-analysis — make sure the UX clearly communicates this (confirmation dialog or progress bar).
