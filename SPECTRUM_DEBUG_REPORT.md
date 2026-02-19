# GPU Spectrum Visualizer - Comprehensive Debug Implementation

**Status:** 🔍 DEBUG BUILD READY
**Objective:** Diagnose why spectrum visualization appears as all-black

---

## What Was Reviewed

### ✅ Rendering Path Architecture
- GPU pixel path (ratatui-image StatefulImage widget)
- Unicode fallback (colored block characters)
- Picker initialization and protocol detection
- Spectrum image generation (punk gradient colors)

### ✅ WezTerm Compatibility
- Terminal environment detection (`TERM_PROGRAM`, `TERM`, `WSL_DISTRO_NAME`)
- Font size querying (`Picker::from_termios()`)
- Graphics protocol auto-detection (iTerm2 preference on WezTerm)
- Fallback mechanism when graphics unavailable

### ✅ Critical Failure Points
1. **Initialization:** Picker creation and protocol selection
2. **Spectrum Computation:** FFT data validation
3. **Image Generation:** Pixel fill and color interpolation
4. **Rendering Decision:** GPU vs Unicode path selection
5. **Render Area:** Dimensions and scaling

---

## Issues Identified & Fixed

| # | Issue | Severity | Fix |
|---|-------|----------|-----|
| 1 | No WezTerm environment logging | MEDIUM | Added TERM_PROGRAM, WSL_DISTRO_NAME checks |
| 2 | Silent terminal query failures | HIGH | Log all Picker initialization steps |
| 3 | Spectrum data not validated | HIGH | Monitor max_db every ~1 sec |
| 4 | Image generated without verification | MEDIUM | Count colored pixels in result |
| 5 | Protocol creation errors ignored | MEDIUM | Explicit error handling |
| 6 | Silent rendering path selection | MEDIUM | Log GPU vs fallback decision |
| 7 | Spectrum quality unknown | MEDIUM | Report min/max dB values |
| 8 | Image generation black box | MEDIUM | Log pixel fill counts |

---

## Debugging Instrumentation Added

### Initialization Logs (one-time at startup)
```
[SPECTRUM_INIT] TERM_PROGRAM=WezTerm, TERM=xterm-256color, WSL=Ubuntu
[SPECTRUM_INIT] from_termios result: OK
[SPECTRUM_INIT] Got font size: 8x16
[SPECTRUM_INIT] Auto-selected protocol: Iterm2
```

### Layout Logs (one-time at first render)
```
[SPECTRUM_RENDER] render area: 120x20 (outer), 118x18 (inner after borders)
```

### Continuous Monitoring (every ~1 second)
```
[SPECTRUM] frame=30, bins=1024, max_db=-20.5, picker=true
```

### Per-Frame Rendering Decision
```
[SPECTRUM_RENDER] Using GPU pixel path (StatefulImage)
[SPECTRUM_IMAGE] spectrum_to_image called: 256x128, 1024 bins
[SPECTRUM_IMAGE] generated: 3420 colored pixels out of 12800
```

Or fallback:
```
[SPECTRUM_RENDER] Using Unicode fallback (render_unicode_fallback)
[SPECTRUM_FALLBACK] rendering 118x18, bins: count=1024, min=-50.0dB, max=-15.3dB
```

---

## How to Run the Debug Build

### 1. Quick Test with an Audio File
```bash
./test_spectrum_debug.sh ~/path/to/audio.wav
```

Then in the app:
- Press `Space` to play
- Watch stderr for debug messages
- Press `q` to quit

### 2. Manual Test with Full Output Capture
```bash
cargo run ~/path/to/audio.wav 2>&1 | tee debug_output.log
```

Then open another terminal:
```bash
# Monitor spectrum logs in real-time
tail -f debug_output.log | grep SPECTRUM
```

### 3. Test with Different Terminal Configurations

**WezTerm (GPU path expected):**
```bash
cargo run ~/audio.wav 2>&1 | head -30
```

**Fallback test (force Unicode):**
```bash
TERM=dumb cargo run ~/audio.wav 2>&1 | head -30
```

---

## What to Look For - Quick Diagnostic

### ✅ Working GPU Path
```
[SPECTRUM_INIT] Auto-selected protocol: Iterm2
[SPECTRUM_RENDER] Using GPU pixel path (StatefulImage)
[SPECTRUM_IMAGE] generated: XXXX colored pixels  # <- Non-zero
```

### ✅ Working Fallback Path
```
[SPECTRUM_INIT] from_termios result: FAILED
[SPECTRUM_RENDER] Using Unicode fallback
[SPECTRUM_FALLBACK] rendering, max_db=XX.X  # <- Should see colors
```

