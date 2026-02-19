# VoiceForge User Use Case Analysis (P0-P4)

**Document Version:** 1.1
**Date:** February 19, 2026
**Project Phase Coverage:** P0 (Scaffolding) through P4 (A/B Comparison)
**Audited:** See `audits/audit-inconsistencies-P0-P3.md`
**Total Use Cases:** 72 (48 Implemented + 24 Planned)

---

## Overview

This document provides a user-centric view of VoiceForge capabilities across implemented features (P0–P3) and planned features through P4. Use cases are grouped by feature area, with clear distinction between implemented (✓) and planned (◊) functionality. Parameter ranges reflect actual slider configurations from the codebase.

---

## 1. File Management (5 use cases, all implemented)

### File Loading & Browsing
1. **✓ User can load an audio file** by pressing `o` to open the file picker dialog
   - Program displays a text input where user types the file path
   - Input validated: file must exist and be a regular file
   - Supported formats: WAV, MP3, FLAC (via Symphonia decoder)
   - Status message displayed on success or error

2. **✓ User can cancel file selection** by pressing Esc in the file picker
   - Program returns to normal mode without loading
   - File picker input cleared
   - No file loaded if previously empty

3. **✓ User can view loaded file metadata** in the status bar
   - Displays: filename, sample rate (Hz), channel count (Mono/Stereo), duration (M:SS)
   - Processing status appended when active (e.g., "Analyzing...", "Processing...")
   - Example: ` File: speech.wav │ 44100 Hz │ Mono │ 3:45`

4. **✓ User can see invalid file error messages** in the status bar
   - Program shows "File not found: /path" or "Path is not a file"
   - Error messages persist until next successful file load or status update

5. **✓ User can load audio on startup** via command-line argument
   - Invoke: `voiceforge /path/to/audio.wav`
   - Program loads file automatically and begins WORLD analysis
   - If file invalid, shows error and starts with empty state

---

## 2. Playback Control (9 use cases)

### Play & Pause
1. **✓ User can play loaded audio** by pressing Space
   - Program starts audio output via cpal
   - Playback position advances from current location
   - Visual indicator in transport bar shows play state

2. **✓ User can pause audio** by pressing Space while playing
   - Program pauses output stream (internal state preserved)
   - Playback position freezes
   - Pressing Space again resumes from same position

3. **✓ User can seek forward 5 seconds** by pressing `]`
   - Program advances playback position by 5 seconds
   - Seeks within file bounds (clamped to [0, duration])
   - Smooth seeking without audio glitches

4. **✓ User can seek backward 5 seconds** by pressing `[`
   - Program rewinds playback position by 5 seconds
   - Seeks within file bounds (clamped to [0, duration])
   - Useful for quick looping during edits

5. **✓ User can loop audio** by pressing `r`
   - Program toggles loop mode on/off
   - When enabled, audio restarts from beginning upon reaching end
   - Loop state shown in status bar or transport display

6. **✓ User can view current playback position** via progress indicator
   - Transport bar displays: `[Play/Pause controls] ──●────── Current / Total time`
   - Format: `0:45 / 3:30`
   - Visual slider represents position in file

7. **◊ User can see seek-bar visualization** (planned P5 with effects)
   - Precise time position display during seeking
   - Estimated audio output quality indicator

8. **◊ User can jump to a specific time** via seek-bar interaction (planned)
   - Click on transport bar or type time (if TUI supports mouse)
   - Program seeks to that position instantly

9. **◊ User can set loop range** (planned P8 polish)
   - Define start/end points for partial file looping
   - Useful for editing small sections repeatedly

---

## 3. Voice Modification — WORLD Vocoder (13 use cases)

### Pitch Shift (Formant-Preserving)
1. **✓ User can shift pitch up** using the WORLD Pitch Shift slider
   - **Parameter:** `-12 to +12 semitones` (half-step units)
   - **Default:** `0.0` (no shift)
   - **Step size:** `0.5 semitones`
   - Voiced frames (f0 > 0) shifted; unvoiced frames unchanged
   - Formants preserved (natural-sounding, no "chipmunk" effect)
   - Real-time preview on slider change (debounced 150ms)

2. **✓ User can shift pitch down** using the WORLD Pitch Shift slider
   - Same parameter range/behavior as pitch up
   - Useful for gender-sounding effects or transposition

