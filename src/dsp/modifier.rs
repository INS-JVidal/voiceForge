use world_sys::WorldParams;

/// Slider values for WORLD parameter modification.
#[derive(Debug, Clone, PartialEq)]
pub struct WorldSliderValues {
    /// Pitch shift in semitones.
    pub pitch_shift: f64,
    /// Pitch range scale factor (1.0 = unchanged).
    pub pitch_range: f64,
    /// Speed factor (1.0 = unchanged, 2.0 = double speed).
    pub speed: f64,
    /// Breathiness multiplier (0.0 = unchanged).
    pub breathiness: f64,
    /// Formant shift in semitones.
    pub formant_shift: f64,
    /// Spectral tilt in dB/octave.
    pub spectral_tilt: f64,
}

impl WorldSliderValues {
    /// Check if all sliders are at their neutral (default) positions.
    pub fn is_neutral(&self) -> bool {
        self.pitch_shift == 0.0
            && self.pitch_range == 1.0
            && self.speed == 1.0
            && self.breathiness == 0.0
            && self.formant_shift == 0.0
            && self.spectral_tilt == 0.0
    }
}

impl Default for WorldSliderValues {
    fn default() -> Self {
        Self {
            pitch_shift: 0.0,
            pitch_range: 1.0,
            speed: 1.0,
            breathiness: 0.0,
            formant_shift: 0.0,
            spectral_tilt: 0.0,
        }
    }
}

/// Apply slider-driven modifications to WORLD parameters.
///
/// Returns a new `WorldParams` with modifications applied.
/// The original `params` is not mutated.
pub fn apply(params: &WorldParams, values: &WorldSliderValues) -> WorldParams {
    let mut result = params.clone();

    apply_pitch_shift(&mut result, values.pitch_shift);
    apply_pitch_range(&mut result, values.pitch_range);
    apply_speed(&mut result, values.speed);
    apply_breathiness(&mut result, values.breathiness);
    apply_formant_shift(&mut result, values.formant_shift);
    apply_spectral_tilt(&mut result, values.spectral_tilt);

    result
}

/// Shift f0 by semitones. f0=0 (unvoiced) frames are left unchanged.
fn apply_pitch_shift(params: &mut WorldParams, semitones: f64) {
    if semitones == 0.0 {
        return;
    }
    let ratio = 2.0_f64.powf(semitones / 12.0);
    for f0 in &mut params.f0 {
        if *f0 > 0.0 {
            *f0 *= ratio;
        }
    }
}

/// Expand/compress f0 around its mean. Only affects voiced frames.
fn apply_pitch_range(params: &mut WorldParams, range: f64) {
    if range == 1.0 {
        return;
    }
    // Compute mean of voiced frames.
    let voiced: Vec<f64> = params.f0.iter().copied().filter(|&f| f > 0.0).collect();
    if voiced.is_empty() {
        return;
    }
    let mean = voiced.iter().sum::<f64>() / voiced.len() as f64;

    for f0 in &mut params.f0 {
        if *f0 > 0.0 {
            *f0 = mean + (*f0 - mean) * range;
            if *f0 < 0.0 {
                *f0 = 0.0;
            }
        }
    }
}

/// Resample frames via linear interpolation to change speed.
/// speed > 1.0 = fewer frames (faster), speed < 1.0 = more frames (slower).
fn apply_speed(params: &mut WorldParams, speed: f64) {
    if speed == 1.0 {
        return;
    }

    let old_len = params.f0.len();
    if old_len == 0 {
        return;
    }
    let new_len = ((old_len as f64) / speed).round().max(1.0) as usize;

    params.f0 = resample_1d(&params.f0, new_len);
    params.temporal_positions = (0..new_len)
        .map(|i| i as f64 * params.frame_period / 1000.0)
        .collect();
    params.spectrogram = resample_2d(&params.spectrogram, new_len);
    params.aperiodicity = resample_2d(&params.aperiodicity, new_len);
}

