# VoiceForge — Voice Modulation TUI in Rust

## 1. Vision

A terminal-based voice modulation workbench built with Rust and ratatui. Load a WAV/MP3 file, decompose it using the WORLD vocoder algorithm, manipulate pitch, timbre, breathiness, and speed via sliders, preview changes in real time with A/B comparison, and visualize the spectrum — all from the terminal.

```
┌─ VoiceForge ──────────────────────────────────────────────────────────────────┐
│                                                                               │
│  ┌─ WORLD Vocoder ─────────────┐  ┌─ Effects ──────────────────────────────┐  │
│  │ Pitch Shift   ──────●────── │  │ Gain (dB)     ─────────●──────────── │  │
│  │ Pitch Range   ────●──────── │  │ Low Cut (Hz)  ──●──────────────────── │  │
│  │ Speed         ──────●────── │  │ High Cut (Hz) ────────────────●────── │  │
│  │ Breathiness   ────●──────── │  │ Compressor    ───────●─────────────── │  │
│  │ Formant Shift ──────●────── │  │ Reverb Mix    ──●──────────────────── │  │
│  │ Spectral Tilt ───────●───── │  │ Pitch (FX)    ──────●──────────────── │  │
│  └─────────────────────────────┘  └───────────────────────────────────────┘  │
│                                                                               │
│  ┌─ Spectrum ─────────────────────────────────────────────────────────────┐   │
│  │  ▁▂▃▅▇█▇▅▃▂▁▁▂▃▄▅▆▇█▇▆▅▄▃▂▁▁▁▂▃▄▅▆▅▄▃▂▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁▁  │   │
│  │  20Hz            200Hz           1kHz          5kHz          20kHz     │   │
│  └───────────────────────────────────────────────────────────────────────┘   │
│                                                                               │
│  ┌─ Transport ──────────────────────────────────────────────────────────┐    │
│  │  [▶ Play] [⏸ Pause] [↺ Repeat]  ──────────●────────── 1:23 / 3:45   │    │
│  │  [A/B: Original ● / Processed ○]           Seek bar                  │    │
│  └──────────────────────────────────────────────────────────────────────┘    │
│                                                                               │
│  File: speech_sample.wav  │  44100 Hz  │  Mono  │  Processing: 12ms          │
└───────────────────────────────────────────────────────────────────────────────┘
```

---

## 2. Architecture

### 2.1 High-Level Data Flow

```
 WAV/MP3 File
     │
     ▼
 ┌──────────────┐
 │   Decoder     │  symphonia (MP3/WAV/OGG/FLAC → f32 PCM)
 └──────┬───────┘
        │  f32 audio buffer
        ▼
 ┌──────────────┐
 │ WORLD Vocoder │  world-ffi (C++ WORLD lib via FFI)
 │   Analysis    │  Extracts: f0, sp, ap
 └──────┬───────┘
        │  f0 (pitch), sp (spectral), ap (aperiodicity) arrays
        ▼
 ┌──────────────┐
 │  Modifier     │  Pure Rust — numpy-like ops via ndarray
 │  (Sliders)    │  Applies: pitch shift, formant warp, breathiness, speed, tilt
 └──────┬───────┘
        │  Modified f0', sp', ap'
        ▼
 ┌──────────────┐
 │ WORLD Vocoder │  Resynthesizes modified parameters → f32 PCM
 │  Synthesis    │
 └──────┬───────┘
        │  f32 processed audio
        ▼
 ┌──────────────┐
 │ Effects Chain │  fundsp: gain → highpass → lowpass → limiter → reverb
 │  + Pitch FX   │  pitch_shift: phase vocoder (time-preserving)
 └──────┬───────┘
        │  f32 effected audio
        ├──────────────────────────┐
        ▼                          ▼
 ┌──────────────┐          ┌──────────────┐
 │    FFT        │          │   Playback    │  cpal → system audio output
 │  Spectrum     │          │   Engine      │  A/B switch between original/processed
 └──────┬───────┘          └──────────────┘
        │
        ▼
 ┌──────────────┐
 │   ratatui     │  TUI rendering: sliders, spectrum chart, transport controls
 │   UI Layer    │
 └──────────────┘
```

### 2.2 Thread Architecture

```
 ┌─────────────────────────────────────────────────────┐
 │  Main Thread                                        │
 │  • ratatui event loop (crossterm backend)           │
 │  • Handles keyboard/mouse input                     │
 │  • Reads shared state for rendering                 │
 └──────────────┬──────────────────────────────────────┘
                │ channels (crossbeam)
                ▼
 ┌──────────────────────┐    ┌──────────────────────────┐
 │  Audio Thread         │    │  Processing Thread       │
 │  • cpal output stream │    │  • WORLD analysis        │
 │  • Pulls from ring    │    │  • Modifier pipeline     │
 │    buffer             │    │  • WORLD resynthesis     │
 │  • A/B source switch  │    │  • Effects (fundsp +     │
 │  • Seek / loop logic  │    │    pitch_shift)          │
 └──────────────────────┘    │  • Writes to ring buffer │
                              │  • FFT for spectrum      │
                              └──────────────────────────┘
```

