# P7 — Export Processed Audio to WAV

## Goal
Allow the user to save the currently processed audio (with all WORLD modifications and effects applied) to a WAV file by pressing `S`.

## Prerequisite
P6 complete (full processing pipeline with effects).

## Steps

### 7.1 Dependency (already in Cargo.toml from plan)
```toml
hound = "3.5"
```

### 7.2 Export module — `src/audio/export.rs`
```rust
pub fn export_wav(
    samples: &[f32],
    sample_rate: u32,
    channels: u16,
    path: &Path,
) -> Result<(), ExportError> { ... }
```
- Use `hound::WavWriter` to write f32 samples as 16-bit PCM WAV (or 32-bit float WAV)
- Set correct sample rate and channel count from the source file
- Handle file creation errors gracefully

### 7.3 Export UI flow
When user presses `S`:
1. Switch to a "Save" mode in AppState
2. Show a text input in the status bar or a modal: pre-fill with a suggested filename based on the source file (e.g., `song_processed.wav`)
3. User can edit the path and press Enter to confirm, or Esc to cancel
4. On confirm:
   - Read the current `processed_pcm` buffer (with effects applied)
   - Call `export_wav()` with the buffer
   - Show "Saved to: /path/to/output.wav" in status bar for a few seconds
   - Return to Normal mode
5. On error: show error message in status bar

### 7.4 Default output path
- Same directory as source file
- Filename: `{original_stem}_processed.wav`
- If file exists, append a number: `_processed_2.wav`, etc.

### 7.5 Edge cases
- If no file is loaded, `S` does nothing (or shows "No file loaded")
- If processing is in progress, wait or show "Processing not complete"
- If the processed buffer is empty (analysis not done yet), show appropriate message

## Human Test Checklist

- [ ] Load a file, adjust some sliders, press `S`
- [ ] Filename input appears with a sensible default path
- [ ] Press Enter → file is saved; confirmation message appears in status bar
- [ ] Open the saved WAV in another player (e.g., `aplay`, Audacity) → audio matches what was heard in VoiceForge
- [ ] The saved file has correct sample rate and channel count
- [ ] Press Esc during save dialog → cancels without saving
- [ ] Press `S` with no file loaded → shows "No file loaded" message
- [ ] Save to a read-only directory → shows error message, doesn't crash
- [ ] Save twice to same name → second file gets `_2` suffix (no overwrite)

## Dependencies Introduced
- `hound` 3.5

## Notes
- This is a small but important phase — without export, the tool is preview-only.
- 16-bit PCM output is standard and widely compatible. 32-bit float could be offered as an option later.
