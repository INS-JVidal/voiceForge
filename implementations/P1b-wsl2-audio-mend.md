# P1b — WSL2 Audio Configuration Mend: Implementation Report

## Scope

Configure WSL2 so that cpal (ALSA backend) can output audio through WSLg's PulseAudio server to Windows host speakers. No code changes — purely system configuration and project documentation.

## Problem

cpal uses ALSA on Linux. In WSL2 there is no physical sound card, so `snd_pcm_open("default")` failed with "Unknown PCM default". The app decoded audio successfully but exited with:

```
Failed to start playback: playback error: no supported output config:
The requested device is no longer available.
```

## Root Cause

The audio chain `cpal → ALSA → hardware` had no hardware endpoint. WSLg already provided a PulseAudio server at `/mnt/wslg/PulseServer` with `PULSE_SERVER` exported, but ALSA had no plugin to route through it.

## Environment (Before)

| Component | State |
|---|---|
| Kernel | Linux 6.6.87.2-microsoft-standard-WSL2 |
| WSLg | Active (`/mnt/wslg/` present, PulseAudio socket at `/mnt/wslg/PulseServer`) |
| `PULSE_SERVER` | `unix:/mnt/wslg/PulseServer` (already set) |
| `libpulse0` | Installed |
| `libasound2-plugins` | **Not installed** |
| `pulseaudio-utils` | **Not installed** |
| `~/.asoundrc` | **Did not exist** |

## What Was Done

### Step 1: Installed ALSA PulseAudio plugin

```bash
sudo apt install -y libasound2-plugins pulseaudio-utils
```

- `libasound2-plugins` — provides `libasound_module_pcm_pulse.so`, the ALSA→PulseAudio bridge
- `pulseaudio-utils` — provides `pactl` and `paplay` for diagnostics

### Step 2: Created ALSA configuration

Created `~/.asoundrc`:

```
pcm.default pulse
ctl.default pulse
```

Routes ALSA's default PCM device and mixer control through PulseAudio. User-level config, no sudo required.

### Step 3: Updated project documentation

Added WSL2 audio setup commands to `CLAUDE.md` under the Build Commands / System Dependencies section, so future sessions have the instructions.

## Verification Results

| Check | Result |
|---|---|
| `pactl info` | Connected — `Server Name: pulseaudio`, `Default Sink: RDPSink`, `Default Sample Specification: s16le 2ch 44100Hz` |
| `paplay test_stereo.wav` | Audio played through Windows speakers |
| `cargo run -- test_stereo.wav` | cpal device opened successfully, playback started, interactive controls working |

## Audio Chain (After)

```
cpal → ALSA → libasound_module_pcm_pulse.so → PulseAudio (WSLg) → RDP audio → Windows host audio
```

## Files Modified

| File | Change |
|---|---|
| `~/.asoundrc` | Created: ALSA default PCM/CTL routed to PulseAudio |
| `CLAUDE.md` | Added WSL2 audio setup instructions to Build Commands section |

## Files Not Modified

No source code changes. The playback engine (`src/audio/playback.rs`) and CLI player (`src/main.rs`) worked correctly once the system audio was configured.

## Notes for Future Reference

- **Native Linux**: `libasound2-plugins` is typically pre-installed by desktop environments. No `~/.asoundrc` needed if PulseAudio is the system default.
- **CI/headless**: All existing tests (15/15) pass without audio hardware. No playback tests exist. If added later, gate with `#[ignore]`.
- **cpal backend**: cpal on Linux only supports ALSA (no native PulseAudio backend). The ALSA→PulseAudio bridge is the standard approach used by most Linux audio applications.
- **Graceful degradation**: Currently the app exits on playback failure. When the TUI is implemented (P2+), consider degrading gracefully — launch without playback, show "No audio device" in status bar.
