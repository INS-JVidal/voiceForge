# VoiceForge: File Loading Pipeline

## 1. Overview

The file loading pipeline is the entry point for processing audio in VoiceForge. It spans three concurrent threads:
- **Main thread**: ratatui TUI event loop, keyboard/mouse input, rendering (~30 fps)
- **Processing thread**: audio decoding, WORLD analysis, parameter synthesis
- **Audio callback thread**: cpal output stream writes samples to hardware

Two entry points trigger loading:
1. **CLI arguments** (`--file <path>`) — loads immediately on startup
2. **Interactive file picker** (`'o'` key) — user browses and selects a file

**Key invariant**: No blocking I/O on the main thread. File operations (decode, analysis) happen asynchronously in the processing thread while the TUI remains responsive.

---

## 2. Architecture

VoiceForge's file loading follows a 4-layer architecture:

```
┌─────────────────────────────────────────────────────────────┐
│ Presentation Layer                                          │
│ (ratatui TUI, keyboard input, file picker UI, status bar)  │
└──────────────────────┬──────────────────────────────────────┘
                       │ Action enum (user intent)
┌──────────────────────▼──────────────────────────────────────┐
│ Application Layer                                           │
│ (AppState, debounce, main event loop, pending_stream_init) │
└──────────────────────┬──────────────────────────────────────┘
                       │ ProcessingCommand enum
┌──────────────────────▼──────────────────────────────────────┐
│ Domain Layer (Processing Thread)                            │
│ (decode_file_with_progress, run_analyze, scan_directory)   │
│ (WorldParams cache, EffectsParams, modifier::apply)        │
└──────────────────────┬──────────────────────────────────────┘
                       │ ProcessingResult enum
┌──────────────────────▼──────────────────────────────────────┐
│ Infrastructure Layer                                        │
│ (symphonia decoder, WORLD C++ FFI, cpal audio output)      │
│ (filesystem directory scan, file magic byte validation)    │
└─────────────────────────────────────────────────────────────┘
```

Each layer communicates via enums and is decoupled from others.

---

## 3. File Picker Flow

### 3.1 Opening the Picker ('o' Key)

When the user presses `'o'` in Normal mode:

1. **Input handler** (`src/input/handler.rs`) returns `Action::ScanDirectory`
2. **Main thread** (`src/main.rs`) receives the action, sets `AppMode::FilePicker`
3. **Main thread** dispatches `ProcessingCommand::ScanDirectory(file_picker_input)` where `file_picker_input` is the current typed prefix (initially empty or the last file's directory)
4. **Processing thread** calls `scan_directory_entries(prefix)`, which:
   - Lists all files in the directory matching common audio extensions (`.wav`, `.flac`, `.ogg`, `.mp3`, `.m4a`, `.aiff`)
   - Returns sorted results
5. **Main thread** receives `ProcessingResult::DirectoryListing(prefix_echo, entries)`
   - **Stale-result guard**: Ignores the result if `prefix_echo != current file_picker_input` (user kept typing while scan was in flight)
   - Updates `file_picker_entries` for rendering

### 3.2 Directory Scanning Sequence Diagram

Each keystroke (typing a new character) re-dispatches `ScanDirectory`:

```
USER        MAIN THREAD         PROCESSING THREAD
 │               │                       │
 │ type 'S'      │                       │
 │──────────────►│ ScanDirectory("S")    │
 │               │──────────────────────►│
 │ type 'o'      │                       │ scan_dir...
 │──────────────►│ ScanDirectory("So") (replaces)
 │               │──────────────────────►│
 │ type 'u'      │                       │ scan_dir...
 │──────────────►│ ScanDirectory("Sou") (replaces)
 │               │──────────────────────►│
 │               │◄──DirectoryListing────│
 │               │ ("Sou", [entries])    │
```

In practice, older scans complete and are ignored because their `prefix_echo` doesn't match the current input.

### 3.3 Confirming a File (Enter Key)

When the user presses Enter in the file picker:

1. **Input handler** returns `Action::PrecheckAudio(path)` for the selected entry
2. **Main thread** dispatches `ProcessingCommand::PrecheckAudio(path)`
3. **Processing thread** reads the first 12 magic bytes of the file to validate it is a supported audio format (WAV, FLAC, OGG, MP3, M4A, AIFF)
4. **Processing thread** sends either:
   - `ProcessingResult::AudioPrecheckDone(path)` → file is valid
   - `ProcessingResult::AudioPrecheckFailed(path, error_msg)` → invalid or unreadable
5. **Main thread** on `AudioPrecheckDone`:
   - **Stale guard**: Ignores if `current_file_path` differs from the response path
   - Calls `prepare_for_load()` (see section 4.1)
   - Sets `AppMode::Normal` (picker closes immediately, optimistic UX)
   - Dispatches `ProcessingCommand::Load(path)` to start decoding
6. **Main thread** on `AudioPrecheckFailed`:
   - Sets a status message showing the error
   - Keeps the file picker open for retry

---

## 4. Load & Decode

### 4.1 prepare_for_load()

Called in the main thread just before dispatching `Load`, this function resets state:

- `processing_status = None` (clears any prior "Decoding..." or "Analyzing..." status)
- `status_message` = empty string
- `spectrum_bins = Vec::new()` (clear any prior spectrum visualization)
- `ab_original = Some(original_audio)` → stashed for A/B comparison
- `original_audio = None`

**Important**: Does **NOT** clear `audio_data`. This allows the previous file to keep playing during load, providing a smooth UX.

### 4.2 run_load_file() — Processing Thread

Once `ProcessingCommand::Load(path)` is received, the processing thread:

1. Sends `ProcessingResult::Status("Decoding...")` immediately
2. Calls `decode_file_with_progress()` (in `src/audio/decoder.rs`) which:
   - Uses symphonia to probe the file format
   - Finds the audio track
   - Creates a format-specific codec decoder
   - Decodes audio packets in a loop, sending progress updates at 25%, 50%, 75%, 100%
   - Converts samples to f32, maintains channel count and sample rate
   - Returns `AudioData { samples: Vec<f32>, sample_rate, channels }`
3. On success:
   - Sends `ProcessingResult::AudioReady(audio, path.clone())`
4. On decode error:
   - Sends `ProcessingResult::Status("Decode failed: ...")` with error details

### 4.3 Main Thread on AudioReady

On receiving `ProcessingResult::AudioReady(audio, loaded_path)`:

1. **Stale check**: Ignores if `current_file_path != loaded_path` (user loaded a different file while this was decoding)
2. Calls `build_file_info(loaded_path, &audio)` to populate:
   - `FileInfo { name, path, sample_rate, channels, original_channels, duration_secs, total_samples }`
3. Sets `app.audio_data = Some(audio)`
4. Sets `pending_stream_init = Some(audio)` — **deferred**, not executed immediately
5. Main loop continues; playback initialization happens at the top of the next iteration (see section 5)
6. **Immediately** dispatches `ProcessingCommand::Resynthesize(neutral_values, neutral_fx)` to start WORLD analysis

---

## 5. Playback Initialization

At the top of each main loop iteration, before rendering, the main thread checks `pending_stream_init`:

### Path A: First File Loaded

If `pending_stream_init.is_some()` and `playback_state.is_none()`:

1. Calls `start_playback(audio, device_config, ...)`
   - Queries cpal for default audio device and output configuration
   - Determines output channels (usually 2 for stereo)
   - Creates `Arc<RwLock<Arc<AudioData>>>` containing the audio
   - Spawns cpal output stream with `write_audio_data` callback
   - Initializes atomics: `playing = false`, `position = 0`, `live_gain = 1.0`
   - Calls `stream.play()`
   - Returns `PlaybackState { stream, audio_lock, playing, position, ... }`
2. Sets `playback_state = Some(state)`
3. Clears `pending_stream_init`

### Path B: Subsequent Files

If `pending_stream_init.is_some()` and `playback_state.is_some()`:

1. Calls `rebuild_stream(audio, existing_atomics, ...)`
   - Reuses `Arc<AtomicBool> playing`, `Arc<AtomicUsize> position`, `Arc<AtomicU32> live_gain`
   - Creates a new cpal stream with the same callback
   - Clamps playback position to valid range for the new audio length
   - Returns new `PlaybackState` with updated `audio_lock`
2. Replaces `playback_state` with the new one
3. Clears `pending_stream_init`

### Audio Callback: write_audio_data

The cpal output stream runs the callback in a dedicated audio thread:

```
write_audio_data(output_buffer, cpal_callback_info):
  - Try to read the current AudioData from the RwLock (non-blocking)
    - If write lock held (during swap), output silence for this period only
  - Read playback position and playing flag atomically
  - For each frame:
    - If playing: read sample at current position
    - Apply live_gain (per-sample multiplication)
    - Upmix if needed: mono→stereo by copying to both channels
      (src_ch = device_ch % audio_channels)
    - Increment position atomically
  - If position >= audio length: set playing = false
```

This callback is latency-sensitive (runs every ~5 ms at 48 kHz, 256-frame buffer). It never blocks on I/O.

---

## 6. WORLD Analysis

### 6.1 run_analyze() — Processing Thread

Immediately after sending `AudioReady`, the processing thread:

1. Calls `world::to_mono_f64()` to convert the decoded f32 interleaved audio to f64 mono:
   - Average all channels into a single mono signal
   - Convert f32 samples to f64
2. Sends `ProcessingResult::Status("Analyzing...")`
3. Calls `world_sys::analyze_with_progress()` (FFI to WORLD C++ library):
   - **DIO**: Extracts fundamental frequency (f0) contour via spectral analysis
   - **StoneMask**: Refines f0 estimates
   - **CheapTrick**: Extracts spectral envelope (sp)
   - **D4C**: Extracts aperiodicity (ap) — voicing confidence per bin
   - Returns `WorldParams { f0, temporal_positions, spectrogram, aperiodicity, fft_size, frame_period }`
   - Sends progress at 25%, 50%, 75%, 100% (displayed in status bar)
4. Caches `WorldParams` in the processing thread (never sent across channel boundary)
5. Calls `world::to_mono()` on the original audio again to cache `original_mono` for the neutral-slider shortcut
6. Sends `ProcessingResult::AnalysisDone(original_mono)`

### 6.2 Main Thread on AnalysisDone

On receiving `ProcessingResult::AnalysisDone(mono)`:

1. Sets `processing_status = None` (clears "Analyzing..." status bar message)
2. Sets `original_audio = Some(mono)` (enables A/B toggle; user can now press `'a'` to compare original vs. processed)
3. **Immediately** sends `ProcessingCommand::Resynthesize(values, fx)` with current slider values

This triggers the WORLD synthesis and effects chain (see **EFFECTS_PIPELINE.md** § 6–9 for details), resulting in an initial output with all current slider settings applied.

---

## 7. Full Loading Sequence Diagram (Happy Path)

```
USER        MAIN THREAD       PROCESSING THREAD      AUDIO HW
 │               │                      │                  │
 │ 'o' key       │                      │                  │
 │──────────────►│ ScanDirectory        │                  │
 │ (type path)   │─────────────────────►│ scan_dir         │
 │               │◄─DirectoryListing────│                  │
 │ Enter         │                      │                  │
 │──────────────►│ PrecheckAudio        │                  │
 │               │─────────────────────►│ read 12 bytes    │
 │               │◄─AudioPrecheckDone───│                  │
 │               │ prepare_for_load()   │                  │
 │               │─── Load(path) ──────►│                  │
 │               │◄─Status("Decoding")──│                  │
 │               │◄─Status("…25%")──────│                  │
 │               │◄─Status("…50%")──────│ (symphonia)      │
 │               │◄─AudioReady──────────│                  │
 │               │ pending_stream_init  │                  │
 │               │ Resynthesize         │                  │
 │               │─────────────────────►│                  │
 │               │◄─Status("Analyzing")─│                  │
 │               │◄─AnalysisDone────────│ (WORLD FFI)      │
 │               │ original_audio set   │                  │
 │               │ Resynthesize         │                  │
 │               │─────────────────────►│                  │
 │               │◄─SynthesisDone───────│ (synthesis+fx)   │
 │               │ swap_audio()         │                  │
 │               │ (next loop top)      │                  │
 │               │ start_playback()     │                  │
 │               │──────────────────────────────────────► │
 │               │    cpal stream.play() & callback start │
 │ (hears audio) │◄──────────────────────────────────────│
 │               │                      │                  │
```

The diagram shows the happy path. Error cases (precheckfail, decode failure, synthesis error) are handled by status messages and do not advance the pipeline.

---

## 8. Key Data Types (Reference)

### AudioData
```rust
pub struct AudioData {
    pub samples: Vec<f32>,   // Interleaved PCM (L, R, L, R, ... for stereo)
    pub sample_rate: u32,    // Hz (typically 44100 or 48000)
    pub channels: u16,       // 1=mono, 2=stereo (WORLD always produces 1)
}
```

### WorldParams (C++ struct via FFI, cached in processing thread)
```rust
pub struct WorldParams {
    pub f0: Vec<f64>,                        // Fundamental frequency per frame
    pub temporal_positions: Vec<f64>,        // Time in seconds per frame
    pub spectrogram: Vec<Vec<f64>>,         // Spectral envelope (2D)
    pub aperiodicity: Vec<Vec<f64>>,        // Voicing confidence (2D)
    pub fft_size: usize,
    pub frame_period: f64,                   // ms per frame
}
```

### FileInfo
```rust
pub struct FileInfo {
    pub name: String,              // Filename only
    pub path: String,              // Full path
    pub sample_rate: u32,
    pub channels: u16,             // Current (after processing)
    pub original_channels: u16,    // From decoded file (preserved for display)
    pub duration_secs: f64,
    pub total_samples: usize,
}
```

### ProcessingCommand Enum
```rust
pub enum ProcessingCommand {
    Load(String),                   // path to decode
    ScanDirectory(String),          // path prefix as typed
    PrecheckAudio(String),          // path to validate
    Resynthesize(WorldSliderValues, EffectsParams),
    ReapplyEffects(EffectsParams),
    Shutdown,
}
```

### ProcessingResult Enum
```rust
pub enum ProcessingResult {
    AudioReady(AudioData, String),           // decoded + source path
    AnalysisDone(AudioData),                 // original mono (for A/B)
    SynthesisDone(AudioData),                // processed audio
    Status(String),                          // status message (display in bar)
    DirectoryListing(String, Vec<String>),   // (input_echo, sorted paths)
    AudioPrecheckDone(String),               // path is valid
    AudioPrecheckFailed(String, String),     // (path, error msg)
}
```

### Action Enum (Input Layer)
```rust
pub enum Action {
    Quit,
    ScanDirectory,
    PrecheckAudio(String),
    Resynthesize,
    ReapplyEffects,
    LiveGain(f32),             // Pre-computed linear multiplier
    ToggleAB,
    ExportWav(String),
}
```

---

## 9. Source File References

- **`src/input/handler.rs`** — Key bindings, file picker mode, Enter/Escape handling
- **`src/app.rs`** — `AppState`, `prepare_for_load()`, `Action` enum, `FileInfo`, `SliderDef`
- **`src/main.rs`** — Event loop, dispatch logic, `pending_stream_init`, result drain
- **`src/dsp/processing.rs`** — `ProcessingCommand` / `ProcessingResult` enums, `handle_command()`, `run_load_file()`, `run_analyze()`, `scan_directory_entries()`, `precheck_audio_file()`
- **`src/audio/decoder.rs`** — `decode_file_with_progress()` via symphonia FFI
- **`src/audio/playback.rs`** — `start_playback()`, `rebuild_stream()`, `write_audio_data()` callback, `PlaybackState`

---

## 10. Known Invariants

1. **No blocking I/O on main thread** — All file operations happen in the processing thread
2. **Stale-result guards** — Main thread ignores results if paths or prefixes don't match current state
3. **Playback continues during load** — `audio_data` is not cleared before new audio arrives
4. **WORLD analysis always produces mono** — Spectral sliders always output a single channel
5. **Position scaling on speed change** — When the pitch shift or speed slider changes output buffer length, playback position is scaled proportionally to maintain sync