3. **✓ User can make fine pitch adjustments** using Shift+Left/Right arrows
   - Fine adjustment: `±0.1 semitones` per keystroke (0.2 steps × 0.5 st/step)
   - Coarse adjustment: `±0.5 semitones` per keystroke (1.0 steps × 0.5 st/step)
   - Allows precise tuning without mouse

### Pitch Dynamics (Range Expansion/Compression)
4. **✓ User can compress pitch range** using WORLD Pitch Range slider
   - **Parameter:** `0.2 to 3.0×` (multiplier)
   - **Default:** `1.0` (no change)
   - **Step size:** `0.1×`
   - Compresses variation around mean pitch: makes monotone-like, robotic
   - Voiced frames only; unvoiced unaffected

5. **✓ User can expand pitch range** using WORLD Pitch Range slider
   - Same parameter range as compression
   - Expands variation around mean: more exaggerated intonation
   - Useful for expressive or character voices

### Temporal Modification
6. **✓ User can speed up audio** using WORLD Speed slider
   - **Parameter:** `0.5 to 2.0×` (time factor)
   - **Default:** `1.0` (no change)
   - **Step size:** `0.05×`
   - Resamples entire signal via linear interpolation
   - Maintains pitch via WORLD (no pitch drift)
   - 0.5× = twice as slow; 2.0× = twice as fast

7. **✓ User can slow down audio** using WORLD Speed slider
   - Same parameter range as speed up
   - Preserves pitch while changing tempo

### Breathiness & Noise
8. **✓ User can add breathiness** using WORLD Breathiness slider
   - **Parameter:** `0 to 3.0×` (additive amount)
   - **Default:** `0.0` (no added breathiness)
   - **Step size:** `0.1×`
   - Increases aperiodicity (H/V ratio) to add whisper/aspiration
   - Useful for breathy voices or asthma-like effects

9. **✓ User can reduce noise floor** by leaving Breathiness at neutral
   - Default state removes no noise
   - Useful for clean, crisp-sounding output

### Formant Modification
10. **✓ User can shift formants up** using WORLD Formant Shift slider
    - **Parameter:** `-5 to +5 semitones`
    - **Default:** `0.0`
    - **Step size:** `0.5 semitones`
    - Warps spectral envelope (formant frequencies shift without pitch change)
    - Changes vocal tract resonances: smaller mouth = higher formants

11. **✓ User can shift formants down** using WORLD Formant Shift slider
    - Same parameter range as shift up
    - Creates deeper, larger-mouth-like sound

### Spectral Tilt
12. **✓ User can tilt spectrum brighter** using WORLD Spectral Tilt slider
    - **Parameter:** `-6 to +6 dB/octave`
    - **Default:** `0.0`
    - **Step size:** `0.5 dB/octave`
    - Increases high-frequency content (brightens, adds sibilance)
    - Positive values = brighter (more treble)

13. **✓ User can tilt spectrum darker** using WORLD Spectral Tilt slider
    - Same parameter range as brighten
    - Decreases high-frequency content (dulls, less sibilant)
    - Negative values = darker (less treble)

---

## 4. Voice Modification — Effects (11 use cases)

> **Note:** Effects sliders exist in the UI and can be adjusted, but the effects
> processing chain is not yet implemented (planned P6). The slider values are
> stored but do not currently affect audio output. Descriptions below reflect
> intended behavior once P6 is complete.

### Gain/Volume Control
1. **◊ User can increase output level** using Effects Gain slider
   - **Parameter:** `-12 to +12 dB`
   - **Default:** `0.0 dB`
   - **Step size:** `0.5 dB`
   - Slider UI implemented; audio processing planned P6

2. **◊ User can decrease output level** using Effects Gain slider
   - Same parameter range as increase
   - Slider UI implemented; audio processing planned P6

### Frequency Filtering
3. **◊ User can apply high-pass (low-cut) filter** using Effects Low Cut slider
   - **Parameter:** `20 to 500 Hz`
   - **Default:** `20 Hz` (minimal filtering, full bandwidth)
   - **Step size:** `10 Hz`
   - Slider UI implemented; audio processing planned P6

