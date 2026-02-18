# P4 — A/B Comparison Toggle

## Goal
Enable instant switching between original and processed audio during playback using the `Tab` key. Both buffers share the same seek position so the switch is seamless.

## Prerequisite
P3 complete (original + processed buffers both exist).

## Steps

### 4.1 Dual buffer architecture — `src/audio/buffer.rs`
```rust
pub struct DualBuffer {
    pub original: Arc<Vec<f32>>,
    pub processed: Arc<RwLock<Vec<f32>>>,
    pub active_source: Arc<AtomicBool>,  // false = original, true = processed
    pub position: Arc<AtomicUsize>,      // shared sample position
}
```

### 4.2 Playback integration
Modify the cpal callback in `src/audio/playback.rs`:
- Read `active_source` flag each callback
- Pull samples from either `original` or `processed` buffer based on flag
- Position advances identically regardless of which buffer is active
- Handle case where processed buffer is shorter/longer than original (speed slider changed duration):
  - Clamp position to the shorter buffer's length
  - Or pad shorter buffer with silence

### 4.3 Tab key handler
In `src/input/handler.rs`:
- `Tab` → flip `active_source` AtomicBool
- Update the transport bar to show current source: `[A/B: Original]` or `[A/B: Processed]`

### 4.4 Visual indicator in transport — `src/ui/transport.rs`
```
 [A: Original ●]  [B: Processed ○]
```
Highlight the active source. Use color or bold to make the active selection obvious.

### 4.5 Edge cases
- If no file is loaded, Tab does nothing
- If analysis hasn't completed yet (no processed buffer), show "Processing..." and keep on original
- If processed buffer is being updated (resynthesis in progress), keep playing the old processed buffer until the new one is ready — then swap atomically

## Human Test Checklist

- [ ] Load a file, wait for analysis to complete
- [ ] Move a slider (e.g., Pitch Shift +5) → audio changes
- [ ] Press `Tab` → audio instantly switches to original (unmodified) sound
- [ ] Press `Tab` again → back to processed (pitched) sound
- [ ] Transport bar label updates to show which source is active
- [ ] Switching does not cause any click/pop or position jump — audio continues from the same timestamp
- [ ] If you seek while on processed, then Tab to original, position is correct
- [ ] Tab before analysis completes → stays on original, no crash

## Dependencies Introduced
None new.

## Notes
- This is a relatively small phase but critical for usability — A/B comparison is the core workflow for voice tuning.