### ❌ Audio Not Loading
```
[SPECTRUM] frame=30, bins=1024, max_db=-inf  # <- Infinity = no audio
[SPECTRUM_FALLBACK] spectrum_bins is empty
```

### ❌ Audio Silent/Too Quiet
```
[SPECTRUM] frame=30, bins=1024, max_db=-80.0  # <- Below -50dB floor
[SPECTRUM_IMAGE] generated: 0 colored pixels out of 12800  # <- All black
```

### ❌ Render Area Too Small
```
[SPECTRUM_FALLBACK] render area too small: 2x0
```

---

## Documentation Files

1. **DEBUG_SPECTRUM.md** - Complete debugging guide with troubleshooting
2. **audits/P5_GPU_SPECTRUM_AUDIT.md** - Detailed issue analysis and fixes
3. **test_spectrum_debug.sh** - Quick test script for easy debugging
4. **This file** - Quick reference

---

## Expected Output Examples

### Scenario 1: WezTerm + Good Audio (GPU Path)
```
[SPECTRUM_INIT] TERM_PROGRAM=WezTerm, TERM=xterm-256color, WSL=Ubuntu
[SPECTRUM_INIT] from_termios result: OK
[SPECTRUM_INIT] Got font size: 8x16
[SPECTRUM_INIT] Auto-selected protocol: Iterm2
[SPECTRUM_RENDER] render area: 120x20 (outer), 118x18 (inner)
[SPECTRUM] frame=30, bins=1024, max_db=-15.3, picker=true
[SPECTRUM_RENDER] Using GPU pixel path (StatefulImage)
[SPECTRUM_IMAGE] spectrum_to_image called: 256x128, 1024 bins
[SPECTRUM_IMAGE] generated: 3420 colored pixels out of 12800

Result: ✅ Smooth pixel gradient visible in spectrum
```

### Scenario 2: Terminal Without GPU Support (Fallback Path)
```
[SPECTRUM_INIT] from_termios result: FAILED, using fallback
[SPECTRUM_INIT] Fallback protocol: Halfblocks
[SPECTRUM_RENDER] render area: 100x10 (outer), 98x8 (inner)
[SPECTRUM] frame=30, bins=1024, max_db=-18.0, picker=true
[SPECTRUM_RENDER] Using Unicode fallback (render_unicode_fallback)
[SPECTRUM_FALLBACK] rendering 98x8, min=-50.0dB, max=-18.0dB

Result: ✅ Colored Unicode blocks (green/yellow/red) visible
```

### Scenario 3: Audio Not Playing
```
[SPECTRUM] frame=30, bins=1024, max_db=-inf, picker=true
[SPECTRUM_RENDER] Using Unicode fallback (render_unicode_fallback)
[SPECTRUM_FALLBACK] spectrum_bins is empty

Result: ❌ "No audio playing" message in spectrum area
```

### Scenario 4: Audio Silent/Too Quiet
```
[SPECTRUM] frame=30, bins=1024, max_db=-85.0, picker=true
[SPECTRUM_IMAGE] generated: 0 colored pixels out of 12800

Result: ❌ All-black spectrum (audio below -50dB floor)
```

---

## Recommended Next Steps

### For Users
1. **Run the debug build** with an audio file
2. **Observe the initialization logs** to see terminal capabilities
3. **Play audio** and watch the spectrum logs
4. **Screenshot or save the log output**
5. **Identify which scenario matches your output**

### For Development
Once we identify the failure point:
- If "GPU path not activating" → Fix ratatui-image integration
- If "spectrum all-black" → Adjust dB scaling or audio source
- If "terminal query failing" → Implement fallback detection
- If "render area wrong" → Fix layout dimension calculations

---

## Building Without Debug Output (If Needed)

To clean up the stderr for release:
```bash
# Remove unsafe static frame counting
# Remove all eprintln! debug statements
# Build release version
cargo build --release
```

The debug build is intentionally verbose to diagnose the black spectrum issue.

---

## Key Takeaways

✅ **What's Working:**
- Rendering infrastructure (both GPU and Unicode paths)
- Spectrum computation (FFT, bin mapping)
- Color interpolation (punk gradient)
- Terminal detection (attempted)

❓ **What We Need to Verify:**
- Is `spectrum_state` being populated?
- Is the GPU path being selected?
- Are spectrum values above -50dB?
- Is the render area large enough?

🔧 **Available Tools:**
- Full debug instrumentation at all critical steps
- Easy test script for quick validation
- Comprehensive documentation for troubleshooting

---

**Next Action:** Run `./test_spectrum_debug.sh <audio_file>` and share the output.