4. **◊ User can apply low-pass (high-cut) filter** using Effects High Cut slider
   - **Parameter:** `2000 to 20000 Hz`
   - **Default:** `20000 Hz` (no filtering, full bandwidth)
   - **Step size:** `500 Hz`
   - Slider UI implemented; audio processing planned P6

### Compression
5. **◊ User can apply dynamic compression** using Effects Compressor slider
   - **Parameter:** `-40 to 0 dB` (threshold)
   - **Default:** `0 dB` (no compression, full dynamic range)
   - **Step size:** `1.0 dB`
   - Slider UI implemented; audio processing planned P6

6. **◊ User can disable compression** by setting slider to 0 dB
   - Slider UI implemented; audio processing planned P6

### Spatial Effects
7. **◊ User can add reverb** using Effects Reverb Mix slider
   - **Parameter:** `0.0 to 1.0` (wet/dry balance)
   - **Default:** `0.0` (no reverb, dry signal only)
   - **Step size:** `0.05`
   - Slider UI implemented; audio processing planned P6

8. **◊ User can remove reverb** by setting slider to 0.0
   - Slider UI implemented; audio processing planned P6

### Pitch-Shifting Effects (Phase Vocoder)
9. **◊ User can apply effects-chain pitch shift** using Effects Pitch Shift FX slider
   - **Parameter:** `-12 to +12 semitones`
   - **Default:** `0.0` (no pitch shift)
   - **Step size:** `0.5 semitones`
   - **IMPORTANT:** Different from WORLD pitch shift — phase vocoder shifts everything (pitch + formants)
   - Slider UI implemented; audio processing planned P6

10. **◊ User can combine WORLD pitch shift with effects pitch shift** via two sliders
    - WORLD slider: formant-preserving (pitch shift only)
    - Effects slider: formant-shifting phase vocoder (pitch + formants)
    - Slider UI implemented; combined audio processing planned P6

### Effects-Only Updates
11. **✓ User can adjust effects sliders without triggering WORLD re-synthesis**
    - Effects panel (right side) changes do NOT trigger `Action::Resynthesize`
    - Only WORLD panel (left side) changes trigger WORLD re-processing
    - Effects slider values are stored but not yet applied to audio (planned P6)

---

## 5. Parameter Adjustment (8 use cases)

### Slider Navigation & Selection
1. **✓ User can focus on WORLD Vocoder panel** by pressing Tab
   - Program cycles panel focus: WORLD → Effects → Transport → WORLD
   - Focused panel highlighted with border or color
   - Slider index clamped to valid range (0 to panel size - 1)

2. **✓ User can focus on Effects panel** by pressing Tab
   - Same behavior as WORLD focus cycling
   - Allows independent slider control per panel

3. **✓ User can focus on Transport panel** by pressing Tab
   - Cycles focus to transport (play/pause/loop controls)
   - Transport has no sliders; focus here disables up/down navigation

4. **✓ User can select previous slider** in focused panel by pressing Up arrow
   - Program decrements selected slider index (if > 0)
   - Does not wrap around; stays at 0 if already at top

5. **✓ User can select next slider** in focused panel by pressing Down arrow
   - Program increments selected slider index (if < panel size - 1)
   - Does not wrap around; stays at max if already at bottom

### Slider Value Adjustment
6. **✓ User can increase slider value (coarse)** by pressing Right arrow
   - **Coarse step:** `+1.0` step units (as defined per slider)
   - Example: Pitch Shift (+0.5 st) → moves from 0 to 0.5 semitones
   - Value clamped to [min, max] range
   - Triggers Action::Resynthesize if WORLD panel focused

7. **✓ User can decrease slider value (coarse)** by pressing Left arrow
   - **Coarse step:** `-1.0` step units
   - Example: Pitch Shift (-0.5 st) → moves from 0 to -0.5 semitones
   - Value clamped to [min, max] range
   - Triggers Action::Resynthesize if WORLD panel focused

8. **✓ User can make fine adjustments** by pressing Shift+Left/Right arrows
   - **Fine step:** `±0.2` step units (vs. coarse ±1.0)
   - More granular control for precise tuning
   - Example: Pitch Shift +0.1 semitones per keystroke
   - Only works on sliders (not transport)

---

## 6. A/B Comparison (6 use cases)

