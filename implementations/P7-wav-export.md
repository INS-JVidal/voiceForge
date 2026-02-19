# P7 — WAV Export: Implementation Report

## Goal

Allow the user to save the currently heard audio (with all WORLD modifications, effects, and gain applied) to a 16-bit PCM WAV file by pressing `s`.

## Prerequisite

P6b complete (35 tests). The tool was preview-only — no way to save processed audio to disk. The `hound` crate was already in `[dev-dependencies]` for tests.

## What Was Built

### New Files (3)

**`src/audio/export.rs`** — WAV export module:

- `ExportError` — error type wrapping hound/IO errors with display formatting.
- `export_wav(samples, sample_rate, channels, path) -> Result<(), ExportError>` — writes interleaved f32 samples as 16-bit PCM WAV. Each sample is clamped to [-1.0, 1.0] and scaled to i16 range (`* 32767.0`). Uses symmetric range (−32767..+32767) which is standard practice.
- `default_export_path(source_path) -> String` — builds `{dir}/{stem}_processed.wav` from the source file path. If the file already exists, tries `{stem}_processed_2.wav`, `_3.wav`, etc. until a free name is found.

**`src/ui/save_dialog.rs`** — Save dialog overlay (mirrors `file_picker.rs`):

- Centered popup (60% width, 5 rows) with cyan border and title " Save WAV ".
- Prompt: "Enter output path (Esc to cancel):".
- Shows the editable path with a cursor block character.
- Reuses `file_picker_input` field from `AppState` (safe because `FilePicker` and `Saving` modes are mutually exclusive).

**`tests/test_export.rs`** — 7 tests:

- `test_export_wav_creates_file` — mono 1s sine wave exported; hound reads back correct sample rate, channels, bit depth.
- `test_export_wav_sample_count` — verify sample count matches input length.
- `test_export_wav_stereo` — 2-channel export; channels and sample count correct.
- `test_export_wav_empty` — empty input produces valid WAV with 0 samples.
- `test_export_wav_clamps_samples` — values outside [-1, 1] are clamped to ±32767.
- `test_default_export_path_basic` — constructs `{stem}_processed.wav` in the source directory.
- `test_default_export_path_collision` — existing `_processed.wav` → falls back to `_processed_2.wav`.

### Modified Files (8)

**`Cargo.toml`** — Moved `hound = "3.5"` from `[dev-dependencies]` to `[dependencies]` (also retained in dev-dependencies for test usage).

**`src/audio/mod.rs`** — Added `pub mod export;`.

**`src/app.rs`** — Three additions:

1. `AppMode::Saving` variant — new modal mode for the save dialog.
2. `Action::ExportWav(String)` variant — carries the destination file path.
3. `FileInfo::path: String` field — stores the full source file path for constructing the default export path.

**`src/input/handler.rs`** — Four additions:

1. `use crate::audio::export;` import.
2. `AppMode::Saving => handle_save_dialog(key, app)` dispatch in `handle_key_event`.
3. `KeyCode::Char('s')` handler in `handle_normal` — if audio is loaded, pre-fills `file_picker_input` with `default_export_path()` and sets `AppMode::Saving`. If no audio, shows "No audio to export" in status bar.
4. `handle_save_dialog()` function — mirrors `handle_file_picker()`: Esc cancels, Enter returns `Action::ExportWav(path)`, Backspace/Char edit the path string.

**`src/main.rs`** — Two additions:

1. `file_info.path` populated in `load_file()` via `path.to_string_lossy()`.
2. `Action::ExportWav` handler:
   - Selects the correct source buffer based on A/B state (`original_audio` when listening to original, `audio_data` when listening to processed).
   - Clones the samples and bakes live gain via `apply_gain()` (gain is applied in the audio callback, not stored in the buffer).
   - Calls `export_wav()` and shows success/error in `status_message`.

**`src/ui/mod.rs`** — Added `pub mod save_dialog;`.

**`src/ui/layout.rs`** — Added save dialog overlay: renders `save_dialog::render()` when `app.mode == AppMode::Saving`.

## Key Design Decisions

### 1. 16-bit PCM Output

