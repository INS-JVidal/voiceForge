//! Diagnostic test for spectrum visualization
//! Tests audio loading, spectrum computation, and image generation

use std::path::Path;
use voiceforge::audio::decoder::decode_file;
use voiceforge::dsp::spectrum::{compute_spectrum, extract_window, FFT_SIZE};
use voiceforge::ui::spectrum::spectrum_to_image;

#[test]
fn test_spectrum_visualization_pipeline() {
    println!("\n═══════════════════════════════════════════════════════════════");
    println!("SPECTRUM VISUALIZATION DIAGNOSTIC TEST");
    println!("═══════════════════════════════════════════════════════════════\n");

    // Test 1: Load audio file
    println!("📁 TEST 1: Load Audio File");
    println!("─────────────────────────────────────────────────────────────────");

    let test_file = Path::new("assets/test_samples/sine_sweep_5s.wav");
    if !test_file.exists() {
        println!("❌ FAIL: Test audio file not found: {:?}", test_file);
        panic!("Missing test audio file");
    }

    let audio = match decode_file(test_file) {
        Ok(a) => {
            println!("✅ SUCCESS: Loaded {:?}", test_file.file_name().unwrap());
            println!("   - Sample rate: {} Hz", a.sample_rate);
            println!("   - Channels: {}", a.channels);
            println!("   - Samples: {}", a.samples.len());
            println!("   - Duration: {:.3}s", a.duration_secs());
            a
        }
        Err(e) => {
            println!("❌ FAIL: Could not decode audio: {}", e);
            panic!("Audio decode failed");
        }
    };

    // Test 2: Extract and compute spectrum
    println!("\n📊 TEST 2: Spectrum Computation");
    println!("─────────────────────────────────────────────────────────────────");

    // Test at different points in the audio
    let test_positions = vec![
        ("Start (0s)", 0),
        ("Middle (2.5s)", (audio.sample_rate as usize * 2 * audio.channels as usize) + (audio.sample_rate as usize / 2)),
        ("End (4.9s)", audio.samples.len() - FFT_SIZE * audio.channels as usize),
    ];

    for (label, pos) in test_positions {
        let window = extract_window(&audio, pos, FFT_SIZE);
        let spectrum = compute_spectrum(&window, FFT_SIZE);

        let max_db = spectrum.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        let min_db = spectrum.iter().cloned().fold(f32::INFINITY, f32::min);
        let mean_db = spectrum.iter().sum::<f32>() / spectrum.len() as f32;

        println!("✅ {} @ sample {}", label, pos);
        println!("   - Bins: {}", spectrum.len());
        println!("   - Max dB: {:.1}", max_db);
        println!("   - Min dB: {:.1}", min_db);
        println!("   - Mean dB: {:.1}", mean_db);

        // Check if spectrum is reasonable
        if max_db < -80.0 {
            println!("   ⚠️  WARNING: Max amplitude very low (below -80dB)");
        } else if max_db > 0.0 {
            println!("   ⚠️  WARNING: Max amplitude exceeds 0dB (possible clipping)");
        } else {
            println!("   ✓ Amplitude in expected range [-80dB, 0dB]");
        }
    }

    // Test 3: Image generation
    println!("\n🎨 TEST 3: Image Generation (GPU Spectrum)");
    println!("─────────────────────────────────────────────────────────────────");

    let window = extract_window(&audio, 0, FFT_SIZE);
    let spectrum = compute_spectrum(&window, FFT_SIZE);

    let img_width = 256u32;
    let img_height = 128u32;

    println!("Generating {}x{} pixel image from {} spectrum bins...",
        img_width, img_height, spectrum.len());

    let img = spectrum_to_image(&spectrum, img_width, img_height);

    // Analyze image
    let mut colored_pixels = 0u32;
    let mut black_pixels = 0u32;
    let mut red_pixels = 0u32;
    let mut green_pixels = 0u32;
    let mut blue_pixels = 0u32;

    for pixel in img.pixels() {
        let r = pixel.0[0];
        let g = pixel.0[1];
        let b = pixel.0[2];

        if r == 0 && g == 0 && b == 0 {
            black_pixels += 1;
        } else {
            colored_pixels += 1;
            // Track dominant color
            if r > g && r > b {
                red_pixels += 1;
            } else if g > r && g > b {
                green_pixels += 1;
            } else if b > r && b > g {
                blue_pixels += 1;
            }
        }
    }

    let total_pixels = img_width * img_height;

    println!("✅ Image generated: {}x{} pixels", img_width, img_height);
    println!("   - Total pixels: {}", total_pixels);
    println!("   - Colored pixels: {} ({:.1}%)", colored_pixels,
        (colored_pixels as f32 / total_pixels as f32) * 100.0);
    println!("   - Black pixels: {} ({:.1}%)", black_pixels,
        (black_pixels as f32 / total_pixels as f32) * 100.0);
    println!("   - Red (top): {}", red_pixels);
    println!("   - Blue (mid): {}", blue_pixels);
    println!("   - Green (bottom): {}", green_pixels);

    // Diagnose
    println!("\n🔍 DIAGNOSIS:");
    println!("─────────────────────────────────────────────────────────────────");

    let max_db = spectrum.iter().cloned().fold(f32::NEG_INFINITY, f32::max);

    if colored_pixels == 0 {
        println!("❌ PROBLEM: All-black image generated");
        println!("   Possible causes:");
        println!("   - Audio spectrum max_db={:.1}dB (floor is -50dB)", max_db);
        println!("   - Audio might be silent or amplitude too low");
        println!("   - Try loading a louder file like complex_chord_3s.wav");
    } else if colored_pixels < total_pixels / 10 {
        println!("⚠️  WARNING: Very sparse colored pixels ({:.1}% filled)",
            (colored_pixels as f32 / total_pixels as f32) * 100.0);
        println!("   Audio spectrum is quiet (max_db={:.1}dB)", max_db);
    } else {
        println!("✅ SUCCESS: Spectrum visualization working!");
        println!("   - Image contains {:.1}% colored pixels",
            (colored_pixels as f32 / total_pixels as f32) * 100.0);
        println!("   - Smooth gradient should be visible in terminal");
        println!("   - Spectrum ranges from violet (low) → pink (high)");
    }

    println!("\n═══════════════════════════════════════════════════════════════");
    println!("TEST COMPLETE");
    println!("═══════════════════════════════════════════════════════════════\n");

    // Assertions
    assert!(!spectrum.is_empty(), "Spectrum should have bins");
    assert!(colored_pixels > 0, "Image should contain colored pixels");
    assert!(max_db > -80.0, "Audio spectrum max should be above -80dB");
}