**Synchronization:**
- `Arc<AtomicBool>` for play/pause, A/B toggle, loop flag
- `Arc<Mutex<SliderState>>` for parameter values (read by processing thread)
- `ringbuf` crate for lock-free audio buffer between processing → playback
- `crossbeam-channel` for UI commands → processing thread

---

## 3. Core Dependencies (Cargo.toml)

| Crate | Version | Purpose | Notes |
|---|---|---|---|
| **ratatui** | 0.30 | Terminal UI framework | Sliders, charts, layout. Major restructure Dec 2025 — crossterm re-exported, no separate crossterm dep needed |
| **cpal** | 0.17 | Cross-platform audio output | Low-latency playback via PortAudio/ALSA/WASAPI |
| **symphonia** | 0.5 | Audio decoding | MP3, WAV, FLAC, OGG — pure Rust |
| **rustfft** | 6.4 | FFT computation | Real-time spectrum analysis |
| **ndarray** | 0.17 | N-dimensional arrays | Manipulate f0/sp/ap (numpy equivalent) |
| **ringbuf** | 0.4 | Lock-free ring buffer | Audio thread ↔ processing thread |
| **crossbeam-channel** | 0.5 | MPMC channels | UI commands → audio/processing |
| **hound** | 3.5 | WAV export | Save processed audio |
| **fundsp** | 0.23 | DSP effects library | Reverb, limiter, filters, gain — MIT/Apache-2.0. Composable graph notation, SIMD-accelerated |
| **pitch_shift** | 1 | Phase vocoder pitch shift | Time-preserving pitch shift by semitones — MIT. Tiny, no heavy deps |
| **cc** | — | C/C++ build integration | Compile WORLD vocoder C++ source |

**Note on crossterm:** As of ratatui 0.30, crossterm is re-exported via `ratatui::crossterm`. No separate `crossterm` dependency is needed in Cargo.toml — ratatui manages the version internally.

### 3.1 WORLD Vocoder FFI Strategy

There is no pure-Rust WORLD vocoder. The approach:

1. Vendor the C++ WORLD source (MIT licensed) from `github.com/mmorise/World`
2. Write a `build.rs` that compiles the C++ sources via the `cc` crate
3. Create thin `unsafe` FFI bindings in a `world-sys` subcrate
4. Wrap in safe Rust API exposing: `analyze(audio, fs) → (f0, sp, ap)` and `synthesize(f0, sp, ap, fs) → audio`

```
world-sys/
├── build.rs          # cc crate compiles World C++ sources
├── world-src/        # Vendored C++ from mmorise/World (MIT license)
│   ├── d4c.cpp
│   ├── dio.cpp
│   ├── cheaptrick.cpp
│   ├── stonemask.cpp
│   ├── synthesis.cpp
│   └── harvest.cpp
├── src/
│   ├── lib.rs        # unsafe extern "C" declarations
│   └── safe.rs       # Safe Rust wrapper: WorldAnalysis, WorldSynthesis
└── Cargo.toml
```

---

## 4. Module Architecture

