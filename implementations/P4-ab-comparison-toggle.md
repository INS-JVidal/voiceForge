# P4 — A/B Comparison Toggle: Implementation Report

## Goal

Enable instant switching between original (unprocessed) and processed audio during playback. Both sources share the same time position so the switch is seamless. This is the core workflow for voice tuning — the user adjusts WORLD sliders, then toggles A/B to compare against the original.

## Prerequisite

P3 complete (18 tests). On file load, WORLD analysis runs and the processing thread produces a mono `AudioData` via `SynthesisDone`. The main thread stores the current audio in `app.audio_data` and rebuilds the cpal stream on each resynthesis.

## What Was Built

### Modified Files (6)

**`src/app.rs`** — Two additions:
- `original_audio: Option<Arc<AudioData>>` field — stores the mono original audio received from `AnalysisDone`. Set to `None` on init and reset on new file load.
- `Action::ToggleAB` variant — returned by the input handler when `'a'` is pressed.

**`src/dsp/processing.rs`** — Changed `ProcessingResult::AnalysisDone` from a unit variant to `AnalysisDone(AudioData)`, carrying the mono original. Both send sites updated (main `Analyze` handler and inline analyze in the stale-command drain loop). The processing thread clones the mono once — one copy is kept locally for the neutral-slider shortcut, the other is sent to the main thread.

**`src/audio/playback.rs`** — Three additions:
- `audio_lock: Option<Arc<RwLock<Arc<AudioData>>>>` field on `PlaybackState` — holds a handle to the `RwLock` shared with the running stream's audio callback. Allows the main thread to swap the audio buffer without tearing down and rebuilding the cpal stream.
- Both `start_playback` and `rebuild_stream` now create the `Arc<RwLock<...>>` externally, share it with `CallbackContext` via `Arc::clone`, and store it in `PlaybackState.audio_lock`. Previously the `RwLock` was created inline in `CallbackContext` and inaccessible from the main thread.
- `swap_audio(audio_lock, new_audio)` function — acquires a write lock and replaces the inner `Arc<AudioData>`. The audio callback uses `try_read()` and outputs silence if the lock is briefly held, so the swap is glitch-free (~microsecond contention).
- `rebuild_stream` signature changed from `&PlaybackState` to `&mut PlaybackState` to allow setting `audio_lock`.

**`src/input/handler.rs`** — Added `'a'` key binding in `handle_normal`:
- Guards on `audio_data.is_some() && original_audio.is_some()` — only allows toggle when both buffers exist (file loaded and analysis complete).
- Flips `app.ab_original` and returns `Some(Action::ToggleAB)`.

**`src/main.rs`** — Three areas of change:

1. **`AnalysisDone(mono_original)` handler** — stores the mono original in `app.original_audio` before auto-sending `Resynthesize`.