#[test]
fn test_spectrum_with_different_files() {
    println!("\n═══════════════════════════════════════════════════════════════");
    println!("MULTI-FILE SPECTRUM TEST");
    println!("═══════════════════════════════════════════════════════════════\n");

    let test_files = vec![
        ("sine_440hz_1s.wav", "Pure 440Hz sine wave"),
        ("sine_sweep_5s.wav", "Frequency sweep 20Hz-20kHz"),
        ("noise_white_2s.wav", "White noise"),
        ("complex_chord_3s.wav", "Complex chord"),
    ];

    for (filename, description) in test_files {
        let path = Path::new("assets/test_samples").join(filename);
        if !path.exists() {
            println!("⏭️  SKIP: {} (file not found)", filename);
            continue;
        }

        println!("📁 Testing: {}", filename);
        println!("   Description: {}", description);

        match decode_file(&path) {
            Ok(audio) => {
                let window = extract_window(&audio, 0, FFT_SIZE);
                let spectrum = compute_spectrum(&window, FFT_SIZE);
                let max_db = spectrum.iter().cloned().fold(f32::NEG_INFINITY, f32::max);

                // Generate image
                let img = spectrum_to_image(&spectrum, 256, 128);
                let colored = img.pixels()
                    .filter(|p| p.0[0] > 0 || p.0[1] > 0 || p.0[2] > 0)
                    .count();

                println!("   ✅ Loaded: {} samples", audio.samples.len());
                println!("   - Max dB: {:.1}", max_db);
                println!("   - Image fill: {:.1}%",
                    (colored as f32 / (256.0 * 128.0)) * 100.0);

                if max_db > -50.0 && colored > 100 {
                    println!("   ✓ Good spectrum visualization expected");
                } else if max_db > -80.0 {
                    println!("   ~ Minimal visualization (quiet audio)");
                } else {
                    println!("   ⚠️  Poor visualization (very quiet)");
                }
            }
            Err(e) => {
                println!("   ❌ Error: {}", e);
            }
        }
        println!();
    }

    println!("═══════════════════════════════════════════════════════════════\n");
}
