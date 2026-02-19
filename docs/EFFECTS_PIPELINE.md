# VoiceForge: Effects & EQ Pipeline

## 1. Overview

The effects pipeline applies audio post-processing in two distinct paths:

1. **WORLD Resynthesis Path** (`Resynthesize` command)
   - Modifies WORLD parameters (pitch, formants, spectrum)
   - Runs parameter modifier (`modifier::apply`)
   - Runs WORLD synthesis (C++ FFI)
   - Runs effects chain (EQ, compression, reverb, filters, gain)
   - Slower (100–200 ms per frame)
   - Cached output (`post_world_audio`) is used by subsequent effects-only operations

2. **Effects-Only Path** (`ReapplyEffects` command)
   - Reads cached `post_world_audio` (already processed by WORLD)
   - Re-applies effects chain with new settings
   - Faster (~50 ms)
   - Triggered by effects slider changes (EQ, compression, reverb, etc.)

3. **Live Gain Path** (Output Gain slider)
   - Bypasses both debounce and processing thread entirely
   - Direct atomic write; audio callback reads it per-sample
   - Lowest latency (~5 ms, one audio buffer period)

---

## 2. Pipeline Architecture Diagram

```
┌─────────────────────────────────────────────────────────┐
│ MAIN THREAD                                             │
│ (slider changes, debounce timers)                       │
└──┬──────────────────────────────────────────────────────┘
   │
   │ RESYNTHESIZE                    │  REAPPLY EFFECTS
   │ (WORLD sliders changed)         │  (FX sliders changed)
   ▼                                 ▼
┌────────────────────────────────────────────────────────┐
│ PROCESSING THREAD                                       │
│                                                        │
│  Resynthesize:                                         │
│  ┌─────────────────────────────┐                      │
│  │ modifier::apply()           │  (6 transforms)    │
│  │ (pitch, speed, etc.)        │                      │
│  └──────────────┬──────────────┘                      │
│                 ▼                                      │
│  ┌─────────────────────────────┐                      │
│  │ world::synthesize()         │  (C++ FFI)         │
│  └──────────────┬──────────────┘                      │
│                 ▼                                      │
│  ┌─────────────────────────────┐                      │
│  │ apply_fx_chain()            │ ◄──────────────────┐ │
│  │ (EQ, comp, reverb, etc.)    │                    │ │
│  └──────────────┬──────────────┘                     │ │
│                 │                 ReapplyEffects:   │ │
│  ┌──────────────▼──────────────┐                    │ │
│  │ post_world_audio =          │                    │ │
│  │   synthesized output        │  (reads cache) ───┘ │
│  │ (cached for ReapplyEffects) │                      │
│  └──────────────┬──────────────┘                      │
│                 ▼                                      │
│  ┌─────────────────────────────┐                      │
│  │ SynthesisDone(audio)        │  ◄──────┐           │
│  └──────────────┬──────────────┘         │           │
│                 ▼                        │           │
└─────────────────┼────────────────────────┼───────────┘
                  │                        │
                  └────────────┬───────────┘
                               ▼
                   ┌──────────────────────┐
                   │ MAIN THREAD          │
                   │ swap_audio() — O(1)  │
                   │ (RwLock swap)        │
                   └──────────┬───────────┘
                              ▼
                   ┌──────────────────────┐
                   │ AUDIO CALLBACK       │
                   │ (cpal thread)        │
                   │ reads new audio_data │
                   │ from RwLock          │
                   └──────────────────────┘

Live Gain bypasses this entire pipeline:
    MAIN ──LiveGain(linear)──► Arc<AtomicU32> ──► Callback (per-sample)
```

---

## 3. Trigger Paths Summary Table

| Trigger | Panel | Action | Command | Debounce | Processing |
|---------|-------|--------|---------|----------|------------|
| WORLD slider | WorldSliders | `Resynthesize` | `Resynthesize(w, fx)` | 150 ms | Modifier + Synth + FX |
| Effects slider | EffectsSliders | `ReapplyEffects` | `ReapplyEffects(fx)` | 80 ms | FX chain only |
| EQ band Up/Down | EqBands | `ReapplyEffects` | `ReapplyEffects(fx)` | 80 ms | FX chain only |
| Output Gain | Master | `LiveGain(linear)` | — (direct atomic) | None | Callback only |
| WORLD bypass 'w' | (key) | `Resynthesize` | `Resynthesize(w, fx)` | 150 ms | Modifier + Synth + FX |

---

## 4. Debounce Mechanism

Rapid slider adjustments are debounced to avoid excessive processing. The debounce logic lives in the main loop (`src/main.rs`):

