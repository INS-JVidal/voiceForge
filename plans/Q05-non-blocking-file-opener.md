# Q05 — Non-Blocking File Opener (SOLID Layered Refactor)

**Phase:** P5-P6 Expansion (Post-Smooth Slider Rendering)
**Scope:** Eliminate blocking I/O from UI thread; consolidate file loading logic
**Status:** Ready for implementation

---

## Overview

Streamline the file loading workflow by moving all blocking I/O operations (directory scanning, audio format validation, file decoding) out of the UI thread and into the processing thread. The refactor establishes a clean **SOLID-compliant layered architecture** where each layer has a single responsibility:

1. **Presentation layer (handler.rs):** Event → Action only; zero I/O
2. **Application layer (main.rs):** Action → Command dispatch; result receive
3. **Domain layer (processing.rs):** All blocking I/O lives here
4. **Infrastructure (decoder.rs):** Audio file decoding

**Current pain points:**
- File picker input (keystroke) → `fs::read_dir` + `metadata()` blocking on every character
- Enter key → `File::open` + 12-byte read (blocking)
- CLI args → Synchronous `decode_file` before event loop (blank terminal during decode)
- 4× code duplication of decode→AudioReady→analyze block

**Target:**
- UI thread never blocks on I/O (responsive file picker typing)
- CLI loads non-blocking (terminal renders "Loading..." while decoding)
- Single `run_load_file` helper eliminates DRY
- Stale-result guards prevent race conditions if user switches files mid-precheck

---

## Design Goals

1. **Responsiveness**
   - File picker input character typing → zero delay (no fs I/O)
   - Directory listing populated asynchronously (via processing thread result)
   - CLI path loads without blanking terminal

2. **Code Quality (SOLID)**
   - **Single Responsibility:** Each module owns its domain (I/O, UI, app logic)
   - **Dependency Injection:** Commands/results flow through channels
   - **Interface Segregation:** Minimal action types, clear command/result enums
   - **DRY:** `run_load_file` consolidates repeated decode logic
   - **Testability:** Async I/O decoupled from UI events

3. **Thread Safety**
   - Stale-result guards check `awaiting_load_path` before processing precheck results
   - Discard `DirectoryListing` if input changed since dispatch
   - No TOCTOU races (time-of-check-time-of-use)

4. **User Experience**
   - Optimistic UI close: picker closes immediately on Enter, error shown if precheck fails
   - Progress feedback: "Checking..." → "Decoding... X%" → "Analyzing..."
   - Consistent behavior: file picker, raw input, CLI all dispatch same commands

---

## Architecture

### Data Flow

```
┌───────────────────────────────────────────────────────────────┐
│ Presentation   │ handler.rs
│                │ - handle_key_event(KeyEvent) → Option<Action>
│                │ - Returns Action::ScanDirectory, PrecheckAudio
│                │ - ZERO I/O operations
└───────────────────────────────────────────────────────────────┘
                               ↓
┌───────────────────────────────────────────────────────────────┐
│ Application    │ main.rs
│                │ - Action dispatch: send ProcessingCommand
│                │ - Result receive: update AppState + UI state
│                │ - Debounce + stream initialization logic
└───────────────────────────────────────────────────────────────┘
                               ↓ (channel)
┌───────────────────────────────────────────────────────────────┐
│ Domain         │ processing.rs
│                │ - handle_command(ProcessingCommand) → loop
│                │ - run_load_file() — decode + analyze
│                │ - scan_directory_entries() — fs::read_dir + filter
│                │ - precheck_audio_file() — File open + magic bytes
│                │ - ALL blocking I/O here
└───────────────────────────────────────────────────────────────┘
                               ↓ (channel)
┌───────────────────────────────────────────────────────────────┐
│ Infrastructure │ decoder.rs
│                │ - decode_file(path) → Result<AudioData>
│                │ - decode_file_with_progress(path, callback)
└───────────────────────────────────────────────────────────────┘
```

### Action Enum (app.rs)

```rust
pub enum Action {
    Quit,
    ScanDirectory,                 // Main thread: dispatch ScanDirectory cmd
    PrecheckAudio(String),         // Main thread: dispatch PrecheckAudio cmd
    Resynthesize,
    ReapplyEffects,
    LiveGain(f32),
    ToggleAB,
    ExportWav(String),
}
```

### ProcessingCommand Enum (processing.rs)

```rust
pub enum ProcessingCommand {
    Load(String),                  // path to decode
    ScanDirectory(String),         // path prefix typed by user
    PrecheckAudio(String),         // path to validate magic bytes
    Resynthesize(WorldSliderValues, EffectsParams),
    ReapplyEffects(EffectsParams),
    Shutdown,
}
```

### ProcessingResult Enum (processing.rs)

```rust
pub enum ProcessingResult {
    AudioReady(AudioData, String),                    // decoded audio + path
    AnalysisDone(AudioData),
    SynthesisDone(AudioData),
    Status(String),
    DirectoryListing(String, Vec<String>),            // (prefix echo, entries)
    AudioPrecheckDone(String),                        // path valid
    AudioPrecheckFailed(String, String),              // (path, error msg)
}
```

### AppState Fields (app.rs)