### Audio Buffer Switching
1. **✓ User can toggle between original and processed audio** by pressing `a`
   - Only active after WORLD analysis completes (both buffers must exist)
   - Program swaps audio buffer in playback stream's RwLock via `swap_audio`
   - Seamless switch: audio callback picks up new buffer on next read-lock
   - Playback position scaled proportionally when buffer lengths differ
   - Transport bar displays `[A]` or `[B]` indicator in magenta

2. **✓ User can hear original (unprocessed) audio** while A/B is in "Original" mode
   - Program outputs mono downmix of original file
   - WORLD analysis has captured this; no need for re-analysis
   - Useful baseline for comparison

3. **✓ User can hear processed audio** while A/B is in "Processed" mode
   - Program outputs audio after all WORLD modifications applied
   - Effects sliders NOT applied in current pipeline (planned P6)
   - Real-time feedback on slider changes

### A/B Workflow
4. **✓ User can adjust sliders while A/B is active** and hear both versions
   - Adjust WORLD slider → Processed version updates
   - Press `a` to switch to Original → hear baseline
   - Press `a` again → hear updated Processed version
   - Allows direct comparison of before/after

5. **✓ User can see visual A/B indicator in transport bar**
   - Transport bar displays `[A]` or `[B]` in magenta based on `ab_original` state
   - `[A]` = original audio, `[B]` = processed audio

6. **◊ User can sync A/B position display** with spectrum visualization (planned P5)
   - When A/B toggles, spectrum changes to reflect current buffer's content
   - Visual confirmation of which version is active

---

## 7. UI Navigation (7 use cases, all implemented)

### Mode Transitions
1. **✓ User can enter file picker mode** by pressing `o`
   - Program switches from Normal to FilePicker mode
   - Status bar or overlay shows input prompt
   - File picker input field ready for typing

2. **✓ User can exit file picker without loading** by pressing Esc
   - Program returns to Normal mode immediately
   - No file loaded; input cleared
   - Last file remains active if one was previously loaded

3. **✓ User can navigate file picker input** using Backspace to delete characters
   - Each Backspace removes last character from path input
   - Allows correction of typing mistakes

4. **✓ User can type file path** into file picker using any printable character
   - Program accumulates input string
   - Path validated only on Enter key press

5. **✓ User can submit file path** by pressing Enter in file picker
   - Path validation: must exist, must be regular file
   - On success: loads file, begins WORLD analysis
   - On failure: shows error message, returns to file picker

### Panel & Focus Management
6. **✓ User can navigate between three panel groups** via Tab key
   - Cycling order: WORLD Sliders → Effects Sliders → Transport → WORLD...
   - Useful for keyboard-only navigation
   - Each panel maintains independent selected slider index

7. **✓ User can see focus indicator** on currently focused panel
   - Panel border highlighted or colored differently
   - Selected slider within panel visually marked
   - Feedback loop: user always knows what will be affected by arrow keys

---

## 8. Spectrum Visualization (4 use cases)

### Real-Time Display (Planned P5)
1. **◊ User can see real-time FFT spectrum** during playback
   - Program computes 2048-point FFT on current audio window
   - Display updates every 2-3 render frames (~20-30 Hz spectrum update)
   - Uses logarithmic frequency scale (more bars for low freqs)
   - Magnitude displayed in dB range: -80 to 0 dB (normalized)

2. **◊ User can see frequency labels** on spectrum X-axis
   - Labels show: 20Hz, 200Hz, 1kHz, 5kHz, 20kHz
   - Helps user identify which frequencies are being emphasized

3. **◊ User can observe spectrum changes** when pressing A/B toggle
   - Original buffer: shows spectral content of source file
   - Processed buffer: shows modified spectrum with WORLD/effects applied
   - Visual feedback: pitch shift, formant shift, spectral tilt all visible

4. **◊ User can see spectrum freeze** when pausing playback
   - Last FFT window displayed as static
   - Resumes animating when playback resumed
   - Prevents flickering on pause

---

## 9. Settings & Advanced (3 use cases)

### Processing Configuration (Planned P4+)
1. **◊ User can view WORLD analysis settings** in status bar or settings panel
   - Frame period (ms): `5.0` (default)
   - FFT size: automatic based on sample rate
   - Harvest/DIO algorithm parameters (not user-configurable in P0-P4)