### Two Instant Timers

```rust
let mut resynth_pending: Option<Instant> = None;     // WORLD sliders
let mut effects_pending: Option<Instant> = None;     // Effects sliders
```

### Per-Slider-Change: Update Deadline

When the user adjusts a slider (press Up/Down/Left/Right):

1. **WORLD slider** → Set `resynth_pending = Some(Instant::now() + 150ms)`
   - Also clears `effects_pending` (because Resynthesize will re-apply all effects)
2. **Effects slider or EQ band** → Set `effects_pending = Some(Instant::now() + 80ms)`
   - Does NOT touch `resynth_pending`

### Main Loop: Check Timers (Top of Iteration)

```
if let Some(deadline) = resynth_pending {
    if Instant::now() >= deadline {
        dispatch ProcessingCommand::Resynthesize(values, fx)
        resynth_pending = None
        effects_pending = None  // also clear effects (will be done in Resynthesize)
    }
}

if let Some(deadline) = effects_pending {
    if Instant::now() >= deadline {
        dispatch ProcessingCommand::ReapplyEffects(fx)
        effects_pending = None
    }
}
```

### ASCII Sequence: 3 Rapid Slider Moves

```
USER      MAIN LOOP            PROCESSING THREAD
 │            │                        │
 │ adj 1      │                        │
 │──────────►│ resynth_pending=T+150ms │
 │ adj 2      │                        │
 │──────────►│ resynth_pending=T+150ms │ (reset deadline)
 │ adj 3      │                        │
 │──────────►│ resynth_pending=T+150ms │ (reset deadline)
 │            │                        │
 │ 150ms wait │                        │
 │            │──Resynthesize(w,fx)───►│
 │            │                        │ (only 1 synthesis runs)
 │            │◄──SynthesisDone────────│
 │            │ swap_audio()           │
```

This ensures that rapid adjustments result in a single synthesis run with the final slider values, not multiple intermediate runs.

---

## 5. Drain Loop (Stale Command Discarding)

Before processing a synthesis or effects command, the processing thread drains the command queue to keep only the latest parameters:

### Stale Command Discarding Pattern

```rust
// In run_resynthesize or apply_fx_chain:
let (mut latest_world, mut latest_fx) = (initial_world, initial_fx);

while let Ok(cmd) = cmd_rx.try_recv() {
    match cmd {
        ProcessingCommand::Resynthesize(w, fx) => {
            latest_world = w;
            latest_fx = fx;
            // Continue loop to see if there's another one
        }
        ProcessingCommand::ReapplyEffects(fx) => {
            latest_fx = fx;
            // Continue loop
        }
        ProcessingCommand::ScanDirectory(_) |
        ProcessingCommand::PrecheckAudio(_) => {
            // Fast operations, handle inline, continue drain
        }
        ProcessingCommand::Load(_) => {
            // Load aborts the drain loop; return false to start new load
            return false;
        }
        ProcessingCommand::Shutdown => {
            // Handle shutdown
        }
    }
}

// Now process with latest_world and latest_fx (all stale versions discarded)
```

**Benefit**: If the user moves a slider 5 times in quick succession before debounce fires, only the 5th command reaches the processing thread (due to debounce), and if multiple synthesis commands arrive, only the latest is used.

---

## 6. WORLD Resynthesis Path (ProcessingCommand::Resynthesize)

### 6.1 Neutral / Bypass Shortcut

When a Resynthesize command arrives, the processing thread first checks:

```rust
if latest_world.bypass == true || latest_world.is_neutral() {
    // Skip modifier and synthesis; use original_mono directly
    audio_out = original_mono.clone();
} else {
    // Full processing (sections 6.2–6.4)
}
```

**Benefits**:
- Avoids expensive WORLD synthesis when sliders are at defaults
- Avoids modifier parameter calculations
- User hears the original audio with minimal latency

### 6.2 Parameter Modification (modifier::apply)

If not neutral, the modifier applies 6 transforms in sequence:

#### 1. **Pitch Shift** (semitones)
- Scales all voiced f0 frames by `2^(semitones / 12)`
- Unvoiced frames (f0 ≈ 0) remain 0

#### 2. **Pitch Range** (%)
- Expands or compresses the f0 contour around its mean
- Formula: `f0_new = mean + (f0 - mean) * (1 + range / 100)`
- Range 0% = no change; 50% = expands contour; -50% = compresses

#### 3. **Speed** (%)
- Resamples the f0, spectral envelope, and aperiodicity time axis
- Negative speed slows down; positive speeds up
- Internally resamples the 2D arrays via linear interpolation
- **Changes output buffer length** → main thread scales playback position proportionally

