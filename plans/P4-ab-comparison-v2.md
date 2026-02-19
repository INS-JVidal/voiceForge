# P4 — A/B Comparison Toggle (v2)

> Supersedes `P4-ab-comparison.md`. Revised to align with the actual P0–P3 codebase after the audit integration.

## Goal

Enable instant switching between original (unprocessed) and processed audio during playback. Both sources share the same time position so the switch is seamless. This is the core workflow for voice tuning — the user adjusts WORLD sliders, then toggles A/B to compare against the original.

## Prerequisite

P3 complete (18 tests). On file load, WORLD analysis runs and the processing thread produces a mono `AudioData` via `SynthesisDone`. The main thread stores the current audio in `app.audio_data` and rebuilds the cpal stream on each resynthesis.

## Current State (what P0–P3 provides)

- **`app.ab_original: bool`** — placeholder field added in P2, rendered as `[A]`/`[B]` in transport bar. Not toggled by any key.
- **`app.audio_data: Option<Arc<AudioData>>`** — holds the *current* audio (replaced on each `SynthesisDone`). No separate "original" buffer is stored.
- **`Tab` key** — already bound to cycle panel focus (World → Effects → Transport). Cannot be reused for A/B.
- **`CallbackContext.audio: Arc<RwLock<Arc<AudioData>>>`** — the audio callback reads through an `RwLock`. The main thread can swap the inner `Arc<AudioData>` without rebuilding the stream.
- **Processing thread** — always outputs mono. On neutral sliders, returns a mono downmix of the original (not the raw stereo). So both "original" and "processed" are mono after first analysis.
- **`file_info.channels`** — updated to match `new_audio.channels` on each `SynthesisDone`. After first resynthesis, always 1 (mono).
- **`rebuild_stream`** — creates a new cpal stream. This is the current approach for buffer swap but is heavier than necessary if only the audio data changes.

## Design

### Core idea: store both buffers, swap via `RwLock`

Instead of rebuilding the cpal stream on A/B toggle, swap the `Arc<AudioData>` inside the existing stream's `RwLock`. The audio callback already acquires a read lock each invocation — it will seamlessly pick up the new buffer on the next callback cycle (sub-ms latency, glitch-free).

### Key binding: `'a'` for A/B toggle

`Tab` is taken. Use `'a'` — mnemonic for "A/B", single key, not used by any current binding. The transport display already shows `[A]`/`[B]`.

## Files to Modify (6)

### 1. `src/app.rs`

Add a field to store the original (mono) audio alongside the processed audio:

```rust
pub struct AppState {
    // ... existing fields ...
    pub original_audio: Option<Arc<AudioData>>,  // NEW: mono original, set on first SynthesisDone
    // ab_original: bool — already exists, will be used
}
```

Also add `Action::ToggleAB` variant:

```rust
pub enum Action {
    Quit,
    LoadFile(String),
    Resynthesize,
    ToggleAB,  // NEW
}
```

Initialize `original_audio: None` in `AppState::new()`.

### 2. `src/dsp/processing.rs`

Add a new result variant so the main thread receives the mono original separately:

```rust
pub enum ProcessingResult {
    AnalysisDone(AudioData),  // CHANGED: now carries the mono original
    SynthesisDone(AudioData),
    Status(String),
}
```

In the processing loop, after `Analyze` completes, send the mono original along with `AnalysisDone`:

```rust
ProcessingCommand::Analyze(audio) => {
    let _ = result_tx.send(ProcessingResult::Status("Analyzing...".into()));
    sample_rate = audio.sample_rate;
    let params = world::analyze(&audio);
    cached_params = Some(params);
    let mono = world::to_mono(&audio);
    original_mono = Some(mono.clone());
    let _ = result_tx.send(ProcessingResult::AnalysisDone(mono));
}
```

### 3. `src/audio/playback.rs`

Add a function to swap audio data in an existing stream without rebuilding it:

```rust
/// Swap the audio buffer in a running stream's RwLock.
/// Returns the previous buffer. Glitch-free — the audio callback
/// picks up the new data on its next read-lock acquisition.
pub fn swap_audio(
    audio_lock: &Arc<RwLock<Arc<AudioData>>>,
    new_audio: Arc<AudioData>,
) -> Arc<AudioData> {
    let mut guard = audio_lock.write().expect("audio lock poisoned");
    let old = Arc::clone(&*guard);
    *guard = new_audio;
    old
}
```

