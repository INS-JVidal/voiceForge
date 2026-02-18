# P0 — Scaffold & WORLD Vocoder FFI: Implementation Report

## Scope

Set up the Cargo workspace, vendor the WORLD C++ vocoder source, build FFI bindings, expose a safe Rust API, and verify correctness with a roundtrip integration test.

## Files Created

| File | Purpose |
|---|---|
| `Cargo.toml` | Workspace root (`voiceforge` + `crates/world-sys`) |
| `src/main.rs` | Placeholder entry point |
| `crates/world-sys/Cargo.toml` | FFI crate, `cc` build dependency |
| `crates/world-sys/build.rs` | Compiles vendored C++ via `cc` crate |
| `crates/world-sys/world-src/` | Vendored C++ from mmorise/World (MIT) |
| `crates/world-sys/world-src/world/` | WORLD headers (subdirectory matches `#include "world/..."` paths) |
| `crates/world-sys/src/lib.rs` | Raw `extern "C"` FFI declarations + init helpers |
| `crates/world-sys/src/safe.rs` | Safe Rust API: `analyze()`, `synthesize()`, `WorldParams` |
| `tests/test_world_ffi.rs` | 11 integration tests (roundtrip + edge cases) |

## What Was Implemented

### Workspace structure

Standard Cargo workspace with a root binary crate (`voiceforge`) depending on a path subcrate (`world-sys`). The subcrate is MIT-licensed (matching WORLD's license); the root crate is GPL-3.0-or-later.

### Vendored C++ compilation

- WORLD C++ source cloned from `github.com/mmorise/World` into `crates/world-sys/world-src/`.
- Headers placed under `world-src/world/` to match the `#include "world/dio.h"` paths used by the C++ source.
- `build.rs` uses `cc::Build` with C++11 standard, warnings suppressed (vendored code), and `cargo:rerun-if-changed=world-src/` to avoid unnecessary rebuilds.

### FFI bindings (`lib.rs`)

Declares `extern "C"` bindings for the full WORLD analysis/synthesis pipeline:

- **DIO** — f0 (pitch) estimation with `DioOption` struct
- **StoneMask** — f0 refinement
- **CheapTrick** — spectral envelope extraction with `CheapTrickOption` struct
- **D4C** — aperiodicity estimation with `D4COption` struct
- **Synthesis** — waveform reconstruction from f0/spectrogram/aperiodicity
- **Helpers** — `GetSamplesForDIO`, `GetFFTSizeForCheapTrick`

All FFI types and functions are `pub(crate)` — not exposed to downstream crates.

Two generic init helpers (`init_option`, `init_option_with_fs`) eliminate repeated `MaybeUninit` boilerplate.

### Safe wrapper (`safe.rs`)

Two public functions:

- `analyze(audio: &[f64], sample_rate: i32) -> WorldParams` — runs the full DIO -> StoneMask -> CheapTrick -> D4C pipeline.
- `synthesize(params: &WorldParams, sample_rate: i32) -> Vec<f64>` — reconstructs audio from (possibly modified) parameters.

`WorldParams` holds `f0`, `temporal_positions`, `spectrogram` (Vec of Vec rows), `aperiodicity`, `fft_size`, and `frame_period`. Derives `Debug` and `Clone`.

### Test suite

11 tests in `tests/test_world_ffi.rs`:

| Test | What it verifies |
|---|---|
| `test_world_ffi_roundtrip` | Full analyze->synthesize cycle: f0 near 440 Hz, RMS energy ratio 0.3-3.0, peak cross-correlation > 0.7 |
| `test_world_ffi_clone_params` | `WorldParams` clone produces identical data |
| `test_world_ffi_analyze_empty_audio` | Panics on empty input |
| `test_world_ffi_analyze_zero_sample_rate` | Panics on sample_rate=0 |
| `test_world_ffi_analyze_negative_sample_rate` | Panics on sample_rate=-1 |
| `test_world_ffi_synthesize_zero_sample_rate` | Panics on sample_rate=0 |
| `test_world_ffi_synthesize_empty_f0` | Panics on empty f0 |
| `test_world_ffi_synthesize_mismatched_spectrogram` | Panics when spectrogram rows != f0 length |
| `test_world_ffi_synthesize_mismatched_aperiodicity` | Panics when aperiodicity rows != f0 length |
| `test_world_ffi_synthesize_wrong_spectrogram_width` | Panics when spectrogram row width != fft_size/2+1 |
| `test_world_ffi_synthesize_mismatched_temporal_positions` | Panics when temporal_positions length != f0 length |

## Issues Found and Corrected

### Review Round 1

| # | Issue | Severity | Fix |
|---|---|---|---|
| 1 | `analyze()` panicked on empty audio (no guard) | Bug | Added `assert!(!audio.is_empty())` |
| 2 | `synthesize()` arithmetic underflow on empty `f0` (`len - 1` with len=0) | Bug | Added `assert!(!params.f0.is_empty())` |
| 3 | `std::mem::zeroed` on FFI option structs — fragile if WORLD adds pointer/bool fields | Robustness | Replaced with `MaybeUninit` + `assume_init()` |
| 4 | FFI `repr(C)` structs lacked `Debug` derive | Quality | Added `#[derive(Debug)]` |
| 5 | `build.rs` didn't suppress vendored C++ warnings (~60 lines of noise per build) | Quality | Added `.warnings(false)` |
| 6 | `build.rs` missing `cargo:rerun-if-changed` — recompiled C++ on every build | Performance | Added `println!("cargo:rerun-if-changed=world-src/")` |
| 7 | Raw FFI types (`DioOption`, etc.) were `pub` — leaked unsafe internals | Architecture | Changed to `pub(crate)`, only safe API is exported |
| 8 | Test function name (`test_world_roundtrip`) didn't match expected filter (`cargo test test_world_ffi`) | Consistency | Renamed to `test_world_ffi_roundtrip` |

### Review Round 2

| # | Issue | Severity | Fix |
|---|---|---|---|
| 1 | `synthesize()` didn't validate `WorldParams` consistency — mismatched dimensions caused UB in C code | **Safety hole** | Added `WorldParams::validate()` checking all dimension invariants before FFI calls |
| 2 | `audio.len() as c_int` silent truncation on inputs > i32::MAX | Robustness | Added `assert!(audio.len() <= c_int::MAX as usize)` |
| 3 | `GetSamplesForDIO` return value unchecked — negative/zero could cause UB | Robustness | Added `assert!(f0_length_raw > 0)` |
| 4 | `pub` on `pub(crate)` struct fields — misleading visibility | Consistency | Changed fields to `pub(crate)` |
| 5 | No `Clone` on `WorldParams` — needed for A/B buffers in later phases | Architecture | Added `#[derive(Clone)]` |
| 6 | `MaybeUninit` init pattern repeated 3 times identically | Cohesion | Extracted `init_option()` and `init_option_with_fs()` helpers |
| 7 | Redundant `let fs = sample_rate as c_int` cast (`i32` is `c_int`) | Clarity | Removed the cast, use `sample_rate` directly |
| 8 | No `#[must_use]` on expensive return values | Quality | Added to `analyze()` and `synthesize()` |

## Final State

- `cargo build` — clean, no warnings
- `cargo build --release` — clean
- `cargo clippy --workspace` — zero warnings
- `cargo test --test test_world_ffi` — 11/11 pass
- Roundtrip metrics: mean f0 = 439.9 Hz (target 440), RMS ratio = 1.05, peak correlation = 0.79