#### 4. **Breathiness** (0–1)
- Pushes aperiodicity values toward 1.0 (increasing voicing noise)
- Formula: `ap_new = ap + (1 - ap) * breathiness`
- 0 = no change; 1 = full voicing/noise

#### 5. **Formant Shift** (semitones)
- Warps the frequency axis of the spectrogram
- Shifts all resonances up or down while preserving pitch
- Linear resampling in the frequency domain
- Different from WORLD pitch shift (which shifts pitch, not formants)

#### 6. **Spectral Tilt** (dB/octave)
- Applies a frequency-dependent gain slope across the spectrum
- Positive tilt = boost high frequencies; negative = boost lows
- Implemented as a per-bin gain multiplier

### 6.3 WORLD Synthesis (world_sys::synthesize FFI)

After parameter modification:

```rust
let synthesized = world_sys::synthesize(
    &modified_params,    // WorldParams after 6 transforms
    sample_rate,
)?;  // Result<Vec<f64>, WorldError>
```

- **Input**: Modified `WorldParams` (f0, sp, ap)
- **Output**: Mono f64 audio vector
- **Performance**: ~100–200 ms for 10 seconds of audio
- **Mono guarantee**: WORLD always produces a single channel

On synthesis error, the processing thread sends `Status("Synthesis failed: ...")` and does NOT continue to effects.

### 6.4 Effects Chain (apply_fx_chain)

Applied to the synthesized mono audio (see section 8).

### 6.5 Cache Update

After effects are applied:

```rust
post_world_audio = Some(effects_output.clone());
```

This cached output is used by future `ReapplyEffects` commands to skip re-synthesis.

### ASCII Sequence (WORLD Path, Happy Path)

```
USER      MAIN LOOP            PROCESSING THREAD
 │            │                        │
 │ slider move│                        │
 │──────────►│ Action::Resynthesize    │
 │            │ (150ms debounce)       │
 │            │──Resynthesize(w,fx)───►│
 │            │                        │ drain stale
 │            │                        │ is_neutral() check
 │            │                        │ modifier::apply()
 │            │                        │  - pitch_shift
 │            │                        │  - pitch_range
 │            │                        │  - speed
 │            │                        │  - breathiness
 │            │                        │  - formant_shift
 │            │                        │  - spectral_tilt
 │            │                        │ world::synthesize()
 │            │                        │ apply_fx_chain()
 │            │                        │ post_world_audio update
 │            │◄──SynthesisDone────────│
 │            │ swap_audio() O(1)      │
 │            │ (next loop: render)    │
```

---

## 7. Effects-Only Path (ProcessingCommand::ReapplyEffects)

When an effects slider or EQ band changes:

1. **Drain loop** removes stale `ReapplyEffects` commands, keeping only the latest
2. **Shortcut**: If `post_world_audio.is_none()` (no file loaded), silently return (no-op)
3. **Read cache**: Load `post_world_audio` (already processed by WORLD)
4. **Apply effects chain**: Call `apply_fx_chain(post_world_audio, latest_fx)` with new settings
5. **Update cache**: `post_world_audio = Some(effects_output)` (in case user changed EQ, then WORLD slider; the next Resynthesize will use this cached version)
6. **Send result**: `SynthesisDone(effects_output)`

### ASCII Sequence (Effects Path)

```
USER      MAIN LOOP            PROCESSING THREAD
 │            │                        │
 │ EQ adjust  │                        │
 │──────────►│ Action::ReapplyEffects  │
 │            │ (80ms debounce)        │
 │            │──ReapplyEffects(fx)───►│
 │            │                        │ drain stale
 │            │                        │ read post_world_audio
 │            │                        │ apply_fx_chain()
 │            │                        │ post_world_audio update
 │            │◄──SynthesisDone────────│
 │            │ swap_audio() O(1)      │
```

Latency: ~80 ms debounce + ~10–20 ms effects processing.

---

## 8. The Effects Chain (apply_fx_chain / apply_effects)

Located in `src/dsp/effects.rs`. Applied to **every** synthesized audio (or cached audio for ReapplyEffects).

### Neutral Shortcut

If all effects are at default values:

```rust
if effects_params.is_neutral() {
    return audio.clone();  // Zero DSP, exit early
}
```

### Application Order

Each effect is **conditionally skipped** if at default; otherwise, it processes the audio and passes output to the next stage.

#### 1. **High-Pass Filter (Low Cut)**
- Default: 20 Hz
- Type: Biquad IIR, Butterworth, Q = 0.707
- Removes subsonic rumble