```rust
pub struct AppState {
    // ... existing fields ...
    pub awaiting_load_path: Option<String>,  // stale-result guard
}

impl AppState {
    /// Reset all transient state for loading a new file.
    pub fn prepare_for_load(&mut self) {
        self.processing_status = Some("Loading file...".to_string());
        self.status_message = None;
        self.status_message_time = None;
        self.spectrum_bins.clear();
        self.ab_original = false;
        self.original_audio = None;
        self.awaiting_load_path = None;
    }
}
```

---

## Implementation Changes

### 1. Handler (src/input/handler.rs)

**Remove:** `update_file_picker_matches()`, `precheck_audio_file()`

**Change:** Text input and file picker navigation dispatch `Action::ScanDirectory`

| Event | Old Behavior | New Behavior |
|-------|--------------|--------------|
| Keystroke in file picker | `fs::read_dir` + `metadata()` blocking | Return `Action::ScanDirectory` |
| Tab on match | `update_file_picker_matches()` blocking | Return `Action::ScanDirectory` |
| Enter on file | `precheck_audio_file()` blocking | Close picker, return `Action::PrecheckAudio(path)` |
| Enter on raw input | `precheck_audio_file()` blocking | Close picker, return `Action::PrecheckAudio(path)` |
| Open file picker ('o') | `update_file_picker_matches()` blocking | Return `Action::ScanDirectory` |

---

### 2. Processing Thread (src/dsp/processing.rs)

**Add:**
- `run_load_file()` helper: decode + analyze (consolidates 4× DRY)
- `scan_directory_entries()`: moved from handler, fs I/O
- `precheck_audio_file()`: moved from handler, file magic bytes

**Modify:**
- `ProcessingCommand::Load` arm: call `run_load_file()`
- Top-level `handle_command()`: add arms for `ScanDirectory`, `PrecheckAudio`
- All three drain loops: add inline handling for new commands

**Remove:**
- `ProcessingCommand::Analyze` variant (only dispatched from old CLI sync path)

---

### 3. Main Event Loop (src/main.rs)

**Remove:**
- `load_file()` function (no longer needed; all I/O now in processing thread)

**Change:**
- CLI args: dispatch `ProcessingCommand::Load` directly (non-blocking)
- Action dispatch: add `ScanDirectory` and `PrecheckAudio` handlers
- Result receive: add handlers for `DirectoryListing`, `AudioPrecheckDone`, `AudioPrecheckFailed`

**Dispatch Logic:**
```rust
Action::ScanDirectory => {
    processing.send(ProcessingCommand::ScanDirectory(app.file_picker_input.clone()));
}
Action::PrecheckAudio(path) => {
    app.processing_status = Some("Checking...".to_string());
    app.awaiting_load_path = Some(path.clone());
    processing.send(ProcessingCommand::PrecheckAudio(path));
}
```

**Result Logic:**
```rust
ProcessingResult::DirectoryListing(prefix, entries) => {
    // Discard if input changed since dispatch
    if prefix == app.file_picker_input {
        app.file_picker_matches = entries;
        app.file_picker_scroll = 0;
        // clamp selection
    }
}
ProcessingResult::AudioPrecheckDone(path) => {
    // Discard if user switched paths
    if app.awaiting_load_path.as_deref() == Some(path.as_str()) {
        app.prepare_for_load();
        current_file_path = Some(path);
        processing.send(ProcessingCommand::Load(path));
    }
}
ProcessingResult::AudioPrecheckFailed(path, msg) => {
    // Discard if user switched paths
    if app.awaiting_load_path.as_deref() == Some(path.as_str()) {
        app.awaiting_load_path = None;
        app.set_status(format!("Error: {msg}"));
    }
}
```

---

## Verification Checklist

- [ ] `cargo check` — zero errors
- [ ] `cargo clippy --all-targets -- -D warnings` — zero warnings
- [ ] `cargo test --all-targets` — all 62 tests pass
- [ ] File picker typing responsive (no UI freeze)
- [ ] Enter on file closes picker, shows "Checking...", then "Decoding..." and "Analyzing..."
- [ ] Enter on non-audio file shows error without UI freeze
- [ ] `cargo run -- path/to/file.wav` — terminal renders "Loading..." immediately
- [ ] Slider changes during file picker still work (resynth debounce honored)
- [ ] Status messages for precheck errors appear on screen

---

## Benefits

1. **Responsive UI:** No blocking on any keystroke (file picker, typing, Enter)
2. **Clean Architecture:** SOLID-compliant layered design with clear responsibilities
3. **Code Quality:** DRY consolidation reduces 80+ lines of duplication
4. **Non-blocking CLI:** Terminal doesn't blank while decoding CLI argument file
5. **Thread Safety:** Stale-result guards prevent race conditions
6. **Maintainability:** I/O isolated in one place; UI and app logic decoupled

---

## Testing Strategy

**Unit Tests:** No new unit tests needed; existing 62 tests cover all logic.

**Integration Tests:** Manual verification of user flows:
1. File picker → keystroke → list updates within 100ms
2. File picker → Enter valid file → loads without error
3. File picker → Enter invalid file → error shown without freeze
4. CLI → `cargo run -- audio.wav` → terminal renders immediately
5. Slider change during file load → doesn't interrupt load

---

## Timeline

**Expected Duration:** 1–2 hours
- Plan review: 10 min
- Implementation: 60 min
- Testing: 20 min
- Documentation: 10 min

