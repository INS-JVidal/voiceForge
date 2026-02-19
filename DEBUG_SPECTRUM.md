# Spectrum Visualizer Debug Guide

## Build & Run with Full Debugging

```bash
cargo build 2>&1 && cargo run 2>&1 | tee spectrum_debug.log
```

This will capture ALL debug output to `spectrum_debug.log` and also display it in the terminal.

## What to Look For

### 1. Initialization Phase (appears once at startup)

```
[SPECTRUM_INIT] TERM_PROGRAM=WezTerm, TERM=xterm-256color, WSL=...
[SPECTRUM_INIT] from_termios result: OK or FAILED
[SPECTRUM_INIT] Got font size: 8x16
[SPECTRUM_INIT] Auto-selected protocol: Halfblocks or Sixel or Kitty or Iterm2
```

**What this tells you:**
- `TERM_PROGRAM=WezTerm` → WezTerm detected ✓
- `from_termios result: OK` → Terminal font size query succeeded ✓
- `Auto-selected protocol` → Which graphics protocol is being used

### 2. Layout Phase (appears once at startup)

```
[SPECTRUM_RENDER] render area: 120x20 (outer), 118x18 (inner after borders)
```

**What this tells you:**
- The spectrum area dimensions after subtracting borders
- If inner dimensions are too small, the spectrum will be cramped

### 3. Every 30 Frames (~1 second at 30fps)

```
[SPECTRUM] frame=30, bins=1024, max_db=XX.X, picker=true
```

**What this tells you:**
- Audio is being decoded/played
- Spectrum is being computed (bins are being filled)
- max_db value:
  - Should be above -50dB for visible spectrum
  - If it's -∞ or below -80dB, audio is silent or very quiet

### 4. Rendering Decision (every frame)

**If using GPU pixel path:**
```
[SPECTRUM_RENDER] Using GPU pixel path (StatefulImage)
[SPECTRUM_IMAGE] spectrum_to_image called: 256x128, 1024 bins
[SPECTRUM_IMAGE] generated: XXXX colored pixels out of XXXX
```

**If using Unicode fallback:**
```
[SPECTRUM_RENDER] Using Unicode fallback (render_unicode_fallback)
[SPECTRUM_FALLBACK] rendering 118x18, bins: count=1024, min=XX.XdB, max=XX.XdB
```

## Troubleshooting by Log Patterns

### Pattern 1: "No audio playing" shows in spectrum area
```
[SPECTRUM_FALLBACK] spectrum_bins is empty
```
**Diagnosis:** Spectrum computation isn't happening
- Audio might not be loading
- Audio might not be playing (press Space?)
- Check file is valid

### Pattern 2: All black spectrum with GPU path
```
[SPECTRUM_IMAGE] generated: 0 colored pixels out of 0
```
**Diagnosis:** All spectrum values are below -50dB (audio very quiet)
- Try a louder file or test tone
- Check audio is actually playing (position counter advancing?)

### Pattern 3: Using fallback instead of GPU
```
[SPECTRUM_RENDER] Using Unicode fallback (render_unicode_fallback)
```
**Diagnosis:**  `spectrum_state` is None
- Check: `from_termios result` at startup
- If FAILED: Terminal doesn't support graphics protocol query
- Fallback should still show colored Unicode bars

### Pattern 4: Render area too small
```
[SPECTRUM_FALLBACK] render area too small: 2x0
```
**Diagnosis:** Terminal or window is too small
- Expand your terminal window
- Minimum recommended: 80x24

## Step-by-Step Test Procedure

1. **Start the app:**
   ```bash
   cargo run 2>&1 | head -20
   ```
   Look for the initialization logs showing TERM, protocol, font size.

2. **Load an audio file:**
   - Press `o` in the app
   - Navigate to a WAV/MP3/FLAC file
   - Select it

3. **Press Space to play** and watch the logs:
   ```bash
   # In another terminal, watch logs in real-time
   tail -f spectrum_debug.log | grep SPECTRUM
   ```

4. **Observe:**
   - Does `max_db` value appear and change?
   - Does it say "GPU pixel path" or "Unicode fallback"?
   - Does the spectrum area show colors or is it black?

## WezTerm + WSL2 Specific Checks

1. **Verify WezTerm detection:**
   ```bash
   echo $TERM_PROGRAM  # Should output: WezTerm
   ```

2. **Verify graphics protocol support:**
   ```bash
   # WezTerm supports iTerm2 protocol natively
   # Check if detected:
   cargo run 2>&1 | grep "protocol:"
   ```

3. **Test fallback behavior:**
   ```bash
   TERM=dumb cargo run
   # Should show Unicode fallback with colored blocks
   ```

## Interpreting Results

### ✅ Good Case:
```
[SPECTRUM_INIT] from_termios result: OK
[SPECTRUM_INIT] Auto-selected protocol: Iterm2  ← WezTerm uses iTerm2
[SPECTRUM_RENDER] Using GPU pixel path (StatefulImage)
[SPECTRUM_IMAGE] generated: 2500 colored pixels out of 12800
```
→ GPU rendering working! You should see smooth pixel gradient.

### ⚠️  Fallback Case:
```
[SPECTRUM_INIT] from_termios result: OK
[SPECTRUM_INIT] Auto-selected protocol: Halfblocks
[SPECTRUM_RENDER] Using Unicode fallback (render_unicode_fallback)
[SPECTRUM_FALLBACK] rendering 118x18, bins: count=1024, min=-50.0dB, max=-10.0dB
```
→ Unicode blocks showing with colors. GPU unavailable but fallback works.

### ❌ Problem Case:
```
[SPECTRUM] frame=30, bins=1024, max_db=-inf, picker=true
[SPECTRUM_IMAGE] generated: 0 colored pixels out of 0
```
→ Audio is silent or not playing. Check audio file and playback.

## Enable Debug Logging Selectively

To reduce output noise, you can filter:

```bash
# Only spectrum debug messages
cargo run 2>&1 | grep "\\[SPECTRUM"

# Only initialization
cargo run 2>&1 | grep "\\[SPECTRUM_INIT"

# Monitor spectrum generation only
watch -n 1 'cargo run 2>&1 | grep SPECTRUM | tail -5'
```

## Known Issues & Workarounds

### Issue: "from_termios result: FAILED"
**Cause:** Terminal font size query not supported
**Workaround:** Falls back to default 8x16 font size (should still work)

### Issue: "Protocol: Halfblocks"
**Cause:** Terminal doesn't support graphics protocols
**Workaround:** Unicode fallback still works, shows colored blocks

### Issue: Black spectrum even with GPU path
**Cause:** Audio is very quiet (below -50dB floor)
**Workaround:** Try a louder audio file or check audio volume

## Next Steps for Further Debugging

If logs don't match expected patterns, check:

1. **Audio is loading:** Does `[SPECTRUM] bins=1024` appear?
2. **Audio is playing:** Does `max_db` value change over time?
3. **Spectrum is computing:** Does `spectrum_to_image` get called?
4. **Rendering path:** GPU or Unicode fallback being used?

Attach the full `spectrum_debug.log` output if reporting issues.