/// Increase aperiodicity to add breathiness.
fn apply_breathiness(params: &mut WorldParams, amount: f64) {
    if amount == 0.0 {
        return;
    }
    for row in &mut params.aperiodicity {
        for val in row.iter_mut() {
            // Aperiodicity is in [0, 1] range (or close). Increase towards 1.
            *val = (*val + amount).clamp(0.0, 1.0);
        }
    }
}

/// Warp the spectrogram frequency axis to shift formants.
fn apply_formant_shift(params: &mut WorldParams, semitones: f64) {
    if semitones == 0.0 {
        return;
    }
    let ratio = 2.0_f64.powf(semitones / 12.0);
    let sp_width = params.fft_size / 2 + 1;

    for row in &mut params.spectrogram {
        let original = row.clone();
        for (i, bin) in row.iter_mut().enumerate().take(sp_width) {
            // Map destination bin i to source bin.
            let src = i as f64 / ratio;
            let src_floor = src.floor() as usize;
            let frac = src - src_floor as f64;

            if src_floor + 1 < sp_width {
                *bin = original[src_floor] * (1.0 - frac) + original[src_floor + 1] * frac;
            } else if src_floor < sp_width {
                *bin = original[src_floor];
            } else {
                // Beyond the original spectrum — use the last bin value.
                *bin = original[sp_width - 1];
            }
        }
    }
}

/// Apply a spectral tilt (dB per octave slope) across frequency bins.
fn apply_spectral_tilt(params: &mut WorldParams, tilt_db_per_oct: f64) {
    if tilt_db_per_oct == 0.0 {
        return;
    }
    let sp_width = params.fft_size / 2 + 1;
    if sp_width < 2 {
        return;
    }

    // Tilt is applied relative to bin 1 (lowest non-DC bin) to avoid log2(0).
    // Each bin represents frequency bin i ~ i * (sr/fft_size).
    // gain_db = tilt * log2(bin_index / ref_bin) for each bin.
    let ref_bin = 1.0_f64;

    for row in &mut params.spectrogram {
        for (i, bin) in row.iter_mut().enumerate().take(sp_width).skip(1) {
            let octaves = (i as f64 / ref_bin).log2();
            let gain_db = tilt_db_per_oct * octaves;
            let gain_linear = 10.0_f64.powf(gain_db / 20.0);
            // Spectrogram values are power spectra, so apply gain squared.
            *bin *= gain_linear * gain_linear;
        }
    }
}

/// Linearly resample a 1D vector to a new length.
fn resample_1d(data: &[f64], new_len: usize) -> Vec<f64> {
    if new_len == 0 {
        return Vec::new();
    }
    if data.len() <= 1 || new_len == 1 {
        return vec![data.first().copied().unwrap_or(0.0); new_len];
    }

    let old_len = data.len();
    (0..new_len)
        .map(|i| {
            let t = i as f64 * (old_len - 1) as f64 / (new_len - 1) as f64;
            let lo = t.floor() as usize;
            let hi = (lo + 1).min(old_len - 1);
            let frac = t - lo as f64;
            data[lo] * (1.0 - frac) + data[hi] * frac
        })
        .collect()
}

/// Linearly resample a 2D vector (rows) to a new row count.
fn resample_2d(data: &[Vec<f64>], new_len: usize) -> Vec<Vec<f64>> {
    if new_len == 0 || data.is_empty() {
        return Vec::new();
    }
    let width = data[0].len();
    let old_len = data.len();

    if old_len == 1 || new_len == 1 {
        return vec![data[0].clone(); new_len];
    }

    (0..new_len)
        .map(|i| {
            let t = i as f64 * (old_len - 1) as f64 / (new_len - 1) as f64;
            let lo = t.floor() as usize;
            let hi = (lo + 1).min(old_len - 1);
            let frac = t - lo as f64;
            (0..width)
                .map(|j| data[lo][j] * (1.0 - frac) + data[hi][j] * frac)
                .collect()
        })
        .collect()
}