2. **◊ User can adjust WORLD analysis quality** (planned P8 polish)
   - Tradeoff: faster analysis vs. higher pitch tracking accuracy
   - Settings menu: "Fast", "Balanced" (default), "High Quality"

3. **◊ User can view processing timing metrics** in status bar
   - Currently shows "Analyzing..." and "Processing..." status text (implemented)
   - Elapsed time / benchmark display not yet implemented
   - No "Ready" idle indicator — status field cleared on completion

---

## 10. Export (3 use cases — Planned P7)

### File Export Workflow
1. **◊ User can save processed audio** by pressing `s` (export mode)
   - Program prompts for output filename
   - Default suggestion: `{original_stem}_processed.wav`
   - User can edit path or press Esc to cancel

2. **◊ User can choose output format** (planned P7 extension)
   - 16-bit PCM WAV: standard, widely compatible
   - 32-bit float WAV: lossless, preserves DSP precision
   - MP3 (optional): compressed, smaller file size

3. **◊ User can prevent file overwrite** with auto-numbering
   - If `speech_processed.wav` exists, saves as `speech_processed_2.wav`
   - Prevents accidental data loss
   - Shows confirmation: "Saved to: /path/speech_processed_2.wav"

---

## 11. Application Control (3 use cases)

### Quitting & Status
1. **✓ User can quit the application** by pressing `q` or Esc
   - Program exits TUI cleanly (terminal restored)
   - Audio playback stops
   - All threads gracefully shut down
   - No data loss if file not exported

2. **✓ User can view status messages** in the status bar
   - File metadata: name, sample rate, channels (Mono/Stereo), duration (M:SS)
   - Processing status appended in yellow when active: "Analyzing...", "Processing..."
   - Error messages shown in red when no file loaded: "File not found", etc.
   - Processing status clears on completion; error messages persist until next successful file load

3. **✓ User can see error feedback** on failed file loads
   - "File not found: /bad/path"
   - "unsupported format: xyz"
   - "zero channels"
   - Messages shown in status bar; user can try again with `o`

---

## Summary Statistics

### Implementation Coverage

| Category | Total | Implemented | Planned |
|----------|-------|-------------|---------|
| File Management | 5 | 5 | 0 |
| Playback Control | 9 | 6 | 3 |
| WORLD Vocoder | 13 | 13 | 0 |
| Effects | 11 | 1 | 10 |
| Parameter Adjustment | 8 | 8 | 0 |
| A/B Comparison | 6 | 5 | 1 |
| UI Navigation | 7 | 7 | 0 |
| Spectrum Visualization | 4 | 0 | 4 |
| Settings & Advanced | 3 | 0 | 3 |
| Export | 3 | 0 | 3 |
| Application Control | 3 | 3 | 0 |
| **TOTALS** | **72** | **48** | **24** |

**Note:** Effects sliders exist in the UI (adjustable, navigable) but audio processing is planned P6. Only UC11 (slider adjustment without WORLD re-synthesis) is counted as implemented.

### Keyboard Shortcut Reference

| Key(s) | Action | Category |
|--------|--------|----------|
| `o` | Open file picker | File Management |
| `Esc` | Quit / Cancel file picker | Application / File |
| `q` | Quit application | Application |
| Space | Toggle play/pause | Playback |
| `[` | Seek back 5 seconds | Playback |
| `]` | Seek forward 5 seconds | Playback |
| `r` | Toggle loop mode | Playback |
| `a` | Toggle A/B (original/processed) | A/B Comparison |
| Tab | Cycle panel focus (World → Effects → Transport) | UI Navigation |
| Up arrow | Select previous slider | Parameter Adjustment |
| Down arrow | Select next slider | Parameter Adjustment |
| Left arrow | Decrease slider (coarse, -1.0 step) | Parameter Adjustment |
| Right arrow | Increase slider (coarse, +1.0 step) | Parameter Adjustment |
| Shift+Left | Decrease slider (fine, -0.2 step) | Parameter Adjustment |
| Shift+Right | Increase slider (fine, +0.2 step) | Parameter Adjustment |
| Backspace (file picker) | Delete character from path | File Management |
| Enter (file picker) | Confirm file selection | File Management |
| `s` | **[Planned P7]** Save processed audio | Export |

