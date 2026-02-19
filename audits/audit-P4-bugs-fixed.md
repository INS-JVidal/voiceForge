# Audit: Full Codebase Review After P4 + P4b

**Date:** February 19, 2026
**Scope:** All 19 source files, 3 test files, Cargo.toml, world-sys FFI crate
**Trigger:** Post-implementation review of P4 (A/B comparison toggle) and P4b (enhanced seek navigation)
**Result:** 1 bug found and fixed, 0 inconsistencies remaining

---

## Bug Found

### B1. `status_message` invisible when a file is loaded

**Severity:** Medium
**Files:** `src/ui/status_bar.rs`, `src/main.rs`

**Problem:** The status bar rendering logic used an `if/else if/else` chain:

```rust
if file_info.is_some() → show file info (+ optional processing_status)
else if status_message.is_some() → show error message
else → "No file loaded"
```

When a file was loaded (`file_info` is `Some`), `status_message` was **never displayed**. This caused two error conditions to be silently swallowed:

1. **Playback rebuild failure** — If `rebuild_stream` failed during `SynthesisDone` handling (`main.rs:151-153`), the error was stored in `status_message` but hidden behind the file info display.
2. **File picker validation failure** — If the user tried to open a second file via the file picker and the path failed validation (`handler.rs:32-33`), the "File not found" message was invisible because the first file's `file_info` took priority.

**Fix applied:**

1. **`src/ui/status_bar.rs`** — Added `status_message` display (in red) after file info spans when both `file_info` and `status_message` are present. Error messages are now always visible.

2. **`src/main.rs`** — Added `app.status_message = None` at the start of the `SynthesisDone` handler, so stale playback errors are cleared when synthesis succeeds. Without this, a `rebuild_stream` error from a previous attempt would linger indefinitely after recovery via the `swap_audio` path.

**Commit:** Same commit as this audit entry.

---

## Areas Verified Clean

### Synchronization

- All `position` atomic loads/stores use `Acquire/Release` ordering, consistent across `main.rs`, `handler.rs`, and `playback.rs`.
- `Relaxed` is only used for UI display reads in `transport.rs` (`playing.load(Ordering::Relaxed)`) — acceptable, worst case is one 33ms frame of stale state.
- `swap_audio` acquires write lock; audio callback uses `try_read()` with silence fallback on contention. No deadlock possible.

### Lock Safety

- `swap_audio` and `ToggleAB` use `.expect("audio lock poisoned")`. The write body (`*guard = new_audio`) is an infallible Arc assignment — the lock cannot be poisoned.
- Audio callback degrades gracefully to silence on any `try_read()` failure (poisoned or contended).

### Edge Cases

- `Home`/`End` keys with no file loaded: no-op (position stays at 0 / `file_info` guard prevents End).
- `'a'` toggle with incomplete analysis: no-op (guard requires both `audio_data` and `original_audio`).
- `Left`/`Right` seek when Transport focused but no file: no-op (`file_info` guard).
- A/B toggle position scaling: round-trips perfectly via proportional scaling. `new_pos.min(new_len)` prevents overflow.
- `SynthesisDone` while on A: stores processed audio silently, doesn't touch stream.

### Processing Thread

- Stale command drain correctly handles interleaved `Analyze`/`Resynthesize`/`Shutdown` commands.
- Neutral slider shortcut returns mono copy without WORLD artifacts.
- Synthesis errors sent as `Status(msg)`, no panics.
- `Drop` impl sends `Shutdown` and joins thread for panic-safe cleanup.

### Modifier Math

- Pitch shift: `2^(st/12)` ratio, only voiced frames. Correct.
- Pitch range: expand/compress around voiced mean, clamp to >= 0. Correct.
- Speed: linear interpolation resampling, temporal positions regenerated. Correct.
- Breathiness: additive increase towards 1.0, clamped to [0, 1]. Correct.
- Formant shift: frequency axis warp with linear interpolation, last-bin fallback. Correct.
- Spectral tilt: dB/octave slope relative to bin 1, power gain = voltage gain squared. Mathematically correct.

### Memory Management

- Old `Arc<AudioData>` references cleaned up on file load (stream dropped, playback state replaced).
- A/B swap: old buffer stays alive in `app.audio_data` / `app.original_audio`. No data loss.
- `audio_lock` Arc ref count correct: shared between `PlaybackState` and `CallbackContext`, both dropped together.

### UI

- Transport bar budget handles longer `[A: Original]`/`[B: Processed]` text via `saturating_sub`. Seek bar shrinks gracefully on narrow terminals.
- Slider panel handles narrow terminal with value-only fallback.
- File picker centered popup with `Clear` background.

### Tests

- 18/18 pass: 4 decoder + 11 WORLD FFI + 3 modifier.
- Test coverage spans decoder edge cases, FFI validation, and modifier transforms.

---

## Summary

| Finding | Severity | Files Changed | Status |
|---|---|---|---|
| `status_message` invisible when file loaded | Medium | `src/ui/status_bar.rs`, `src/main.rs` | Fixed |

Total: 1 bug fixed across 2 files. All 18 tests pass, zero clippy warnings.