#### 2. **Low-Pass Filter (High Cut)**
- Default: 20000 Hz
- Type: Biquad IIR, Butterworth, Q = 0.707
- Removes ultrasonic noise

#### 3. **Compressor**
- Parameters:
  - Threshold: 0 dB (at threshold, compressor is "off"; below 0 dB activates)
  - Ratio: 4:1 (4 dB reduction per 1 dB above threshold)
  - Attack: 5 ms
  - Release: 50 ms
- Per-sample gain reduction, applied after frequency domain processing

#### 4. **Pitch Shift FX** (Phase Vocoder)
- Default: 0 semitones (off)
- Type: Linear resampling (not WORLD pitch shift)
- **Shifts everything including formants** (unlike WORLD pitch shift, which preserves formants)
- **Resizes buffer**: Output length differs from input if shift != 0
- Main thread scales playback position proportionally on swap

#### 5. **Reverb**
- Type: Schroeder reverberator (classic, low CPU)
- Design: 4 parallel comb filters + 2 cascaded allpass filters
- Parameters:
  - Wet/dry mix: 0.0 = off (dry only); 1.0 = 100% wet
  - Room size, decay time (implicit)
- Adds spatial dimensionality

#### 6. **EQ** (12-Band Graphic)
- **Bands**: 31, 63, 125, 250, 500, 1k, 2k, 3.15k, 4k, 6.3k, 10k, 16k Hz
- **Shelves**:
  - Band 0: Low shelf (gain affects all frequencies ≤ 31 Hz)
  - Bands 1–10: Peaking (parametric, centered on band frequency)
  - Band 11: High shelf (gain affects all frequencies ≥ 16 kHz)
- **Q (bandwidth)**: 1.41 (constant for all bands)
- **Skipped**: Bands at 0 dB are not processed

### Notes

- **Gain (Output Gain) is NOT in the chain**. It is applied live in the audio callback (see section 9).
- **Parameter order is fixed**. Changing the order would alter the sonic result (e.g., EQ before vs. after compression).
- **No hard-clipping** in the chain; soft limiting is implicit in the compressor.

---

## 9. Live Gain Path (Output Gain Slider, Master Panel)

The Output Gain slider bypasses debounce and the processing thread entirely:

### Direct Atomic Write

In the main loop (input handler returns `Action::LiveGain(linear_multiplier)`):

```rust
if let Action::LiveGain(linear) = action {
    playback_state.live_gain.store(linear.to_bits(), Ordering::Relaxed);
    // Immediately applied to audio callback (next buffer period)
}
```

### Audio Callback Read

The cpal callback reads the atomic on every sample:

```rust
while output_has_samples {
    let linear = f32::from_bits(live_gain.load(Ordering::Relaxed));
    output_sample *= linear;
}
```

### Latency

- **Direct feedback**: ≤ 1 audio buffer period (~5 ms at 48 kHz, 256-frame buffer)
- No processing thread involvement
- No debounce

### Export

When exporting to WAV, the live gain is **baked in** by `apply_gain()` at export time. The exported file's samples are multiplied by the gain value.

---

## 10. Audio Buffer Swap (Shared by Resynthesize & ReapplyEffects)

Both `Resynthesize` and `ReapplyEffects` result in `SynthesisDone(audio)`, which is followed by `swap_audio()` in the main thread.

### swap_audio() Implementation

```rust
fn swap_audio(
    audio_lock: &Arc<RwLock<Arc<AudioData>>>,
    new_audio: Arc<AudioData>,
    position: &Arc<AtomicUsize>,
) {
    // Acquire write lock (blocks reader for ~1 audio buffer period)
    let mut lock = audio_lock.write().unwrap();

    // Clamp position inside the lock (prevents TOCTOU race)
    let old_len = lock.samples.len() / lock.channels as usize;
    let new_len = new_audio.samples.len() / new_audio.channels as usize;
    let old_pos = position.load(Ordering::Acquire);
    let clamped = if new_len > 0 {
        (old_pos * old_len / (new_len).max(1)).min(new_len.saturating_sub(1))
    } else {
        0
    };

    // Swap the Arc pointer (O(1) ref count swap, no sample copy)
    *lock = new_audio;
    position.store(clamped, Ordering::Release);
}
```

### When swap_audio() is Called

After `SynthesisDone` is received:

```rust
ProcessingResult::SynthesisDone(new_audio) => {
    if app.ab_original {
        // User is in A/B mode; store the new audio but don't swap
        app.ab_current = new_audio;
    } else {
        // Normal mode; swap immediately
        swap_audio(&playback_state.audio_lock, Arc::new(new_audio), &playback_state.position);
    }
}
```