**Total Shortcuts:** 17 (13 implemented, 1 planned explicit, 3 planned features)

### Audio Format Support

**Supported Input Formats (via Symphonia decoder):**
- WAV (PCM, various bit depths)
- MP3 (MPEG Layer III)
- FLAC (Free Lossless Audio Codec)

**Supported Output Formats (Planned P7):**
- WAV (16-bit PCM, 32-bit float) — implemented
- MP3 (optional; compression format)

### WORLD Vocoder Parameters Summary

| Parameter | Range | Default | Step | Unit | Function |
|-----------|-------|---------|------|------|----------|
| Pitch Shift | -12 to +12 | 0.0 | 0.5 | st (semitones) | Formant-preserving pitch transposition |
| Pitch Range | 0.2 to 3.0 | 1.0 | 0.1 | × (multiplier) | Expand/compress pitch dynamics around mean |
| Speed | 0.5 to 2.0 | 1.0 | 0.05 | × | Time-stretching (pitch maintained) |
| Breathiness | 0 to 3.0 | 0.0 | 0.1 | × | Add aperiodicity/whisper noise |
| Formant Shift | -5 to +5 | 0.0 | 0.5 | st | Spectral envelope warping (vocal tract resonance) |
| Spectral Tilt | -6 to +6 | 0.0 | 0.5 | dB/oct | High-frequency brightness adjustment |

### Effects Parameters Summary

| Parameter | Range | Default | Step | Unit | Function |
|-----------|-------|---------|------|------|----------|
| Gain | -12 to +12 | 0.0 | 0.5 | dB | Output level scaling |
| Low Cut (High-Pass) | 20 to 500 | 20 | 10 | Hz | Sub-bass rumble removal |
| High Cut (Low-Pass) | 2000 to 20000 | 20000 | 500 | Hz | Sibilance/noise removal |
| Compressor | -40 to 0 | 0.0 | 1.0 | dB | Dynamic range compression threshold |
| Reverb Mix | 0 to 1.0 | 0.0 | 0.05 | (dry/wet) | Spatial reflection intensity |
| Pitch Shift FX | -12 to +12 | 0.0 | 0.5 | st | Phase-vocoder pitch shift (formant-shifting) |

### Threading & Performance Notes

**Analysis Phase (Offline):**
- WORLD analysis: ~2–5 seconds per minute of audio
- Single-threaded, blocking processing thread
- Main thread remains responsive (separate ratatui event loop)

**Resynthesis Phase (On Demand):**
- WORLD synthesis: ~1–2 seconds per minute of audio
- Triggered only when WORLD sliders change (debounced 150ms)
- Neutral sliders (all at default): skips synthesis, outputs mono downmix

**Playback:**
- Real-time audio callback (cpal) reads from `Arc<AudioData>` via `RwLock`
- A/B toggle: sub-millisecond buffer swap via `swap_audio`, glitch-free
- Effects chain not yet applied to audio output (planned P6)

**Rendering:**
- TUI updates ~30 FPS (ratatui event loop)
- Spectrum FFT: every 2–3 frames (if P5 implemented)

---

## Implementation Roadmap Reference

| Phase | Features | Status | Primary Use Cases |
|-------|----------|--------|-------------------|
| **P0** | WORLD FFI scaffolding, roundtrip test | ✓ Complete | Test infrastructure |
| **P1** | Audio decoder (Symphonia), cpal playback | ✓ Complete | File loading, basic playback |
| **P1b** | WSL2 audio fix (PulseAudio) | ✓ Complete | WSL2 users: audio output |
| **P2** | TUI skeleton (ratatui), file picker, sliders | ✓ Complete | UI navigation, slider interaction |
| **P3** | WORLD integration, slider-driven resynthesis | ✓ Complete | Voice modification (12 sliders, 6 WORLD active) |
| **P3b** | Audit integration corrections | ✓ Complete | API compatibility fixes |
| **P4** | A/B comparison toggle | ✓ Complete | Direct before/after comparison |
| **P5** | Spectrum FFT visualization | Planned | Real-time frequency analysis |
| **P6** | Effects chain (fundsp) implementation | Planned | Full effects pipeline |
| **P7** | WAV export / save workflow | Planned | File export, output saving |
| **P8** | Polish, optimization, edge case handling | Planned | UX refinement, stability |