2. **`SynthesisDone` handler** — now branches on `app.ab_original`:
   - **On B (processed)**: same channel adjustment and position clamping as before, but now prefers `swap_audio` over `rebuild_stream` when `audio_lock` is available. Falls back to `rebuild_stream` only if `audio_lock` is `None` (defensive, shouldn't happen after `start_playback`).
   - **On A (original)**: just stores `app.audio_data = Some(new_audio)` without touching the stream or `file_info`. The user is hearing the original; the processed audio is silently updated for when they toggle back to B.
   - Atomic ordering upgraded from `Relaxed` to `Acquire/Release` for consistency with the rest of the codebase.

3. **`Action::ToggleAB` handler** — performs the buffer swap:
   - Reads the current buffer length from the `RwLock` (via `read()`) to determine `old_len`.
   - If `old_len != new_len` (speed slider changed the processed buffer duration), scales position proportionally: `new_pos = (pos / old_len) * new_len`. This preserves the approximate time position across buffers of different durations.
   - Updates `file_info` (total_samples, duration_secs, channels) to reflect the now-active buffer.
   - Calls `swap_audio` to atomically replace the buffer in the stream.

4. **`Action::LoadFile` handler** — resets A/B state on new file load: `ab_original = false`, `original_audio = None`.

**`src/ui/transport.rs`** — Enhanced A/B display:
- Changed from `[A]`/`[B]` to `[A: Original]`/`[B: Processed]` for clarity.
- Color-coded: green for original (A), magenta for processed (B), both bold.
- Bar budget calculation naturally handles the longer text via `saturating_sub`.

### No New Files

Everything fits into existing modules.

## Key Design Decisions

### 1. Swap via RwLock, Not Stream Rebuild

The existing `CallbackContext.audio` is `Arc<RwLock<Arc<AudioData>>>`. Swapping the inner `Arc` via `write()` is O(1) and glitch-free — the audio callback uses `try_read()` and outputs silence if the lock is briefly held. This avoids the ~1ms gap from tearing down and rebuilding the cpal stream. Stream rebuild is retained only as a fallback in `SynthesisDone` if `audio_lock` is somehow `None`.

### 2. `'a'` Key Instead of Tab

Tab cycles panel focus (World -> Effects -> Transport) since P2. All three panels need keyboard access for slider navigation. `'a'` is unused, mnemonic for "A/B", and single-key.

### 3. Proportional Position Scaling on A/B Switch

When the speed slider is non-default, the processed buffer has a different length than the original. Rather than clamping (which would jump to a different time position), position is scaled proportionally: `new_pos = (old_pos / old_len) * new_len`. This preserves the approximate time position across buffers. The scaling round-trips perfectly: toggling A->B->A returns to the exact original position.

### 4. Both Buffers Are Always Mono

P3 established that WORLD always outputs mono. The neutral-slider shortcut also returns mono (via `to_mono()`). The `original_audio` stored in P4 is the mono version sent via `AnalysisDone` — never the raw stereo file. This means both A and B buffers are always mono, eliminating channel-mismatch issues on toggle.

### 5. SynthesisDone Respects A/B State

When the user is on A (original) and a `SynthesisDone` arrives (from a slider change), the new processed audio is stored in `audio_data` but the stream is not touched. The user continues hearing the original uninterrupted. When they toggle back to B, the latest processed audio is used.

### 6. `audio_lock` Exposed to Main Thread

Both `start_playback` and `rebuild_stream` now create the `RwLock` externally and store a clone in `PlaybackState`. This gives the main thread a handle to swap audio at any time. Previously the `RwLock` was created inline inside `CallbackContext` and unreachable from outside.

## Architecture

```
load_file()
  -> start_playback() returns PlaybackState with audio_lock
  -> Analyze(audio) sent to processing thread

processing thread:
  Analyze -> AnalysisDone(mono_original) + stores WorldParams
  main stores original_audio = mono_original
  auto-sends Resynthesize(values)
  -> SynthesisDone(processed_audio)
  main stores audio_data = processed_audio
  swap_audio(lock, processed)  [if on B]

'a' key pressed:
  ab_original flips
  swap_audio(lock, original_audio or audio_data)
  position scaled proportionally if lengths differ
  file_info updated for new active buffer
  transport shows [A: Original] or [B: Processed]
```

## Edge Cases Handled

| Scenario | Behavior |
|---|---|
| No file loaded, press `'a'` | No-op (guard: `audio_data.is_some() && original_audio.is_some()`) |
| Analysis not complete, press `'a'` | No-op (`original_audio` is `None` until `AnalysisDone`) |
| Toggle to A before first `SynthesisDone` | Works — swaps to mono original; `audio_data` still holds raw file from load. When `SynthesisDone` arrives, processed audio stored silently. |
| Resynthesis in progress, on B | Keeps playing old processed buffer. `SynthesisDone` swaps new audio into stream. |
| Resynthesis in progress, on A | `SynthesisDone` stores new `audio_data` but doesn't swap stream. Next toggle to B uses latest processed audio. |
| Speed changed, switch A<->B | Position scaled proportionally. Duration and total_samples updated in `file_info`. |
| Load new file while on A | `ab_original` reset to `false`, `original_audio` cleared. Fresh analysis starts. |
| Rapid toggle (A->B->A) | Position round-trips perfectly via proportional scaling. |
| `audio_lock` is `None` on `SynthesisDone` | Falls back to `rebuild_stream` (defensive — can't happen after successful `start_playback`). |

## Robustness Considerations

- **Lock poisoning**: `swap_audio` and `ToggleAB` use `.expect("audio lock poisoned")`. The only write operation (`*guard = new_audio`) is infallible (Arc assignment), so the lock cannot be poisoned. The audio callback uses `try_read()` and degrades to silence on any lock error.
- **Position clamping**: `new_pos.min(new_len)` prevents out-of-bounds on proportional scaling. End-of-buffer position maps correctly (callback outputs silence for `pos >= total_samples`).
- **No data loss on swap**: Old audio `Arc` remains alive in `app.audio_data` or `app.original_audio` after swap. Only the stream's reference changes.
- **Clean state on new file**: Both `ab_original` and `original_audio` are reset before analysis starts. The old `audio_lock` is dropped when `app.playback` is replaced by `start_playback`'s new `PlaybackState`.
- **Atomic ordering**: All position loads/stores in P4 code use `Acquire/Release` (upgraded from `Relaxed` that was in the P3 code), consistent with the rest of the codebase.

## No New Dependencies

Everything uses existing crates.

## Verification

- `cargo clippy --workspace` — zero warnings
- `cargo test` — 18/18 pass (4 decoder + 11 WORLD FFI + 3 modifier)
- Manual workflow: load file -> wait for analysis -> adjust Pitch Shift -> press `'a'` -> hear original -> press `'a'` -> hear processed -> no click/pop or time jump on toggle

## Resolved Placeholders

- `ab_original: bool` — now functional, toggled by `'a'` key, drives buffer swap and transport display
- `[A]`/`[B]` transport display — now shows `[A: Original]`/`[B: Processed]` with distinct colors

## Remaining Placeholders for Future Phases

- `loop_enabled: bool` — wired to UI display but not yet to playback loop logic (P5)
- Effects sliders — displayed and adjustable but not wired to effects processing (P6)
- Spectrum panel — placeholder text, real FFT visualization (P5)
