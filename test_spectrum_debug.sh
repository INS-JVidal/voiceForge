#!/bin/bash
# Quick test script for spectrum debugging

set -e

echo "========================================="
echo "VoiceForge Spectrum Visualizer Debug Test"
echo "========================================="
echo ""

# Check if audio file provided
if [ $# -eq 0 ]; then
    echo "Usage: ./test_spectrum_debug.sh <audio_file>"
    echo ""
    echo "Example:"
    echo "  ./test_spectrum_debug.sh ~/Music/song.wav"
    echo ""
    echo "Supported formats: WAV, MP3, FLAC"
    exit 1
fi

AUDIO_FILE="$1"

if [ ! -f "$AUDIO_FILE" ]; then
    echo "Error: File not found: $AUDIO_FILE"
    exit 1
fi

echo "Building voiceforge with debug instrumentation..."
cargo build 2>&1 | grep -E "Compiling voiceforge|Finished"

echo ""
echo "Starting app with audio file: $AUDIO_FILE"
echo ""
echo "Press Space to play (watch stderr for debug messages)"
echo "Press q to quit"
echo ""
echo "Debug output will show:"
echo "  [SPECTRUM_INIT] - Terminal detection"
echo "  [SPECTRUM] - Spectrum monitoring (every ~1 second)"
echo "  [SPECTRUM_RENDER] - Which rendering path (GPU vs fallback)"
echo "  [SPECTRUM_IMAGE] - Image generation details"
echo ""
echo "========================================="
echo ""

cargo run "$AUDIO_FILE" 2>&1 | tee spectrum_test_$(date +%s).log

echo ""
echo "========================================="
echo "Test complete. Debug output saved to spectrum_test_*.log"
echo "========================================="