Also, `start_playback` and `rebuild_stream` should return the `Arc<RwLock<Arc<AudioData>>>` so the main thread can later swap buffers without rebuilding. Add a new field to `PlaybackState`:

```rust
pub struct PlaybackState {
    pub playing: Arc<AtomicBool>,
    pub position: Arc<AtomicUsize>,
    pub audio_lock: Option<Arc<RwLock<Arc<AudioData>>>>,  // NEW
}
```

Both `start_playback` and `rebuild_stream` store the `Arc<RwLock<...>>` they create into the returned `PlaybackState` (or return it separately). This gives the main thread a handle to swap audio without touching the stream.

### 4. `src/input/handler.rs`

Add `'a'` key binding in `handle_normal`:

```rust
KeyCode::Char('a') => {
    if app.audio_data.is_some() && app.original_audio.is_some() {
        app.ab_original = !app.ab_original;
        Some(Action::ToggleAB)
    } else {
        None
    }
}
```

Only toggles when both buffers exist (file loaded and analysis complete).

### 5. `src/main.rs`

Handle the new action and result:

**`AnalysisDone(mono_original)`:**
```rust
ProcessingResult::AnalysisDone(mono_original) => {
    app.processing_status = None;
    app.original_audio = Some(Arc::new(mono_original));
    // Auto-resynthesize with current slider values
    let values = app.world_slider_values();
    processing.send(ProcessingCommand::Resynthesize(values));
}
```

**`SynthesisDone(audio)`:** Same as current, but also handle the first-time channel adjustment. Store as processed audio. If `ab_original` is false (listening to processed), swap the audio lock. If `ab_original` is true (listening to original), just store it — don't swap.

**`Action::ToggleAB`:**
```rust
Action::ToggleAB => {
    // ab_original was already flipped by the handler
    if let Some(ref lock) = app.playback.audio_lock {
        let target = if app.ab_original {
            app.original_audio.as_ref()
        } else {
            app.audio_data.as_ref()
        };
        if let Some(audio) = target {
            // Adjust position if buffer lengths differ (speed slider)
            let old_len = {
                let guard = lock.read().expect("audio lock poisoned");
                guard.samples.len()
            };
            let new_len = audio.samples.len();
            if old_len != new_len {
                let pos = app.playback.position.load(Ordering::Acquire);
                // Scale position proportionally
                let fraction = if old_len > 0 { pos as f64 / old_len as f64 } else { 0.0 };
                let new_pos = (fraction * new_len as f64).round() as usize;
                app.playback.position.store(new_pos.min(new_len), Ordering::Release);
            }

            // Update file_info for the new active buffer
            if let Some(ref mut info) = app.file_info {
                info.total_samples = audio.samples.len();
                info.duration_secs = audio.duration_secs();
                info.channels = audio.channels;
            }

            audio::playback::swap_audio(lock, Arc::clone(audio));
        }
    }
}
```

**On `LoadFile`:** Reset A/B state:
```rust
app.ab_original = false;
app.original_audio = None;
```

### 6. `src/ui/transport.rs`

Enhance the A/B display. Currently shows `[A]` or `[B]` in magenta. Change to show the active source name and use distinct styling:

```rust
let ab_display = if app.ab_original {
    Span::styled(" [A: Original] ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD))
} else {
    Span::styled(" [B: Processed] ", Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD))
};
```

## Files NOT Modified

- **`src/dsp/modifier.rs`** — no changes needed, modifier logic is independent of A/B.
- **`src/dsp/world.rs`** — no changes needed.
- **`src/ui/status_bar.rs`** — no changes needed.
- **No new files** — everything fits into existing modules.

## Architecture

```
load_file()
  → start_playback() returns PlaybackState with audio_lock
  → Analyze(audio) sent to processing thread

processing thread:
  Analyze → AnalysisDone(mono_original) + stores WorldParams
  main stores original_audio = mono_original
  auto-sends Resynthesize(values)
  → SynthesisDone(processed_audio)
  main stores audio_data = processed_audio
  swaps audio_lock to processed (default: B)

'a' key pressed:
  ab_original flips
  swap_audio(lock, original_audio or audio_data)
  position scaled proportionally if lengths differ
  file_info updated for new active buffer
  transport shows [A: Original] or [B: Processed]
```

## Key Design Decisions

### 1. Swap via RwLock, not stream rebuild

