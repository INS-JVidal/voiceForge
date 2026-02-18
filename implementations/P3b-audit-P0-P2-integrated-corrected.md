# P3b — Audit Integration Corrections

## Context

After P3 was committed (`74d4920`), the P0–P2 security audit branch (`audit`) was merged (`3a883a8`). The audit introduced 20 robustness and security fixes across the codebase. Two of those changes broke P3 code with compile errors. This document records the problems found and the fixes applied in commit `f02b09f`.

## Audit Changes That Affected P3

The audit branch (`ab4abe1`) modified 10 files. Two changes created API incompatibilities with P3 code:

### 1. `CallbackContext` Restructured — `src/audio/playback.rs`

**What the audit changed:**

The audio callback's `CallbackContext` struct was redesigned for safe hot-swap of audio data during file reload. The audit replaced the direct `Arc<AudioData>` with `Arc<RwLock<Arc<AudioData>>>`, allowing the main thread to atomically swap in new audio data without invalidating the callback reference. As part of this, two struct fields were removed:

```rust
// Before (P3 code assumed this shape):
struct CallbackContext {
    audio: Arc<AudioData>,
    playing: Arc<AtomicBool>,
    position: Arc<AtomicUsize>,
    total_samples: usize,       // ← removed
    audio_channels: u16,        // ← removed
    device_channels: u16,
}

// After (audit):
struct CallbackContext {
    audio: Arc<RwLock<Arc<AudioData>>>,
    playing: Arc<AtomicBool>,
    position: Arc<AtomicUsize>,
    device_channels: u16,
}
```

`total_samples` and `audio_channels` are now read from the `AudioData` behind the `RwLock` at callback time, rather than being cached at stream creation time. This is correct — the cached values would become stale on audio data swap.

**How P3 broke:**

`rebuild_stream()` (added in P3) constructed a `CallbackContext` using the old field layout:

```rust
// P3's rebuild_stream — compile error after audit merge
let ctx = CallbackContext {
    playing: Arc::clone(&state.playing),
    position: Arc::clone(&state.position),
    total_samples: audio.samples.len(),   // E0560: field doesn't exist
    audio_channels: audio.channels,       // E0560: field doesn't exist
    device_channels: config.channels,
    audio,                                // E0308: type mismatch
};
```

Three compile errors:
- `E0560` — `total_samples` field no longer exists
- `E0560` — `audio_channels` field no longer exists
- `E0308` — `audio` field expects `Arc<RwLock<Arc<AudioData>>>`, got `Arc<AudioData>`

**Fix applied:**

Updated `rebuild_stream` to construct `CallbackContext` matching the new struct shape, wrapping the audio in `Arc::new(RwLock::new(audio))`:

```rust
let ctx = CallbackContext {
    playing: Arc::clone(&state.playing),
    position: Arc::clone(&state.position),
    device_channels: config.channels,
    audio: Arc::new(RwLock::new(audio)),
};
```

### 2. `world_sys::synthesize` Returns `Result` — `crates/world-sys/src/safe.rs`

**What the audit changed:**

`world_sys::synthesize` was changed from a panicking function to one returning `Result<Vec<f64>, WorldError>`. This was part of a broader pattern: `WorldParams::validate()` now returns `Result<(), WorldError>` instead of panicking via `assert!`, and a new `MAX_SYNTHESIS_SAMPLES` guard prevents unreasonable allocations. A new `WorldError` enum was added:

```rust
// Before (P3 code assumed this):
pub fn synthesize(params: &WorldParams, sample_rate: i32) -> Vec<f64>

// After (audit):
pub fn synthesize(params: &WorldParams, sample_rate: i32) -> Result<Vec<f64>, WorldError>
```

**How P3 broke:**

`dsp::world::synthesize` called `world_sys::synthesize` and used the return value directly as `&[f64]`:

```rust
// P3's dsp::world::synthesize — compile error after audit merge
pub fn synthesize(params: &WorldParams, sample_rate: u32) -> AudioData {
    let samples = world_sys::synthesize(params, sample_rate as i32);
    //  E0308: expected &[f64], got &Result<Vec<f64>, WorldError>
    from_mono_f64(&samples, sample_rate)
}
```

Additionally, `tests/test_modifier.rs` called `world_sys::synthesize` directly in the neutral roundtrip test and used the return as `Vec<f64>`.

**Fixes applied (3 locations):**

1. **`src/dsp/world.rs`** — Changed signature to propagate the error:
   ```rust
   pub fn synthesize(params: &WorldParams, sample_rate: u32) -> Result<AudioData, WorldError> {
       let samples = world_sys::synthesize(params, sample_rate as i32)?;
       Ok(from_mono_f64(&samples, sample_rate))
   }
   ```

2. **`src/dsp/processing.rs`** — The processing thread now handles the error gracefully instead of assuming success. On synthesis failure, it sends a status message to the UI rather than panicking:
   ```rust
   match world::synthesize(&modified, sample_rate) {
       Ok(audio) => {
           let _ = result_tx.send(ProcessingResult::SynthesisDone(audio));
       }
       Err(e) => {
           let _ = result_tx.send(ProcessingResult::Status(format!("Synthesis error: {e}")));
       }
   }
   ```

3. **`tests/test_modifier.rs`** — Added `.unwrap()` to the two `world_sys::synthesize` calls in the neutral roundtrip test (test context — panicking on error is appropriate).

## Other Audit Changes (No P3 Impact)

The remaining audit changes compiled cleanly with P3 code:

| Audit change | Why no conflict |
|---|---|
| `Ordering::Relaxed` → `Acquire/Release` in `PlaybackState` | P3's `main.rs` accesses `position` via `PlaybackState` methods, not directly. The methods were updated by the audit. |
| `seek_by_secs` overflow clamping | P3 calls `seek_by_secs` through existing code paths, no direct usage. |
| `SliderDef::adjust` divide-by-zero guard | P3 doesn't modify `adjust()`. |
| File pre-existence check in `load_file` | P3 doesn't change `load_file`'s structure, just adds processing dispatch after it. |
| `TerminalGuard` logging errors | P3's `main.rs` had the old `let _ =` pattern, but the audit's version was picked up via merge. |
| `MaybeUninit::zeroed()` in FFI init | Internal to `world-sys`, no API change. |
| `world_sys::analyze` `debug_assert` for finite values | Returns `WorldParams` unchanged — no API change. |
| Test file `test_world_ffi.rs` updated for `Result` | Already adapted by audit, no P3-specific test changes needed there. |

## Summary

| File | Errors | Root Cause | Fix |
|---|---|---|---|
| `src/audio/playback.rs` | 3 (`E0560` ×2, `E0308` ×1) | `CallbackContext` struct redesigned | Match new struct fields, wrap audio in `RwLock` |
| `src/dsp/world.rs` | 1 (`E0308`) | `synthesize` returns `Result` | Propagate with `?`, change return type |
| `src/dsp/processing.rs` | 0 (logic fix) | Upstream now fallible | Handle `Err` with status message |
| `tests/test_modifier.rs` | 2 (`E0308`) | `synthesize` returns `Result` | Add `.unwrap()` |

Total: 4 compile errors + 2 logic adaptations across 4 files. All 18 tests pass after fixes.
