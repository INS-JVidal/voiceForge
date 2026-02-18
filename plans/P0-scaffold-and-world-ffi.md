# P0 — Scaffold Project & WORLD Vocoder FFI

## Goal
Set up the Cargo workspace, vendor the WORLD C++ source, build FFI bindings, and verify with a roundtrip test (analyze → synthesize ≈ original).

## Steps

### 0.1 Create project structure
```
voiceforge/
├── Cargo.toml              # Workspace root
├── crates/
│   └── world-sys/
│       ├── Cargo.toml
│       ├── build.rs        # cc crate compiles C++ sources
│       ├── world-src/      # Vendored C++ from mmorise/World (MIT)
│       └── src/
│           ├── lib.rs      # Raw unsafe extern "C" FFI declarations
│           └── safe.rs     # Safe Rust wrapper API
├── src/
│   └── main.rs             # Minimal "hello world" placeholder
└── tests/
    └── test_world_ffi.rs   # Roundtrip integration test
```

### 0.2 Workspace Cargo.toml
```toml
[workspace]
members = [".", "crates/world-sys"]

[package]
name = "voiceforge"
version = "0.1.0"
edition = "2021"
license = "GPL-3.0-or-later"

[dependencies]
world-sys = { path = "crates/world-sys" }
```

### 0.3 Vendor WORLD C++ source
Clone `github.com/mmorise/World` and copy into `crates/world-sys/world-src/`:
- Source files: `d4c.cpp`, `dio.cpp`, `cheaptrick.cpp`, `stonemask.cpp`, `synthesis.cpp`, `harvest.cpp`, `codec.cpp`, `common.cpp`, `fft.cpp`, `matlabfunctions.cpp`
- Headers: all `.h` files from `src/` and `src/world/`

### 0.4 world-sys build.rs
Use `cc` crate to compile all `.cpp` files with:
- C++11 or later standard
- Include path pointing to `world-src/` for headers
- Link as static library

### 0.5 world-sys FFI bindings (lib.rs)
Declare `extern "C"` functions for:
- `Dio` / `Harvest` — f0 estimation
- `StoneMask` — f0 refinement
- `CheapTrick` — spectral envelope extraction
- `D4C` — aperiodicity estimation
- `Synthesis` — waveform synthesis from f0/sp/ap
- Helper functions: `GetSamplesForDIO`, `GetFFTSizeForCheapTrick`

### 0.6 Safe Rust wrapper (safe.rs)
Expose two high-level functions:
```rust
pub fn analyze(audio: &[f64], sample_rate: i32) -> WorldParams {
    // Calls Dio → StoneMask → CheapTrick → D4C
    // Returns WorldParams { f0, spectrogram, aperiodicity, fft_size, frame_count }
}

pub fn synthesize(params: &WorldParams, sample_rate: i32) -> Vec<f64> {
    // Calls Synthesis
    // Returns reconstructed audio
}
```

### 0.7 Roundtrip integration test
`tests/test_world_ffi.rs`:
1. Generate a synthetic sine wave (e.g., 440 Hz, 1 second, 44100 Hz sample rate)
2. Call `analyze()` to extract f0/sp/ap
3. Call `synthesize()` with unmodified parameters
4. Verify output length matches input length
5. Verify output is similar to input (correlation > 0.9 or low RMS error)

## Human Test Checklist

- [ ] `cargo build` compiles without errors (C++ WORLD source compiles via cc)
- [ ] `cargo test test_world_ffi` passes — roundtrip produces audio similar to input
- [ ] Inspect `WorldParams` after analyze: f0 array contains values near 440 Hz for the test sine wave
- [ ] `cargo build --release` also works (optimized C++ compilation)

## Dependencies Introduced
- `cc` (build dependency for world-sys)
- `world-sys` (path dependency)

## Risk Notes
- WORLD uses `double` (f64) everywhere — the safe wrapper should accept/return f64. Conversion to f32 happens later when interfacing with audio playback.
- If WORLD headers have changed upstream, check that function signatures in lib.rs match the vendored version exactly.