---

## Key Distinctions: Implemented vs. Planned

### Implemented (✓) — P0–P4 Complete
- File loading, metadata display, error messages, CLI loading
- Basic playback (play/pause, seek, loop toggle)
- All WORLD vocoder sliders with active audio processing (6 transforms)
- All effects sliders (UI only — values stored, processing planned P6)
- Keyboard-driven slider adjustment (arrows, fine tuning)
- UI panel navigation and focus management
- Terminal-based TUI rendering (ratatui)
- Real-time WORLD resynthesis (debounced 150ms)
- A/B comparison toggle with buffer swap
- Status bar feedback and error messages
- Command-line file loading

### Planned (◊) — P5 & Beyond
- **P5:** Spectrum FFT visualization with frequency labels
- **P6:** Effects chain integration (fundsp-based processing)
- **P7:** WAV export with dialog and auto-numbering
- **P8:** Polish, advanced settings, optimization

### Major Missing from P0–P4
- **Effects processing:** Sliders exist in UI, but effects chain not yet applied to audio (planned P6)
- **Spectrum display:** Placeholder widget; FFT computation planned (P5)
- **Export dialog:** Save workflow not implemented (P7)
- **Settings menu:** WORLD analysis parameters not user-configurable yet
- **Loop playback:** Toggle and display exist, but audio callback does not wrap position

---

## Design Assumptions & Constraints

1. **Mono Output:** WORLD always synthesizes mono. Stereo original is downmixed on first analysis. Both A/B buffers are mono after processing.

2. **Real-Time Latency:** WORLD analysis is offline (~2–5s/min). Playback is real-time; effects chain targets <50ms processing latency.

3. **No Mouse Support:** Current implementation keyboard-only. Tab/arrows for navigation. Mouse support (if added later) would enhance slider/spectrum interaction.

4. **Effects Chain Order (Planned P6):**
   ```
   WORLD output (mono)
     → Gain
     → Low-Cut filter
     → High-Cut filter
     → Compression
     → Reverb (mix)
     → Pitch Shift FX
     → Output to playback
   ```

5. **File Path Validation:** Only checks file existence and type; no symlink resolution or permission pre-check (deferred to OS open).

6. **Error Recovery:** File load errors show message but don't clear prior state. User can retry with `o` key.

---

## Notes for Future Enhancement

### Potential P8+ Features
- **Batch processing:** Apply same slider values to multiple files
- **Preset system:** Save/load slider configurations
- **Undo/redo:** Revert slider changes
- **Macro recording:** Automate slider sequences
- **MIDI control:** Drive sliders via external MIDI controller
- **Multi-voice harmony:** Clone voice with pitch offsets
- **Real-time latency reduction:** GPU WORLD processing (if feasible)
- **Analysis previewing:** FFT of input before processing

### Known Limitations (P0–P4)
- Stereo source becomes mono after first WORLD analysis (by design)
- No real-time pitch detection visualization
- Effects not yet applied to output (P6 dependency)
- Spectrum frozen on pause; no interpolation
- Single-threaded WORLD processing (no parallel analysis)
- File picker text input only (no directory browsing UI)

---

## Test Coverage Summary

**Current Test Count:** 18 tests (from CLAUDE.md)

| Category | Count |
|----------|-------|
| Decoder tests | 4 |
| WORLD FFI tests | 11 |
| Modifier tests | 3 |
| **Total** | **18** |

**Tests for Planned Features (P4–P8):** Not yet written; to be added as phases complete.

---

## Conclusion

VoiceForge currently delivers a robust, keyboard-driven voice modulation TUI with advanced WORLD vocoder integration (P0–P3 complete). Users can load audio, manipulate pitch/formants/dynamics in real time, and hear changes instantly. The next milestone (P4) will enable seamless A/B comparison; subsequent phases (P5–P8) will add spectrum visualization, effects processing, export, and polish.

This use case analysis provides a user-centric roadmap for both current capabilities and planned features, grounded in actual codebase parameters and architecture.

---

**Document Maintainer:** Claude Code (Anthropic)
**Last Updated:** February 19, 2026
**Status:** Complete for P0–P4 coverage; to be updated as P5–P8 phases are implemented
