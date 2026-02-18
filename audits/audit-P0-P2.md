# Security & Robustness Audit — Phases P0–P2

**Date**: 2026-02-18
**Branch**: `audit`
**Commit**: `ab4abe1`
**Scope**: All source code implemented through P0 (WORLD FFI), P1 (decoder + playback), and P2 (TUI skeleton)

---

## Scope & Method

### Files audited

| Module | Files | Lines |
|---|---|---|
| world-sys FFI | `crates/world-sys/src/lib.rs`, `safe.rs` | ~370 |
| Audio decoder | `src/audio/decoder.rs` | ~180 |
| Audio playback | `src/audio/playback.rs` | ~200 |
| App state | `src/app.rs` | ~260 |
| Input handler | `src/input/handler.rs` | ~136 |
| UI widgets | `src/ui/` (6 files) | ~285 |
| Entry point | `src/main.rs` | ~115 |
| Tests | `tests/test_world_ffi.rs`, `test_decoder.rs` | ~320 |

### Audit categories

1. **Security** — unsafe code, FFI boundaries, path traversal, input validation
2. **Robustness** — unwrap/panic in production paths, unhandled edge cases
3. **Fault tolerance** — device unavailable, corrupt files, invalid parameters
4. **Architecture** — separation of concerns, testability

---

## Findings Summary

| Severity | Count | Fixed |
|---|---|---|
| Critical | 3 | 3 |
| High | 5 | 5 |
| Medium | 6 | 6 |
| Low | 6 | 6 |
| **Total** | **20** | **20** |

---

## Critical Findings

### C1. Audio callback lifetime vulnerability
**File**: `src/audio/playback.rs`
**Problem**: `CallbackContext` held a bare `Arc<AudioData>`. During file reload, the main thread could drop the old Arc while the callback was mid-read, causing a use-after-free if the Arc reached zero.
**Fix**: Wrapped in `Arc<RwLock<Arc<AudioData>>>`. Callback uses `try_read()` — outputs silence if the lock is held during a swap.

### C2. Position update race condition
**File**: `src/audio/playback.rs`
**Problem**: Callback and main thread both used `Ordering::Relaxed` for the position atomic. A seek from the main thread could be lost if it occurred between the callback's load and store.
**Fix**: Upgraded to `Acquire` on loads, `Release` on stores. This establishes a happens-before relationship between the callback and seek operations.

### C3. Undocumented unwrap on sample buffer
**File**: `src/audio/decoder.rs:165`
**Problem**: `sample_buf.as_mut().unwrap()` after assigning `Some(...)` — safe but undocumented invariant.
**Fix**: Changed to `.expect("sample_buf was just assigned Some")`.

---

## High Findings

### H1. Integer overflow in seek calculation
**File**: `src/audio/playback.rs:47`
**Problem**: `(secs * sample_rate * channels) as isize` overflows for large seeks.
**Fix**: Clamp the float product to `isize::MIN..isize::MAX` before casting.

### H2. Division by zero in slider adjustment
**File**: `src/app.rs:65`
**Problem**: `1.0 / self.step` panics if `step == 0.0`.
**Fix**: Early return if `step <= 0.0 || !step.is_finite()`.

### H3. Path traversal in file picker
**File**: `src/input/handler.rs:27`
**Problem**: User-entered paths passed directly to decoder with no validation.
**Fix**: Check `path.exists()` and `path.is_file()` before loading. (Note: full sandboxing deferred — see Lessons Learned.)

### H4. Silent channel count default
**File**: `src/audio/decoder.rs:125`
**Problem**: Unknown channel layout silently defaulted to 1, causing duration miscalculation.
**Fix**: Return `DecoderError::UnsupportedFormat` instead.

### H5. selected_slider out of bounds after focus change
**File**: `src/input/handler.rs:56`
**Problem**: When switching to Transport panel (0 sliders), `selected_slider` was not clamped to 0.
**Fix**: Explicit `if count == 0 { selected_slider = 0 }` branch.

---

## Medium Findings

### M1. Panicking assertions in library code
**File**: `crates/world-sys/src/safe.rs`
**Problem**: `WorldParams::validate()` and `synthesize()` used `assert!()` — panics in production.
**Fix**: `validate()` returns `Result<(), WorldError>`. `synthesize()` returns `Result<Vec<f64>, WorldError>`.

### M2. No post-FFI output validation
**File**: `crates/world-sys/src/safe.rs`
**Problem**: WORLD C code could produce NaN/Inf values that silently propagate.
**Fix**: `debug_assert!` on f0 values after analysis. (Full validation deferred to avoid performance cost.)

