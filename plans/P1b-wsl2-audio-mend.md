# P1b — WSL2 Audio Configuration Mend

## Problem

cpal uses the ALSA backend on Linux. In WSL2, there is no physical ALSA sound card — `snd_pcm_open("default")` fails with "Unknown PCM default". However, WSLg provides a PulseAudio server at `/mnt/wslg/PulseServer` (socket), and `PULSE_SERVER=unix:/mnt/wslg/PulseServer` is already set in the environment. The missing piece is the ALSA-to-PulseAudio bridge that routes ALSA calls through PulseAudio.

## Current Environment

- **Kernel**: Linux 6.6.87.2-microsoft-standard-WSL2
- **WSLg**: Active — `/mnt/wslg/` exists, PulseAudio socket present
- **PULSE_SERVER**: `unix:/mnt/wslg/PulseServer` (already exported)
- **PulseAudio client lib**: `libpulse0` installed
- **PulseAudio tools**: `pactl` / `pulseaudio` CLI **not** installed
- **ALSA plugins**: `libasound2-plugins` **not** installed
- **ALSA config**: No `/etc/asound.conf` or `~/.asoundrc` exists

## Root Cause

cpal → ALSA → `snd_pcm_open("default")` → ALSA looks for hardware card 0 → none exists in WSL2. The fix is to install `libasound2-plugins` (provides `libasound_module_pcm_pulse.so`) and configure ALSA's default PCM to route through PulseAudio.

## Plan

### Step 1: Install ALSA PulseAudio plugin

```bash
sudo apt install libasound2-plugins
```

This provides the `pulse` PCM plugin for ALSA, allowing ALSA applications to output through PulseAudio transparently.

### Step 2: Create ALSA configuration

Create `~/.asoundrc` (user-level, no sudo):

```
pcm.default pulse
ctl.default pulse
```

This tells ALSA to route its default PCM device and mixer control through PulseAudio. Since `PULSE_SERVER` is already set to the WSLg socket, PulseAudio will forward audio to the Windows host audio.

### Step 3: Install PulseAudio CLI tools (optional but recommended)

```bash
sudo apt install pulseaudio-utils
```

Provides `pactl` and `paplay` for diagnostics:
- `pactl info` — verify PulseAudio connection
- `paplay test.wav` — verify audio reaches Windows speakers

### Step 4: Verify the audio chain

```bash
# 1. Verify PulseAudio connection
pactl info

# 2. Verify ALSA sees PulseAudio as default
aplay -L | head -20

# 3. Test with a WAV file via ALSA
aplay assets/test_samples/test_stereo.wav

# 4. Test with voiceforge
cargo run -- assets/test_samples/test_stereo.wav
```

Expected: audio plays through Windows host speakers/headphones.

### Step 5: Update project documentation

Add a WSL2 audio setup section to `CLAUDE.md` under System Dependencies, so future sessions know the configuration:

```markdown
## WSL2 Audio Setup (Required for playback)

WSLg provides PulseAudio forwarding. ALSA needs to be configured to route through it:

    sudo apt install libasound2-plugins pulseaudio-utils
    echo -e "pcm.default pulse\nctl.default pulse" > ~/.asoundrc

Verify: `pactl info` should show "Server Name: pulseaudio" and `cargo run -- file.wav` should produce audio.
```

## Robustness Considerations

### Fallback when no audio device is available

The current `main.rs` exits with `process::exit(1)` when playback fails. This is correct for a CLI player, but as the project evolves into a TUI workbench (P2+), the app should still launch without audio — users may want to analyze/modify parameters even without playback. Consider:

- **P1b scope (now)**: Keep current behavior (exit with clear error). Add the WSL2 setup instructions to the error message so users know how to fix it.
- **P2+ scope (later)**: Degrade gracefully — start the TUI without audio, disable playback controls, show "No audio device" in status bar.

### ALSA config portability

`~/.asoundrc` is user-level and doesn't affect the system. On native Linux with real ALSA hardware, the PulseAudio plugin is typically already configured by the desktop environment. The `pulse` PCM type gracefully falls back if PulseAudio isn't running — ALSA will report "Connection refused" rather than crashing.

### cpal backend selection

cpal on Linux always uses ALSA (no PulseAudio backend). The ALSA→PulseAudio bridge via `libasound2-plugins` is the standard approach and is how most Linux desktop apps work. No code changes needed in voiceforge.

### CI/headless environments

Tests that don't involve playback (`test_decoder_*`, `test_world_ffi_*`) already work without audio hardware. No playback tests exist currently, so CI is unaffected. If playback integration tests are added later, they should be gated behind a `#[ignore]` attribute or feature flag.

## Integration with Existing Code

- **No code changes required** — this is purely a system configuration task
- **Error message improvement** (optional): Enhance the `PlaybackError` message in `main.rs` to suggest WSL2 setup steps when running under WSL2
- **CLAUDE.md update**: Add WSL2 audio setup to the build instructions so the project is self-documenting

## Checklist

- [ ] `sudo apt install libasound2-plugins` succeeds
- [ ] `~/.asoundrc` created with pulse defaults
- [ ] `pactl info` shows PulseAudio connected (after installing `pulseaudio-utils`)
- [ ] `aplay assets/test_samples/test_stereo.wav` produces audio on Windows speakers
- [ ] `cargo run -- assets/test_samples/test_stereo.wav` plays audio with controls working
- [ ] Space toggles play/pause, `[`/`]` seek, `q` quits cleanly
- [ ] CLAUDE.md updated with WSL2 audio setup instructions
