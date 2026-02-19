# P4b — Enhanced Seek Navigation

## Context

After P4 (A/B comparison), the app's seek controls are limited to `[`/`]` for ±5 second jumps. There's no way to jump to the start/end of the file, and the bracket keys aren't discoverable. Arrow keys (Left/Right) do nothing when Transport panel is focused — they adjust sliders only when World/Effects panels are focused. This is a gap before moving to P5 (spectrum visualization).

## Goal

Add two seek navigation features:
1. **Home/End keys** — jump to start/end of file (global, works regardless of panel focus)
2. **Left/Right arrows when Transport focused** — seek ±5s (same as `[`/`]` but more discoverable)

## File to Modify (1)

### `src/input/handler.rs`

All changes are in `handle_normal()`. No new actions, no new state, no changes to other files.

**1. Add Home/End key bindings** (global — no focus check):

```rust
KeyCode::Home => {
    app.playback.position.store(0, std::sync::atomic::Ordering::Release);
    None
}
KeyCode::End => {
    if let Some(ref info) = app.file_info {
        app.playback.position.store(info.total_samples, std::sync::atomic::Ordering::Release);
    }
    None
}
```

**2. Modify Left/Right arrow handlers** — add Transport-focused seek before the existing slider logic:

```rust
KeyCode::Left => {
    if app.focus == PanelFocus::Transport {
        // Seek backward when Transport panel is focused
        if let Some(ref info) = app.file_info {
            app.playback.seek_by_secs(-5.0, info.sample_rate, info.channels, info.total_samples);
        }
        None
    } else {
        // Existing slider adjustment logic (unchanged)
        ...
    }
}
```

Same pattern for `KeyCode::Right` with `+5.0`.

## Key Design Decisions

- **No new `Action` variants** — seek is immediate (atomic store / `seek_by_secs`), no main-loop handling needed.
- **Home sets position to 0** — simplest possible rewind. Uses `Release` ordering consistent with other position stores.
- **End sets position to `total_samples`** — the audio callback treats `pos >= total_samples` as end-of-file (outputs silence). Consistent with existing position clamping.
- **Transport Left/Right reuses `seek_by_secs`** — same ±5s as `[`/`]`. Keeps behavior consistent.
- **No conflict** — when Transport is focused, `focused_sliders_mut()` returns `None` and no slider adjustment happens. The current Left/Right code already does nothing useful in Transport focus. We just intercept earlier.

## Implementation Order

1. Add `Home`/`End` key bindings in `handle_normal` (after existing `']'` binding)
2. Wrap existing `Left`/`Right` handlers with `if app.focus == PanelFocus::Transport` check
3. `cargo clippy` + `cargo test`

## Verification

1. `cargo clippy --workspace` — zero warnings
2. `cargo test` — 18 tests pass
3. Manual: load file → press `Home` → position jumps to 0:00 → press `End` → position jumps to end
4. Tab to Transport → press Left → seeks back 5s → press Right → seeks forward 5s
5. Tab to World Sliders → Left/Right still adjust sliders as before