```
voiceforge/
├── Cargo.toml
├── Cargo.lock
├── LICENSE                     # GPL-3.0
├── README.md
├── build.rs                    # (if not using workspace for world-sys)
│
├── crates/
│   └── world-sys/              # FFI bindings to C++ WORLD vocoder
│       ├── build.rs
│       ├── Cargo.toml
│       ├── world-src/          # Vendored C++ (MIT)
│       └── src/
│           ├── lib.rs          # Raw FFI
│           └── safe.rs         # Safe Rust API
│
├── src/
│   ├── main.rs                 # Entry point: init terminal, run app
│   │
│   ├── app.rs                  # AppState: all shared state, mode, file info
│   │
│   ├── audio/
│   │   ├── mod.rs
│   │   ├── decoder.rs          # symphonia: load MP3/WAV → f32 PCM buffer
│   │   ├── playback.rs         # cpal: output stream, play/pause/seek/loop
│   │   ├── buffer.rs           # Ring buffer management, A/B source switching
│   │   └── export.rs           # hound: save processed audio to WAV
│   │
│   ├── dsp/
│   │   ├── mod.rs
│   │   ├── world.rs            # High-level WORLD interface: analyze(), synthesize()
│   │   ├── modifier.rs         # Apply slider params to f0/sp/ap arrays
│   │   ├── effects.rs          # Post-processing: fundsp chain (gain, filters, limiter, reverb) + pitch_shift phase vocoder
│   │   └── spectrum.rs         # rustfft: compute magnitude spectrum for display
│   │
│   ├── ui/
│   │   ├── mod.rs
│   │   ├── layout.rs           # ratatui layout: panels, grid, proportions
│   │   ├── slider.rs           # Custom slider widget (horizontal, labeled, value)
│   │   ├── spectrum.rs         # Spectrum bar chart / sparkline widget
│   │   ├── transport.rs        # Play/Pause/Repeat/Seek/A-B toggle widget
│   │   ├── file_picker.rs      # Simple file browser or path input
│   │   └── status_bar.rs       # File info, sample rate, processing latency
│   │
│   └── input/
│       ├── mod.rs
│       └── handler.rs          # Keyboard/mouse → AppState mutations + commands
│
├── assets/
│   └── test_samples/           # Example WAV/MP3 files for development
│
└── tests/
    ├── test_world_ffi.rs       # Roundtrip: analyze → synthesize ≈ original
    ├── test_decoder.rs         # Load various formats
    ├── test_modifier.rs        # Parameter application correctness
    └── test_spectrum.rs        # FFT output sanity checks
```

---

## 5. Slider Parameters

### 5.1 WORLD Vocoder Sliders (Left Panel)

| Slider | Range | Default | Unit | Operation |
|---|---|---|---|---|
| Pitch Shift | -12.0 … +12.0 | 0.0 | semitones | `f0 *= 2^(val/12)` |
| Pitch Range | 0.2 … 3.0 | 1.0 | × | Scale f0 around mean |
| Speed | 0.5 … 2.0 | 1.0 | × | Resample f0/sp/ap time axis |
| Breathiness | 0.0 … 3.0 | 1.0 | × | Scale aperiodicity array |
| Formant Shift | -5.0 … +5.0 | 0.0 | semitones | Warp sp frequency axis |
| Spectral Tilt | -6.0 … +6.0 | 0.0 | dB/oct | Slope across sp bins |

### 5.2 Effects Sliders (Right Panel)

| Slider | Range | Default | Unit | Implementation |
|---|---|---|---|---|
| Gain | -12.0 … +12.0 | 0.0 | dB | fundsp `gain_db(val)` |
| Low Cut | 20 … 500 | 20 | Hz | fundsp `highpass_hz(freq, 0.7)` |
| High Cut | 2000 … 20000 | 20000 | Hz | fundsp `lowpass_hz(freq, 0.7)` |
| Compressor Thresh | -40 … 0 | 0 | dB | fundsp `limiter((attack, release))` + `gain_db()` makeup. For full ratio control: ~30-line envelope follower using fundsp `follow()` |
| Reverb Mix | 0.0 … 1.0 | 0.0 | wet/dry | fundsp `reverb_stereo(room_size, time, damping)` — 32-channel FDN algorithm |
| Pitch Shift (FX) | -12.0 … +12.0 | 0.0 | semitones | `pitch_shift` crate — phase vocoder, time-preserving. Factor: `2.0_f32.powf(val / 12.0)` |

**Note on two pitch shift controls:** The WORLD Pitch Shift (left panel) modifies f0 directly during resynthesis — it preserves formants and sounds natural for voice. The Effects Pitch Shift (right panel) is a post-processing phase vocoder applied to the final PCM buffer — it shifts everything including formants, producing a distinct "chipmunk" or "giant" effect at extreme values. Both are useful; they serve different creative purposes.

### 5.3 Analysis Settings (Advanced, togglable panel)

| Setting | Options | Default |
|---|---|---|
| Pitch Algorithm | DIO / Harvest | DIO |
| f0 Floor | 50 … 200 Hz | 71 |
| f0 Ceil | 400 … 1200 Hz | 800 |
| Frame Period | 1.0 … 10.0 ms | 5.0 |

---

## 6. Key Interactions

| Key | Action |
|---|---|
| `Space` | Play / Pause |
| `R` | Toggle repeat/loop |
| `Tab` | Switch A/B (original ↔ processed) |
| `↑/↓` | Navigate sliders |
| `←/→` | Adjust selected slider value |
| `Shift+←/→` | Fine adjustment (0.1× step) |
| `[/]` | Seek backward/forward 5s |
| `S` | Save processed audio to WAV |
| `O` | Open file (path input) |
| `A` | Toggle advanced settings panel |
| `Q / Esc` | Quit |

---

## 7. Processing Strategy

### 7.1 Offline Pre-computation

WORLD analysis is **not real-time** for long files. Strategy:

