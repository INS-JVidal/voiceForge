#!/usr/bin/env python3
"""Generate test audio assets for VoiceForge."""

import wave
import struct
import math
import random
import os

SAMPLE_RATE = 44100
MAX_AMP = 32767  # 16-bit signed max

OUT_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), "assets", "test_samples")
os.makedirs(OUT_DIR, exist_ok=True)


def write_wav(filename, samples, channels=1, sample_rate=SAMPLE_RATE):
    path = os.path.join(OUT_DIR, filename)
    with wave.open(path, "w") as w:
        w.setnchannels(channels)
        w.setsampwidth(2)  # 16-bit
        w.setframerate(sample_rate)
        data = b""
        for s in samples:
            clamped = max(-1.0, min(1.0, s))
            data += struct.pack("<h", int(clamped * MAX_AMP))
        w.writeframes(data)
    print(f"  {filename} ({len(samples) // channels} frames, {channels}ch)")


def sine(freq, t):
    return math.sin(2.0 * math.pi * freq * t)


# --- 1. Pure 440Hz sine, 1s mono ---
def gen_sine_440():
    n = SAMPLE_RATE
    return [sine(440, i / SAMPLE_RATE) * 0.8 for i in range(n)]


# --- 2. Log sweep 20Hz-20kHz, 5s mono ---
def gen_sweep():
    n = SAMPLE_RATE * 5
    samples = []
    for i in range(n):
        t = i / SAMPLE_RATE
        # Logarithmic frequency sweep
        freq = 20.0 * (20000.0 / 20.0) ** (t / 5.0)
        phase = 2.0 * math.pi * 20.0 * 5.0 / math.log(20000.0 / 20.0) * ((20000.0 / 20.0) ** (t / 5.0) - 1.0)
        samples.append(math.sin(phase) * 0.8)
    return samples


# --- 3. Simulated voice, 3s mono ---
def gen_voice_simulated():
    n = SAMPLE_RATE * 3
    f0 = 150.0
    samples = []
    for i in range(n):
        t = i / SAMPLE_RATE
        s = 0.0
        # Fundamental + harmonics with decreasing amplitude
        for h in range(1, 8):
            amp = 1.0 / h
            s += amp * sine(f0 * h, t)
        # Add slight noise for breathiness
        s += random.uniform(-0.05, 0.05)
        # Normalize
        samples.append(s * 0.15)
    return samples


# --- 4. Silence, 1s mono ---
def gen_silence():
    return [0.0] * SAMPLE_RATE


# --- 5. Stereo tone, 2s (440Hz left, 880Hz right) ---
def gen_stereo():
    n = SAMPLE_RATE * 2
    samples = []
    for i in range(n):
        t = i / SAMPLE_RATE
        left = sine(440, t) * 0.8
        right = sine(880, t) * 0.8
        samples.append(left)
        samples.append(right)
    return samples


# --- 6. C major chord with envelope, 3s mono ---
def gen_chord():
    n = SAMPLE_RATE * 3
    samples = []
    for i in range(n):
        t = i / SAMPLE_RATE
        # ADSR-ish envelope
        if t < 0.1:
            env = t / 0.1
        elif t < 0.3:
            env = 1.0 - 0.3 * (t - 0.1) / 0.2
        elif t < 2.5:
            env = 0.7
        else:
            env = 0.7 * (1.0 - (t - 2.5) / 0.5)
        env = max(0.0, env)
        s = sine(261.63, t) + sine(329.63, t) + sine(392.0, t)
        samples.append(s * env * 0.25)
    return samples


# --- 7. White noise, 2s mono ---
def gen_noise():
    n = SAMPLE_RATE * 2
    return [random.uniform(-0.8, 0.8) for _ in range(n)]


# --- 8. Speech-like signal, 5s mono ---
def gen_speech_like():
    n = SAMPLE_RATE * 5
    samples = []
    random.seed(42)  # Reproducible
    for i in range(n):
        t = i / SAMPLE_RATE
        # Time-varying pitch: 100-200Hz with slow modulation
        f0 = 150.0 + 50.0 * math.sin(2.0 * math.pi * 0.5 * t)
        # Add vibrato
        f0 += 3.0 * math.sin(2.0 * math.pi * 5.5 * t)
        # Harmonics
        s = 0.0
        phase_base = 0.0
        for h in range(1, 10):
            amp = 1.0 / (h ** 1.2)
            # Formant-like resonances at ~500Hz, ~1500Hz, ~2500Hz
            freq = f0 * h
            formant_boost = 0.0
            for fc in [500, 1500, 2500]:
                formant_boost += 0.5 * math.exp(-((freq - fc) ** 2) / (200 ** 2))
            s += amp * (1.0 + formant_boost) * sine(f0 * h, t)
        # Amplitude modulation (syllable-like rhythm at ~4Hz)
        am = 0.5 + 0.5 * math.sin(2.0 * math.pi * 4.0 * t)
        # Breathy noise component
        noise = random.uniform(-0.03, 0.03)
        samples.append((s * am * 0.1) + noise)
    return samples


print("Generating test audio assets...")
write_wav("sine_440hz_1s.wav", gen_sine_440())
write_wav("sine_sweep_5s.wav", gen_sweep())
write_wav("voice_simulated_3s.wav", gen_voice_simulated())
write_wav("silence_1s.wav", gen_silence())
write_wav("stereo_tone_2s.wav", gen_stereo(), channels=2)
write_wav("complex_chord_3s.wav", gen_chord())
write_wav("noise_white_2s.wav", gen_noise())
write_wav("speech_like_5s.wav", gen_speech_like())
print("Done.")
