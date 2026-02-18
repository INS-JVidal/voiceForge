# P1 — Audio Decoder & Playback Engine: Implementation Report

## Scope

Load audio files (WAV/MP3/FLAC) into f32 PCM buffers, play them back through system audio output with play/pause/seek controls via keyboard, and verify with integration tests.

## Files Created

| File | Purpose |
|---|---|
| `src/lib.rs` | Library root, exposes `pub mod audio` for integration tests |
| `src/audio/mod.rs` | Audio module, re-exports `decoder` and `playback` |
| `src/audio/decoder.rs` | Symphonia-based audio file decoder |
| `src/audio/playback.rs` | cpal-based audio playback engine |
| `src/main.rs` | Updated: minimal CLI player with keyboard controls |
| `tests/test_decoder.rs` | 4 integration tests for the decoder |
| `assets/test_samples/` | Generated test WAV files (created by tests) |

## Files Modified

| File | Change |
|---|---|
| `Cargo.toml` | Added `cpal`, `symphonia`, `ratatui` deps; `hound` as dev-dependency |

## What Was Implemented

### Audio decoder (`src/audio/decoder.rs`)

- **`AudioData`** struct: holds interleaved f32 PCM samples, sample rate, channel count. Derives `Debug`, `Clone`. Has `duration_secs()` and `frame_count()` helper methods (both `#[must_use]`). Guards against division by zero when channels or sample_rate is 0.
- **`DecoderError`** enum: `Io`, `UnsupportedFormat`, `UnsupportedCodec`, `Decode` variants. Implements `Display`, `Error` (with `source()` for `Io`), `From<io::Error>`.
- **`decode_file(path: &Path) -> Result<AudioData, DecoderError>`**: uses symphonia to probe format, find default audio track, decode all packets into interleaved f32 via `SampleBuffer`. Supports WAV, MP3, FLAC. Key behaviors:
  - Provides file extension hints to symphonia for faster format detection
  - Handles non-fatal decode errors gracefully (skip and continue)
  - Recreates `SampleBuffer` when a packet needs more interleaved samples than the existing buffer can hold (compares `frames × channels` consistently)
  - Returns error if no samples were decoded

### Audio playback (`src/audio/playback.rs`)

- **`PlaybackState`** struct: thread-safe playback control via `Arc<AtomicBool>` (playing) and `Arc<AtomicUsize>` (position). Derives `Debug`. Implements `Default`. Methods:
  - `toggle_playing()` — atomic `fetch_xor` flip, returns new state (no TOCTOU race)
  - `seek_by_samples()` — signed offset, clamped to `[0, max_samples]`
  - `seek_by_secs()` — converts seconds to interleaved sample offset via `secs × sample_rate × channels`
  - `current_time_secs()` — current position as seconds, guards against zero sample_rate/channels

- **`PlaybackError`** — newtype around private `String`, implements `Display`, `Error`.

- **`start_playback(audio: Arc<AudioData>) -> Result<(Stream, PlaybackState), PlaybackError>`**: opens default cpal output device, builds output stream matching device sample format (f32/i16/u16), creates shared `PlaybackState`. The audio callback (`write_audio_data`):
  - Fills silence when paused
  - Guards against zero audio/device channels (fills silence)
  - Reads from interleaved buffer at current position
  - Maps audio channels to device channels (mono→stereo via `dev_ch % audio_channels`)
  - Fills silence past end of audio
  - Uses `CallbackContext` struct (6 fields) to bundle callback parameters cleanly

### CLI player (`src/main.rs`)

- Accepts file path as command-line argument
- Decodes file, starts playback via cpal
- Enters crossterm raw mode (via `ratatui::crossterm`) for keyboard input
- **`TerminalGuard`** RAII struct restores terminal (LeaveAlternateScreen + disable_raw_mode) on `Drop`, including panics
- Renders file metadata, playback time, and controls in the alternate screen using crossterm `MoveTo`/`Clear` APIs with `\r\n` line endings for correct raw-mode output
- Controls: Space (play/pause), `]` (seek +5s), `[` (seek -5s), `q`/Esc (quit)
- Display refreshes at ~10 FPS via 100ms poll timeout

### Library structure (`src/lib.rs`)

`pub mod audio` so integration tests can access `voiceforge::audio::decoder` and `voiceforge::audio::playback`. The binary (`main.rs`) imports from the library crate.

### Test suite

4 tests in `tests/test_decoder.rs`:

| Test | What it verifies |
|---|---|
| `test_decoder_wav_basic` | Decode mono WAV: correct sample rate, channels, duration ~0.5s, samples in [-1, 1] |
| `test_decoder_stereo_wav` | Decode stereo WAV: correct channels=2, interleaved sample count, duration ~1.0s |
| `test_decoder_invalid_path` | Returns error on nonexistent file |
| `test_decoder_frame_count` | `frame_count()` matches expected (sample_rate × duration for mono) |

Tests generate WAV files on-the-fly using `hound` (dev-dependency). Each test uses a unique file name via the shared `test_wav_path()` helper to avoid parallel test races.

## Integration with P0