### Audio Callback During Swap

The audio callback uses a non-blocking read:

```rust
let audio = audio_lock.try_read().ok();

if let Some(audio) = audio {
    // Copy samples normally
} else {
    // Write lock held (during swap); output silence for this buffer only
    output_buffer.fill(0.0);
}
```

This ensures that the user hears silence for one callback period (~5 ms) when a swap occurs, rather than a click or glitch.

---

## 11. A/B Comparison Integration

The A/B toggle (`'a'` key) swaps between `ab_original` (mono original) and `ab_current` (processed audio):

- **Original audio stored** during file load in `original_audio` (from `AnalysisDone`)
- **Processing audio stored** in `audio_data` (from `SynthesisDone` before swap)
- **Toggle swaps** the `Arc<AudioData>` pointer in the audio callback's RwLock
- **Position scales** proportionally if buffer lengths differ (e.g., due to pitch shift)

This is the same swap mechanism as above, so it remains glitch-free and O(1).

---

## 12. Key Data Types (Reference)

### EffectsParams
```rust
pub struct EffectsParams {
    pub gain_db: f32,                    // Master output gain (baked at export)
    pub low_cut_hz: f32,                 // High-pass filter (default 20 Hz)
    pub high_cut_hz: f32,                // Low-pass filter (default 20000 Hz)
    pub compressor_thresh_db: f32,       // Threshold (default 0 dB = off)
    pub reverb_mix: f32,                 // Reverb wet/dry mix (default 0.0)
    pub pitch_shift_semitones: f32,      // Phase vocoder (default 0.0)
    pub eq: EqParams,                    // 12-band graphic EQ
}

pub struct EqParams {
    pub gains: [f32; 12],                // 12 graphic EQ bands in dB (defaults 0.0)
}

impl EffectsParams {
    pub fn is_neutral(&self) -> bool { /* true if all at defaults */ }
}

impl EqParams {
    pub fn is_neutral(&self) -> bool { /* true if all bands at 0 dB */ }
}
```

### WorldSliderValues
```rust
pub struct WorldSliderValues {
    pub pitch_shift_semitones: f32,
    pub pitch_range_percent: f32,
    pub speed_percent: f32,
    pub breathiness: f32,          // 0.0–1.0
    pub formant_shift_semitones: f32,
    pub spectral_tilt_db_per_octave: f32,
    pub bypass: bool,
}

impl WorldSliderValues {
    pub fn is_neutral(&self) -> bool { /* true if all at defaults */ }
}
```

### ProcessingCommand & Result Enums (See FILE_LOADING.md § 8)

---

## 13. Source File References

- **`src/input/handler.rs`** — `effects_slider_action()`, EQ key bindings (Up/Down to adjust gain; Left/Right to select band)
- **`src/app.rs`** — `effects_params()`, `world_slider_values()`, `Action` enum
- **`src/main.rs`** — Debounce timers (`resynth_pending`, `effects_pending`), `LiveGain` dispatch, result drain loop
- **`src/dsp/processing.rs`** — `ProcessingCommand::Resynthesize` / `ReapplyEffects` handlers, `handle_command()`, drain loop implementation, `run_resynthesize()`, `apply_fx_chain()` call site
- **`src/dsp/effects.rs`** — `EffectsParams`, `is_neutral()`, `apply_effects()`, individual effect functions (`apply_low_cut()`, `apply_compressor()`, `apply_reverb()`, `apply_eq()`, etc.)
- **`src/dsp/modifier.rs`** — `WorldSliderValues`, `apply()`, the 6 transform functions
- **`src/audio/playback.rs`** — `write_audio_data()` callback, `swap_audio()`, live gain application

---

## 14. Known Invariants

1. **Debounce prevents excessive synthesis** — Rapid slider moves result in a single synthesis with final values
2. **Drain loop keeps only latest** — Stale parameter versions are discarded before processing
3. **Neutral shortcut skips DSP** — All-defaults results in `audio.clone()` with zero processing
4. **Post-WORLD cache enables fast re-effects** — `ReapplyEffects` skips synthesis by reading cached output
5. **Swap is O(1) and glitch-free** — Arc pointer swap, position clamped inside lock
6. **Live gain is direct and low-latency** — Atomic write/read, no debounce, applied per-sample
7. **Output Gain is applied at export time** — Gain is baked into the WAV file, not stored in the buffer
8. **Silence on swap** — Audio callback outputs silence if RwLock write is held, preventing clicks
9. **Position scaling on buffer resize** — Pitch shift FX or speed slider may change output length; position is scaled proportionally