Standard 16-bit PCM WAV for maximum compatibility with audio tools (Audacity, aplay, media players). Samples are clamped to [-1.0, 1.0] before scaling. Symmetric range (±32767) avoids asymmetric clipping artifacts.

### 2. Export What the User Hears

The export respects A/B toggle state:
- **B (processed)**: exports `audio_data` (post-WORLD + post-effects + gain).
- **A (original)**: exports `original_audio` (mono downmix + gain).

In both cases, live gain is baked into the exported samples since it is not stored in the audio buffers (it's applied in the audio callback via `AtomicU32`).

### 3. Collision-Free Default Path

`default_export_path()` generates `{stem}_processed.wav` in the source file's directory. If that file exists, it tries `_processed_2.wav`, `_3.wav`, etc. The user can always edit the path before confirming.

### 4. Reuse `file_picker_input` Field

Both the file picker and save dialog use the same `file_picker_input` string in `AppState`. This is safe because `AppMode` is exclusive — only one modal can be active at a time. Both Esc and Enter paths clear the field. This avoids adding a redundant field to `AppState`.

### 5. Synchronous Export

The export runs synchronously on the main thread. For typical audio files (1-5 minutes at 44100 Hz mono), the write completes in <100ms. The UI freezes briefly but this is acceptable for the current use case. A background export could be added in a future polish pass if needed.

### 6. No Overwrite Confirmation

If the user edits the path to point at an existing file, it is silently overwritten. The default path avoids collisions, so this only happens with intentional user edits. Adding an "Overwrite?" confirmation would require another modal state — deferred to polish (P8).

## Architecture

```
User presses 's':
  handler → app.file_picker_input = default_export_path(file_info.path)
          → app.mode = Saving

Save dialog (modal overlay):
  Esc → cancel, clear input, Normal mode
  Char/Backspace → edit path
  Enter → Action::ExportWav(path)

Action::ExportWav handler (main loop):
  source = if ab_original { original_audio } else { audio_data }
  samples = source.samples.clone()
  apply_gain(&mut samples, gain_db)       ← bake live gain
  export_wav(&samples, sample_rate, channels, path)
  status_message = "Saved: {path}" or "Export error: {e}"
```

## Edge Cases Handled

| Scenario | Behavior |
|---|---|
| No file loaded | `'s'` shows "No audio to export" in status bar |
| A/B on original | Exports `original_audio` (mono downmix + gain) |
| A/B on processed | Exports `audio_data` (WORLD + effects + gain) |
| Gain at 0 dB | No `apply_gain` call (skip when `gain_db == 0.0`) |
| Gain at +12 dB | Samples >1.0 are clamped to 32767 in i16 conversion |
| Default path exists | Falls back to `_processed_2.wav`, `_3.wav`, etc. |
| Empty path (Enter on blank) | No action, returns to Normal mode |
| Esc during save dialog | Cancels without saving, clears input |
| Write error (read-only dir, etc.) | Error displayed in status bar via `ExportError` |
| Empty audio buffer | Valid WAV with 0 samples (hound handles this) |
| Samples outside [-1, 1] | Clamped to ±32767 before i16 conversion |

## Dependencies Introduced

- `hound = "3.5"` — moved from `[dev-dependencies]` to `[dependencies]`. Provides `WavWriter` for 16-bit PCM WAV output.

## Verification

- `cargo clippy --workspace` — zero warnings
- `cargo test` — 42/42 pass (4 decoder + 11 WORLD FFI + 3 modifier + 6 spectrum + 11 effects + 7 export)
- Manual checklist: load file, adjust sliders, press `s`, default path appears, Enter saves, file plays in aplay/Audacity, correct sample rate and channels, Esc cancels, `s` with no file shows message

## Test Count

42 tests: 4 decoder + 11 WORLD FFI + 3 modifier + 6 spectrum + 11 effects + 7 export

## Resolved Placeholders

- WAV export — fully implemented with save dialog, gain baking, A/B-aware source selection

## Remaining Placeholders for Future Phases

- `loop_enabled: bool` — wired to UI display but not yet to playback loop logic
- Polish, keybinds help overlay (P8)