The existing `CallbackContext.audio` is already `Arc<RwLock<Arc<AudioData>>>`. Swapping the inner `Arc` via `write()` is O(1) and glitch-free — the audio callback uses `try_read()` and outputs silence if the lock is briefly held. This avoids the ~1ms gap from rebuilding the cpal stream and is simpler.

### 2. `'a'` key instead of Tab

Tab cycles panel focus (World → Effects → Transport) since P2. All three panels need keyboard access for slider navigation. Reassigning Tab would break the TUI workflow. `'a'` is unused, mnemonic, and single-key.

### 3. Proportional position scaling on A/B switch

When the speed slider is non-default, the processed buffer has a different length than the original. Rather than clamping (which would jump to a different time), scale the position proportionally: `new_pos = (old_pos / old_len) * new_len`. This preserves the approximate time position across buffers of different durations.

### 4. Both buffers are mono

P3 established that WORLD always outputs mono. The neutral-slider shortcut also returns mono (via `to_mono()`). The `original_audio` stored in P4 is the mono version sent via `AnalysisDone` — never the raw stereo file. This means both A and B buffers are always mono, eliminating channel-mismatch issues on toggle.

### 5. No new dependencies

Everything uses existing crates.

## Edge Cases

| Scenario | Behavior |
|---|---|
| No file loaded, press `'a'` | No-op (guard: `audio_data.is_some() && original_audio.is_some()`) |
| Analysis not complete, press `'a'` | No-op (`original_audio` is `None` until `AnalysisDone`) |
| Resynthesis in progress, on B | Keeps playing old processed buffer. On `SynthesisDone`, new audio is stored in `audio_data` and swapped into the lock if still on B. |
| Resynthesis in progress, on A | `SynthesisDone` stores new `audio_data` but doesn't swap the lock (user is listening to original). When user presses `'a'` to go back to B, the latest processed buffer is used. |
| Speed changed, switch A↔B | Position scaled proportionally. Duration and total_samples updated in `file_info`. |
| Load new file while on A | `ab_original` reset to `false`, `original_audio` cleared. Fresh analysis starts. |

## Implementation Order

1. `src/app.rs` — add `original_audio` field, `Action::ToggleAB`
2. `src/dsp/processing.rs` — change `AnalysisDone` to carry mono original
3. `src/audio/playback.rs` — add `audio_lock` to `PlaybackState`, add `swap_audio` function
4. `src/input/handler.rs` — add `'a'` key binding
5. `src/main.rs` — handle `AnalysisDone(mono)`, `Action::ToggleAB`, reset on load
6. `src/ui/transport.rs` — enhanced A/B display
7. `cargo clippy` + `cargo test` — all 18 existing tests pass, no new tests needed (A/B is a UI interaction)

## Verification

1. `cargo clippy --workspace` — zero warnings
2. `cargo test` — 18 tests pass
3. `cargo run -- assets/test_samples/speech_like_5s.wav` — wait for analysis
4. Adjust Pitch Shift to +5 → audio changes (processed playing)
5. Press `'a'` → transport shows `[A: Original]`, audio is unmodified original
6. Press `'a'` → transport shows `[B: Processed]`, audio is pitched
7. No click/pop or time jump on toggle
8. Adjust Speed to 1.5 → processed is shorter. Press `'a'` → time position adjusts proportionally
9. Press `'a'` before analysis completes → nothing happens
10. Press `'o'`, load new file → A/B resets to B, new analysis starts

## Changes from v1

| v1 (old plan) | v2 (this plan) | Reason |
|---|---|---|
| New `DualBuffer` struct in `src/audio/buffer.rs` | No new file; extend existing `PlaybackState` | `PlaybackState` already owns position/playing; `CallbackContext` already has `RwLock` |
| `DualBuffer.position: Arc<AtomicUsize>` | Reuse `PlaybackState.position` | Avoid duplicate position tracking |
| `DualBuffer.original: Arc<Vec<f32>>` | `app.original_audio: Option<Arc<AudioData>>` | Keep `AudioData` (carries sample_rate, channels), consistent with rest of codebase |
| Tab key toggles A/B | `'a'` key toggles A/B | Tab is bound to panel focus cycle since P2 |
| Rebuild stream on toggle | Swap `Arc<AudioData>` via existing `RwLock` | Glitch-free, O(1), no cpal stream teardown |
| Assumes both buffers same format | Both buffers always mono (P3 guarantee) | WORLD outputs mono; neutral shortcut also returns mono |
| No position adjustment for speed changes | Proportional position scaling | Speed slider changes buffer length; clamping would cause time jump |