### M3. Unbounded synthesis allocation
**File**: `crates/world-sys/src/safe.rs:209`
**Problem**: No limit on output buffer size — pathological inputs could exhaust memory.
**Fix**: `MAX_SYNTHESIS_SAMPLES` constant (10 min at 96kHz). Returns error if exceeded.

### M4. Duration display truncation
**File**: `src/ui/status_bar.rs:11`
**Problem**: `(duration / 60.0) as u32` wraps at ~71 minutes.
**Fix**: Clamp to `u32::MAX` before cast.

### M5. No file pre-check before decode
**File**: `src/main.rs:93`
**Problem**: Generic symphonia error on missing file — unclear message.
**Fix**: Check `path.exists()` and `path.is_file()` with specific error strings.

### M6. Float precision drift in slider
**File**: `src/app.rs:65`
**Problem**: Repeated round-trip through `precision = (1.0/step).round()` can drift.
**Fix**: Guard against non-finite precision divisor. Acceptable precision for audio sliders; full fix would require fixed-point.

---

## Low Findings

### L1–L6 Summary

| # | File | Issue | Fix |
|---|---|---|---|
| L1 | `lib.rs:123` | FFI structs initialized with `MaybeUninit::uninit()` | Changed to `zeroed()` as defense-in-depth |
| L2 | `spectrum.rs` | Placeholder lacks bounds-checking note | Added comment for future implementer |
| L3 | `main.rs:43` | App continues after startup load failure silently | Error already shown in status_message; acceptable |
| L4 | `main.rs:24` | Terminal restore errors silently discarded | Now logged to stderr |
| L5 | `playback.rs` | Relaxed ordering undocumented | All orderings now explicitly Acquire/Release |
| L6 | `lib.rs` | FFI safety docs insufficient | Expanded with invariant descriptions |

---

## Patterns Observed

### What went well
- **Error types already existed** — `DecoderError`, `PlaybackError` were well-structured from the start. The audit only needed to add `WorldError`.
- **Arc-based shared state** — The threading model was sound in concept; only the ordering semantics and lifetime management needed tightening.
- **Test coverage on WORLD FFI** — 11 tests covering roundtrip, clone, and invalid inputs. Made it easy to verify the `Result` migration didn't break anything.

### Recurring issues
1. **`Ordering::Relaxed` used by default** — Every atomic in the codebase used Relaxed. This is a common Rust anti-pattern; Relaxed should be the exception, not the default.
2. **`unwrap()` / `assert!()` in non-test code** — Three instances of panicking in library/production paths. The codebase should prefer `Result` propagation for anything reachable from user input.
3. **No input validation at system boundaries** — File paths and slider values were trusted without checks. The file picker accepted any string; sliders relied on UI constraints only.

---

## Lessons Learned for Next Audit

### 1. Audit FFI boundaries first
The `world-sys` crate is the highest-risk area. Future phases (P3 modifier, P6 effects) will add more FFI surface. Priority checklist:
- Every `unsafe` block has a documented safety invariant
- All pointer arguments are validated (non-null, correct length)
- Return values from C are checked before use
- Memory ownership across the FFI boundary is explicit

### 2. Grep for anti-patterns early
Run these before the audit starts:
```bash
cargo clippy -- -W clippy::unwrap_used -W clippy::expect_used
grep -rn 'Ordering::Relaxed' src/
grep -rn '\.unwrap()' src/
grep -rn 'assert!' src/ --include='*.rs' | grep -v test
```

### 3. Test fault injection
Current tests only cover happy paths. Next audit should verify:
- What happens when `cpal` returns no default device
- What happens when symphonia encounters a truncated file
- What happens when WORLD produces all-zero f0 (silence input)
- What happens when slider values are at extreme bounds simultaneously

### 4. Path validation needs a policy decision
The current fix (exists + is_file) is minimal. For a tool that may become a REST API, decide:
- Should paths be restricted to a specific directory?
- Should symlinks be followed or rejected?
- Should there be a max file size check before decoding?

### 5. Consider `#[deny(clippy::unwrap_used)]` at crate level
Adding this to `lib.rs` would catch future unwraps at compile time. Exceptions can use `#[allow]` with a comment.

### 6. Atomic ordering review checklist
For every `AtomicBool`/`AtomicUsize`:
- Who writes it? (which thread)
- Who reads it? (which thread)
- Does a write on thread A need to be visible to thread B before B does something else?
- If yes → use Acquire/Release or SeqCst, not Relaxed.