- P0's `world-sys` crate and all 11 tests remain untouched and passing
- `world-sys` is still listed as a dependency in workspace `Cargo.toml`
- The `src/lib.rs` addition doesn't conflict with P0's structure — P0 only used `src/main.rs` as a placeholder

## Dependencies Introduced

| Crate | Version | Purpose |
|---|---|---|
| `cpal` | 0.17 | Cross-platform audio output (ALSA on Linux/WSL2) |
| `symphonia` | 0.5 | Audio file decoding (WAV, MP3, FLAC) |
| `ratatui` | 0.30 | Terminal UI framework (crossterm re-export used for raw mode keyboard input) |
| `hound` | 3.5 (dev) | WAV file generation for tests |

## System Dependencies Required

- `pkg-config` — needed by `alsa-sys` build script
- `libasound2-dev` — ALSA development headers for cpal

## Design Decisions

1. **`AudioData` uses `Vec<f32>` not `Vec<f64>`**: cpal and audio playback operate on f32. WORLD uses f64, so conversion will happen at the WORLD boundary (P3). This avoids unnecessary f64→f32 round-trips during playback.

2. **`CallbackContext` struct**: bundles the 6 parameters that the audio callback needs, avoiding clippy's `too_many_arguments` warning and making the callback interface cleaner.

3. **Channel mapping in playback**: mono audio plays on all device channels; stereo maps naturally. Done via `dev_ch % audio_channels` in the callback, with a zero-channels guard.

4. **Test WAV generation**: tests use `hound` to generate WAV files on the fly rather than checking in binary assets. This keeps the repo clean and makes tests self-contained.

5. **Library + binary split**: `src/lib.rs` exposes the `audio` module publicly so integration tests can import `voiceforge::audio::decoder`. The binary `main.rs` imports from the library crate.

6. **`TerminalGuard` RAII pattern**: ensures terminal is always restored on exit, including panics. Consistent with Rust idiom of using `Drop` for cleanup of external resources.

7. **Atomic `toggle_playing`**: uses `fetch_xor(true, Relaxed)` instead of load-then-store to eliminate TOCTOU races with the audio callback thread.

8. **Dynamic `SampleBuffer` resizing**: the decoder compares `existing.capacity()` against `frames × channels` (both in interleaved sample units) and recreates the buffer when a packet exceeds it. This handles variable-size packets in compressed formats like MP3.

## Review Fixes

### Round 1

9 issues found and fixed:

| # | Severity | Issue | Fix |
|---|---|---|---|
| 1 | Bug | `println!` in raw mode produces garbled staircase output (`\n` without `\r`) | Replaced with `write!` using `\r\n` line endings |
| 2 | Bug | File metadata printed before `EnterAlternateScreen` — never visible to user | Moved file info into the main render loop inside alternate screen |
| 3 | Bug | `SampleBuffer` created from first packet capacity; later larger packets would panic | Recreate buffer when current packet exceeds existing capacity |
| 4 | Bug | `test_decoder_wav_basic` and `test_decoder_frame_count` share same file path — parallel test race | Each test uses a unique file name via shared `test_wav_path()` helper |
| 5 | Robustness | Panic between `enable_raw_mode` and `disable_raw_mode` leaves terminal in raw mode | Added `TerminalGuard` RAII struct that restores terminal on `Drop` |
| 6 | Robustness | `dev_ch % ac` panics when `audio_channels == 0` | Added early return with silence when `ac == 0 \|\| dc == 0` |
| 7 | Quality | Raw ANSI escapes (`\x1b[H\x1b[2J`) instead of crossterm API | Replaced with `crossterm::cursor::MoveTo` and `crossterm::terminal::Clear` |
| 8 | Quality | Redundant `#[must_use]` on `decode_file()` (`Result` already has it) | Removed the attribute |
| 9 | Quality | Missing `Debug` derive on `PlaybackState` (P0 pattern: all public types derive Debug) | Added `#[derive(Debug)]` |

### Round 2

6 additional issues found and fixed:

| # | Severity | Issue | Fix |
|---|---|---|---|
| 1 | Bug | `SampleBuffer` capacity check compared interleaved samples vs frames — wrong units for multichannel audio | Compare `existing.capacity()` against `frames × channels` (both in interleaved samples) |
| 2 | Bug | `toggle_playing()` load-then-store TOCTOU race on shared `AtomicBool` | Replaced with single `fetch_xor(true, Relaxed)` |
| 3 | Dead code | `looping` flag initialized, checked in callback, cloned into context — but never set to `true` anywhere | Removed `looping` from `PlaybackState`, `CallbackContext`, and callback logic |
| 4 | Quality | `PlaybackError` inner `String` field was `pub` — breaks P0 visibility convention | Changed to private |
| 5 | Quality | `generate_test_wav` doc comment said "return the path" but returns `()` | Fixed doc comment |
| 6 | Quality | `test_decoder_stereo_wav` duplicated directory logic instead of using `test_wav_path()` helper | Refactored to use the shared helper |

## Final State

- `cargo build` — clean, no warnings
- `cargo clippy --workspace` — zero warnings
- `cargo test` — 15/15 pass (11 P0 + 4 P1)
- P0 tests fully unchanged and passing