1. **On file load**: Run full WORLD analysis in processing thread (~2-5s for 1 min audio). Show progress bar.
2. **Store** f0, sp, ap as `ndarray` in `AppState`.
3. **On slider change**: Apply modifier pipeline to cached f0/sp/ap → resynthesize. For short files (<30s) this is near-instant. For longer files, resynthesize in chunks with a progress indicator.
4. **Cache** the processed audio buffer. Only resynthesize when sliders change.

### 7.2 Spectrum Display

- Tap into the **playback buffer** (whichever source is active: original or processed)
- Run `rustfft` on the current playback window (1024 or 2048 samples)
- Convert to dB magnitude: `20 * log10(|FFT|)`
- Map to ratatui `BarChart` or custom sparkline widget
- Update at ~20-30 FPS (every other ratatui frame)

### 7.3 A/B Comparison

Two buffers always in memory:
- `original_pcm: Vec<f32>` — decoded source audio
- `processed_pcm: Vec<f32>` — latest resynthesized audio

`Tab` flips an `AtomicBool`. The playback thread reads from the active buffer. Seek position is shared so A/B switching is seamless at the same timestamp.

---

## 8. Build & Run

### 8.1 System Dependencies

```bash
# Ubuntu / WSL2
sudo apt install build-essential cmake libasound2-dev pkg-config

# WORLD vocoder C++ source (vendored, no system install needed)
```

### 8.2 Initial Setup

```bash
# Create project
cargo init voiceforge
cd voiceforge

# Create workspace
mkdir -p crates/world-sys/world-src
mkdir -p src/{audio,dsp,ui,input}
mkdir -p assets/test_samples tests

# Clone WORLD C++ source (MIT license) into vendor directory
git clone https://github.com/mmorise/World.git /tmp/world
cp /tmp/world/src/*.cpp crates/world-sys/world-src/
cp /tmp/world/src/*.h   crates/world-sys/world-src/
cp /tmp/world/src/world/*.h crates/world-sys/world-src/

# Verify build
cargo build
```

### 8.3 Cargo Workspace (Cargo.toml)

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
ratatui = "0.30"
# crossterm is re-exported by ratatui 0.30 — no separate dep needed
cpal = "0.17"
symphonia = { version = "0.5", features = ["mp3", "wav", "pcm", "flac"] }
rustfft = "6.4"
ndarray = "0.17"
ringbuf = "0.4"
crossbeam-channel = "0.5"
hound = "3.5"
fundsp = { version = "0.23", default-features = false, features = ["std"] }
pitch_shift = "1"
```

---

## 9. Implementation Phases

| Phase | Scope | Milestone |
|---|---|---|
| **P0** | Scaffold project, build WORLD FFI, roundtrip test (analyze → synthesize ≈ original) | `cargo test` passes |
| **P1** | Audio decoder (symphonia) + playback (cpal) with play/pause/seek | Can play a WAV from terminal |
| **P2** | ratatui skeleton: layout, transport controls, file info bar | TUI renders, keys work |
| **P3** | WORLD integration: load file → analyze → display sliders → modify → resynthesize | Sliders change audio |
| **P4** | A/B comparison toggle, dual buffer playback | `Tab` switches instantly |
| **P5** | Real-time spectrum display via rustfft | Bars animate during playback |
| **P6** | Effects chain: fundsp (gain, filters, limiter, reverb) + pitch_shift (phase vocoder FX) | Right panel functional |
| **P7** | Export processed audio (hound WAV writer) | `S` saves file |
| **P8** | Polish: mouse support, fine-tune slider UX, error handling, help screen | Release-ready |

---

## 10. Risks & Mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| WORLD C++ FFI complexity | High | Start with minimal API (dio + cheaptrick + d4c + synthesis). Test roundtrip first. |
| Audio latency on WSL2 | Medium | cpal with ALSA backend should work. Fallback: PulseAudio. Test early (P1). |
| Resynthesis too slow for interactive use | Medium | Cache aggressively. Only resynthesize on slider release, not during drag. Process in chunks. |
| ratatui slider UX limitations | Low | Custom widget. Unicode block chars (`▏▎▍▌▋▊▉█`) give smooth visual. |
| WORLD not designed for real-time | Medium | Offline analysis + cached resynthesis. Acceptable for a workbench tool. |

---

## 11. Future Extensions

- **Preset system**: Save/load slider configurations as JSON/TOML profiles
- **Batch mode**: CLI-only processing without TUI (reuse dsp/ modules)
- **Integration with voice-markup-pipeline**: Use VoiceForge profiles as the configuration backend for the .vmd pipeline
- **WGPU spectrum**: GPU-rendered spectrogram via wgpu if terminal rendering is too coarse
- **Live microphone input**: Real-time voice modification with sounddevice capture
