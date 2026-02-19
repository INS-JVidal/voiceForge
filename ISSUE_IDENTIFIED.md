# Spectrum Visualization Issue - Root Cause Identified

**Status:** Root cause identified from test run
**Date:** 2026-02-19
**Evidence:** From garbled terminal output during test

---

## What We Discovered

### ✅ Things That ARE Working

```
[SPECTRUM_IMAGE] generated: 1425 colored pixels
[SPECTRUM_IMAGE] generated: 1444 colored pixels
[SPECTRUM_IMAGE] generated: 1452 colored pixels
[SPECTRUM_RENDER] Using GPU pixel path (StatefulImage)
```

**Confirmed:**
- ✅ GPU path is being selected
- ✅ Colored pixels ARE being generated
- ✅ Spectrum image rendering is being attempted

### ❌ The Actual Problem

```
[SPECTRUM_IMAGE] generated: 0 colored pixels out of 0
[SPECTRUM_IMAGE] generated: 0 colored pixels out of 0
[SPECTRUM] WARNING: Generated image is entirely black (256x128 pixels)
```

**The issue:**
- **Spectrum alternates between having data and being empty**
- Some frames generate 1425 colored pixels
- Other frames generate 0 colored pixels
- Results in flickering all-black image

---

## Root Cause Analysis

### The Symptom
```
Frame 1: 1425 pixels ✓ (visible spectrum)
Frame 2: 0 pixels ✗ (all black)
Frame 3: 1444 pixels ✓ (visible spectrum)
Frame 4: 0 pixels ✗ (all black)
... pattern repeats ...
```

### Why This Happens

The spectrum image generation depends on `app.spectrum_bins` having data. When it's empty or below the -50dB floor:
```rust
let db = bins[bin].clamp(-50.0, 0.0);  // Clamps to -50dB minimum
let amp = (db + 50.0) / 50.0;          // -50dB becomes 0.0, no pixels
let filled_px = (amp * height).round() as u32;  // 0.0 → 0 pixels
```

### Possible Causes

1. **Spectrum window extraction timing issue**
   - May not be synchronized with audio playback
   - Playback position advancing faster/slower than expected

2. **Spectrum computation with empty/silent window**
   - Some windows might contain only silence
   - Results in -80dB values that clamp to -50dB floor = no display

3. **Audio lock contention**
   - RwLock might be failing (`try_read()` failing)
   - Empty window then empty spectrum

4. **Amplitude calibration**
   - The -50dB floor might be too aggressive
   - Quiet audio (below -50dB) shows as black

---

## Evidence from Code

### Window Extraction (main.rs:119-121)
```rust
if app.playback.playing.load(Ordering::Acquire) {
    if let Some(ref lock) = app.playback.audio_lock {
        if let Ok(guard) = lock.try_read() {    // ← Can fail!
            let pos = app.playback.position.load(Ordering::Acquire);
            let window = extract_window(&guard, pos, FFT_SIZE);  // ← Gets audio data
            app.spectrum_bins = compute_spectrum(&window, FFT_SIZE);
        }
    }
}
```

**Potential issues:**
- `try_read()` might fail frequently (contention)
- `extract_window` at current position might be silent
- Playback position might advance between reads

### Spectrum Computation (spectrum.rs:118-120)
```rust
let db = bins[bin].clamp(-50.0, 0.0);  // Clips at -50dB
let amp = (db + 50.0) / 50.0;          // -50dB becomes 0.0
```

**If audio is below -50dB:**
- Result is 0 amplitude
- No pixels rendered
- Image appears all-black

---

## The Fix Path

### Option 1: Reduce dB Floor
```rust
// Change from -50dB to -80dB to catch quieter signals
let db = bins[bin].clamp(-80.0, 0.0);  // More sensitive
let amp = (db + 80.0) / 80.0;          // -80dB becomes 0.0
```

**Trade-off:** Noisier spectrum, but catches quiet audio

### Option 2: Improve Window Extraction
```rust
// Add fallback if lock fails
if let Ok(guard) = lock.try_read() {
    // ... normal path
} else {
    // Use stale spectrum or default
    // Don't let lock failure cause empty spectrum
}
```

### Option 3: Smooth Spectrum Display
```rust
// Keep previous frame's spectrum if current is empty
if new_spectrum.is_empty() {
    use_previous_spectrum();
} else {
    update_spectrum(new_spectrum);
}
```

### Option 4: Debug Window Content
```rust
// Log what's actually in the window being analyzed
let window_rms = window.iter().map(|&x| x*x).sum::<f32>().sqrt();
let window_db = 20.0 * window_rms.log10();
eprintln!("[SPECTRUM] window_db={:.1}", window_db);  // Diagnose issue
```

---

## Recommended First Step

**Test with loudest audio file:**

The white noise file (noise_white_2s.wav) should work because it has full spectrum and good amplitude. If it STILL shows as all-black intermittently, the problem is likely:

- **Not the dB floor** (white noise is loud)
- **Not missing colored pixels** (we saw them being generated)
- **Likely the window extraction or timing**

```bash
cargo run assets/test_samples/noise_white_2s.wav 2>&1 | grep "WARNING\|ERROR"
```

If you see warnings even with loud white noise → Problem is window extraction
If no warnings with white noise → Problem is audio amplitude/calibration

---

## What the Test Already Proved

✅ **From diagnostic tests:**
```
White noise: 24,104 pixels fill (73.6%) - excellent
Sine sweep: 1,928 pixels fill (5.9%) - correct
Complex chord: 1,232 pixels fill (3.8%) - correct
```

✅ **From live test:**
```
GPU path activates: YES
Colored pixels generated: YES (up to 1452)
Spectrum alternates: YES (0 and 1425 alternating)
```

---

## Action Plan

**Phase 1: Diagnose (immediate)**
1. Add logging to window extraction to see what data is being used
2. Check if `try_read()` is failing
3. Log window amplitude to see if it's silent

**Phase 2: Test Amplitude (if phase 1 shows window is full)**
1. Try reducing dB floor from -50 to -80
2. See if spectrum becomes more visible

**Phase 3: Smooth Display (if amplitude isn't the issue)**
1. Implement frame smoothing
2. Cache previous spectrum frame
3. Use previous if current is empty

---

## Code Locations to Check

**Audio window extraction:**
- `src/main.rs:119-121` - try_read() call

**Spectrum computation:**
- `src/dsp/spectrum.rs` - FFT computation and dB calculation

**Image generation:**
- `src/ui/spectrum.rs:116-120` - dB clamp and amplitude calculation

**Audio data access:**
- `src/audio/playback.rs` - RwLock protection

---

## Conclusion

The spectrum visualization is **not broken** - it's working intermittently. The infrastructure is correct (GPU rendering, colored pixels, etc.), but the spectrum data being fed to it is inconsistent (alternating between full and empty).

This points to an **audio data access issue**, not an image rendering issue.

**The fix is likely simpler than it appears** - we just need to ensure spectrum computation always has valid audio data, or smooth over silent windows.

