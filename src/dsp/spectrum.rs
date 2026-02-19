use crate::audio::decoder::AudioData;
use rustfft::num_complex::Complex;
use rustfft::FftPlanner;
use std::sync::Arc;

/// FFT window size used for spectrum analysis.
pub const FFT_SIZE: usize = 2048;

/// Cached FFT plan for repeated spectrum computation.
struct CachedFft {
    fft: Arc<dyn rustfft::Fft<f32>>,
    size: usize,
}

thread_local! {
    static FFT_CACHE: std::cell::RefCell<Option<CachedFft>> = const { std::cell::RefCell::new(None) };
}

fn get_fft(size: usize) -> Arc<dyn rustfft::Fft<f32>> {
    FFT_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(ref cached) = *cache {
            if cached.size == size {
                return Arc::clone(&cached.fft);
            }
        }
        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(size);
        *cache = Some(CachedFft {
            fft: Arc::clone(&fft),
            size,
        });
        fft
    })
}

pub fn compute_spectrum(samples: &[f32], fft_size: usize) -> Vec<f32> {
    if fft_size < 2 {
        return Vec::new();
    }

    let mut buffer: Vec<Complex<f32>> = (0..fft_size)
        .map(|i| {
            let s = if i < samples.len() { samples[i] } else { 0.0 };
            // Hann window
            let w = 0.5
                * (1.0
                    - (2.0 * std::f32::consts::PI * i as f32 / (fft_size - 1) as f32).cos());
            Complex::new(s * w, 0.0)
        })
        .collect();

    let fft = get_fft(fft_size);
    fft.process(&mut buffer);

    let bin_count = fft_size / 2;
    buffer[..bin_count]
        .iter()
        .map(|c| {
            (20.0 * (c.norm() / (fft_size as f32).sqrt()).max(1e-10).log10()).clamp(-80.0, 0.0)
        })
        .collect()
}

/// Extract `size` mono samples from `audio` at interleaved position `pos`.
/// Downmixes channels by averaging. Zero-pads beyond buffer end.
pub fn extract_window(audio: &AudioData, pos: usize, size: usize) -> Vec<f32> {
    let ch = audio.channels as usize;
    if ch == 0 {
        return vec![0.0; size];
    }
    let total_frames = audio.samples.len() / ch;
    let frame_start = pos / ch;
    (0..size)
        .map(|i| {
            let frame = frame_start + i;
            if frame < total_frames {
                let sum: f32 = (0..ch).map(|c| audio.samples[frame * ch + c]).sum();
                sum / ch as f32
            } else {
                0.0
            }
        })
        .collect()
}
