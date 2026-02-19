use std::f32::consts::PI;

/// Parameters for the 12-band graphic EQ.
#[derive(Debug, Clone, PartialEq)]
pub struct EqParams {
    pub gains: [f32; 12],
}

impl Default for EqParams {
    fn default() -> Self {
        Self { gains: [0.0; 12] }
    }
}

impl EqParams {
    /// True when all EQ bands are at 0 dB (neutral).
    pub fn is_neutral(&self) -> bool {
        self.gains.iter().all(|&g| g.abs() < 1e-6)
    }
}

/// Parameters for the post-WORLD effects chain.
#[derive(Debug, Clone, PartialEq)]
pub struct EffectsParams {
    pub gain_db: f32,
    pub low_cut_hz: f32,
    pub high_cut_hz: f32,
    pub compressor_thresh_db: f32,
    pub reverb_mix: f32,
    pub pitch_shift_semitones: f32,
    pub eq: EqParams,
}

impl Default for EffectsParams {
    fn default() -> Self {
        Self {
            gain_db: 0.0,
            low_cut_hz: 20.0,
            high_cut_hz: 20000.0,
            compressor_thresh_db: 0.0,
            reverb_mix: 0.0,
            pitch_shift_semitones: 0.0,
            eq: EqParams::default(),
        }
    }
}

impl EffectsParams {
    /// True when all processing-thread effects are at their default (bypass) values.
    /// Note: gain is excluded — it is applied live in the audio callback.
    /// M-5: Use epsilon for reverb_mix and pitch_shift to be robust against float drift.
    pub fn is_neutral(&self) -> bool {
        self.low_cut_hz <= 20.0
            && self.high_cut_hz >= 20000.0
            && self.compressor_thresh_db >= 0.0
            && self.reverb_mix.abs() < 1e-6
            && self.pitch_shift_semitones.abs() < 1e-6
            && self.eq.is_neutral()
    }
}

/// Apply the full effects chain in order: gain → highpass → lowpass →
/// compressor → pitch shift → reverb → EQ.  Returns a new buffer.
pub fn apply_effects(samples: &[f32], sample_rate: u32, params: &EffectsParams) -> Vec<f32> {
    if params.is_neutral() || samples.is_empty() || sample_rate == 0 {
        return samples.to_vec();
    }

    let mut buf = samples.to_vec();

    // 1. Gain — applied live in audio callback, skipped here.

    // 2. High-pass (low cut)
    if params.low_cut_hz > 20.0 {
        apply_biquad(&mut buf, sample_rate, BiquadType::Highpass, params.low_cut_hz);
    }

    // 3. Low-pass (high cut)
    if params.high_cut_hz < 20000.0 {
        apply_biquad(&mut buf, sample_rate, BiquadType::Lowpass, params.high_cut_hz);
    }

    // 4. Compressor
    if params.compressor_thresh_db < 0.0 {
        apply_compressor(&mut buf, params.compressor_thresh_db, sample_rate);
    }

    // 5. Pitch shift (FX) — may change buffer length
    if params.pitch_shift_semitones != 0.0 {
        buf = apply_pitch_shift(&buf, params.pitch_shift_semitones);
    }

    // 6. Reverb
    if params.reverb_mix > 0.0 {
        buf = apply_reverb(&buf, sample_rate, params.reverb_mix);
    }

    // 7. EQ (final stage)
    apply_eq(&mut buf, sample_rate, &params.eq);

    buf
}

// ── Gain ────────────────────────────────────────────────────────────────

/// Apply gain in dB to a sample buffer. Public for use in WAV export and tests.
/// M-10: Does not clamp — caller is responsible for clamping if needed
/// (export_wav clamps via `s.clamp(-1.0, 1.0)`, audio callback clamps after gain).
pub fn apply_gain(samples: &mut [f32], gain_db: f32) {
    let linear = 10.0_f32.powf(gain_db / 20.0);
    for s in samples.iter_mut() {
        *s *= linear;
    }
}

// ── Biquad filter (cookbook) ─────────────────────────────────────────────

enum BiquadType {
    Highpass,
    Lowpass,
    Peaking { gain_db: f32, q: f32 },
    LowShelf { gain_db: f32 },
    HighShelf { gain_db: f32 },
}

struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
}

impl Biquad {
    fn new(btype: BiquadType, freq: f32, sample_rate: u32) -> Self {
        let nyquist = sample_rate as f32 / 2.0;
        let clamped = freq.min(nyquist * 0.95).max(1.0);
        let w0 = 2.0 * PI * clamped / sample_rate as f32;
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();

        let (_a0, a1, a2, b0, b1, b2) = match btype {
            BiquadType::Highpass => {
                let q = std::f32::consts::FRAC_1_SQRT_2; // 0.707 Butterworth
                let alpha = sin_w0 / (2.0 * q);
                let a0_val = 1.0 + alpha;
                (
                    a0_val,
                    (-2.0 * cos_w0) / a0_val,
                    (1.0 - alpha) / a0_val,
                    (1.0 + cos_w0) / (2.0 * a0_val),
                    -(1.0 + cos_w0) / a0_val,
                    (1.0 + cos_w0) / (2.0 * a0_val),
                )
            }
            BiquadType::Lowpass => {
                let q = std::f32::consts::FRAC_1_SQRT_2; // 0.707 Butterworth
                let alpha = sin_w0 / (2.0 * q);
                let a0_val = 1.0 + alpha;
                (
                    a0_val,
                    (-2.0 * cos_w0) / a0_val,
                    (1.0 - alpha) / a0_val,
                    (1.0 - cos_w0) / (2.0 * a0_val),
                    (1.0 - cos_w0) / a0_val,
                    (1.0 - cos_w0) / (2.0 * a0_val),
                )
            }
            BiquadType::Peaking { gain_db, q } => {
                let alpha = sin_w0 / (2.0 * q);
                let a = 10.0_f32.powf(gain_db / 40.0);
                let a0_val = 1.0 + alpha / a;
                (
                    a0_val,
                    (-2.0 * cos_w0) / a0_val,
                    (1.0 - alpha / a) / a0_val,
                    (1.0 + alpha * a) / a0_val,
                    (-2.0 * cos_w0) / a0_val,
                    (1.0 - alpha * a) / a0_val,
                )
            }
            BiquadType::LowShelf { gain_db } => {
                let a = 10.0_f32.powf(gain_db / 40.0);
                let s = 1.0; // Shelf slope
                let alpha = sin_w0 / 2.0 * ((a + 1.0 / a) * (1.0 / s - 1.0) + 2.0).sqrt();
                let cos_w0_a = 2.0 * a.sqrt() * alpha;
                let a0_val = (a + 1.0) + (a - 1.0) * cos_w0 + cos_w0_a;
                (
                    a0_val,
                    (-2.0 * ((a - 1.0) + (a + 1.0) * cos_w0)) / a0_val,
                    ((a + 1.0) + (a - 1.0) * cos_w0 - cos_w0_a) / a0_val,
                    (a * ((a + 1.0) - (a - 1.0) * cos_w0 + cos_w0_a)) / a0_val,
                    (2.0 * a * ((a - 1.0) - (a + 1.0) * cos_w0)) / a0_val,
                    (a * ((a + 1.0) - (a - 1.0) * cos_w0 - cos_w0_a)) / a0_val,
                )
            }
            BiquadType::HighShelf { gain_db } => {
                let a = 10.0_f32.powf(gain_db / 40.0);
                let s = 1.0; // Shelf slope
                let alpha = sin_w0 / 2.0 * ((a + 1.0 / a) * (1.0 / s - 1.0) + 2.0).sqrt();
                let cos_w0_a = 2.0 * a.sqrt() * alpha;
                let a0_val = (a + 1.0) - (a - 1.0) * cos_w0 + cos_w0_a;
                (
                    a0_val,
                    (2.0 * ((a - 1.0) - (a + 1.0) * cos_w0)) / a0_val,
                    ((a + 1.0) - (a - 1.0) * cos_w0 - cos_w0_a) / a0_val,
                    (a * ((a + 1.0) + (a - 1.0) * cos_w0 + cos_w0_a)) / a0_val,
                    (-2.0 * a * ((a - 1.0) + (a + 1.0) * cos_w0)) / a0_val,
                    (a * ((a + 1.0) + (a - 1.0) * cos_w0 - cos_w0_a)) / a0_val,
                )
            }
        };

        Self {
            b0,
            b1,
            b2,
            a1,
            a2,
        }
    }

    fn process_sample(&self, x: f32, x1: &mut f32, x2: &mut f32, y1: &mut f32, y2: &mut f32) -> f32 {
        let y = self.b0 * x + self.b1 * *x1 + self.b2 * *x2
            - self.a1 * *y1 - self.a2 * *y2;
        *x2 = *x1;
        *x1 = x;
        *y2 = *y1;
        *y1 = y;
        y
    }
}

fn apply_biquad(samples: &mut [f32], sample_rate: u32, btype: BiquadType, freq: f32) {
    let filt = Biquad::new(btype, freq, sample_rate);
    let (mut x1, mut x2, mut y1, mut y2) = (0.0_f32, 0.0, 0.0, 0.0);
    for s in samples.iter_mut() {
        *s = filt.process_sample(*s, &mut x1, &mut x2, &mut y1, &mut y2);
    }
}

// ── Compressor ──────────────────────────────────────────────────────────

/// L-8: The compressor applies makeup gain unconditionally (above and below threshold).
/// This means signals below threshold are amplified, which raises the noise floor.
/// This is standard compressor behavior — "upward compression" of quiet signals.
fn apply_compressor(samples: &mut [f32], threshold_db: f32, sample_rate: u32) {
    let threshold = 10.0_f32.powf(threshold_db / 20.0);
    let ratio = 4.0_f32;
    // Attack 5ms, release 50ms
    let attack = (-1.0 / (0.005 * sample_rate as f32)).exp();
    let release = (-1.0 / (0.050 * sample_rate as f32)).exp();
    // Makeup gain: compensate roughly half the threshold reduction
    let makeup = 10.0_f32.powf(-threshold_db / 40.0);

    let mut env = 0.0_f32;
    for s in samples.iter_mut() {
        let level = s.abs();
        let coeff = if level > env { attack } else { release };
        env = coeff * env + (1.0 - coeff) * level;

        if env > threshold {
            let gain = (threshold / env).powf(1.0 - 1.0 / ratio);
            *s *= gain * makeup;
        } else {
            *s *= makeup;
        }
    }
}

// ── Pitch Shift (FX — resampling, changes buffer length) ───────────────

fn apply_pitch_shift(samples: &[f32], semitones: f32) -> Vec<f32> {
    let ratio = 2.0_f32.powf(semitones / 12.0);
    let new_len = ((samples.len() as f64) / ratio as f64).round().max(1.0) as usize;

    (0..new_len)
        .map(|i| {
            let src = i as f32 * ratio;
            let idx = src as usize;
            let frac = src - idx as f32;
            if idx + 1 < samples.len() {
                samples[idx] * (1.0 - frac) + samples[idx + 1] * frac
            } else if idx < samples.len() {
                samples[idx]
            } else {
                0.0
            }
        })
        .collect()
}

// ── Reverb (Schroeder: 4 comb ∥ → 2 allpass series) ────────────────────

fn apply_reverb(samples: &[f32], sample_rate: u32, mix: f32) -> Vec<f32> {
    let scale = sample_rate as f32 / 44100.0;

    // Comb filter delays and feedback gains
    let comb_params: [(usize, f32); 4] = [
        (((1557.0 * scale) as usize).max(1), 0.84),
        (((1617.0 * scale) as usize).max(1), 0.82),
        (((1491.0 * scale) as usize).max(1), 0.80),
        (((1422.0 * scale) as usize).max(1), 0.78),
    ];
    // Allpass delays
    let allpass_params: [(usize, f32); 2] = [
        (((225.0 * scale) as usize).max(1), 0.5),
        (((556.0 * scale) as usize).max(1), 0.5),
    ];

    let n = samples.len();

    // Sum of 4 parallel comb filters
    let mut wet = vec![0.0_f32; n];
    for &(delay, feedback) in &comb_params {
        let out = comb_filter(samples, delay, feedback);
        for (w, o) in wet.iter_mut().zip(out.iter()) {
            *w += o;
        }
    }
    for s in wet.iter_mut() {
        *s *= 0.25;
    }

    // Two allpass filters in series
    for &(delay, gain) in &allpass_params {
        wet = allpass_filter(&wet, delay, gain);
    }

    // Wet/dry mix
    samples
        .iter()
        .zip(wet.iter())
        .map(|(&d, &w)| (1.0 - mix) * d + mix * w)
        .collect()
}

fn comb_filter(input: &[f32], delay: usize, feedback: f32) -> Vec<f32> {
    let n = input.len();
    let mut output = vec![0.0_f32; n];
    let mut buf = vec![0.0_f32; delay];
    let mut idx = 0;

    for i in 0..n {
        let delayed = buf[idx];
        buf[idx] = input[i] + feedback * delayed;
        output[i] = delayed;
        idx = (idx + 1) % delay;
    }
    output
}

fn allpass_filter(input: &[f32], delay: usize, gain: f32) -> Vec<f32> {
    let n = input.len();
    let mut output = vec![0.0_f32; n];
    let mut buf = vec![0.0_f32; delay];
    let mut idx = 0;

    for i in 0..n {
        let delayed = buf[idx];
        let temp = input[i] + gain * delayed;
        buf[idx] = temp;
        output[i] = delayed - gain * temp;
        idx = (idx + 1) % delay;
    }
    output
}

// ── 12-Band Graphic EQ ───────────────────────────────────────────────────────

/// 12-band graphic EQ: frequencies and filter types.
const EQ_BANDS: [(f32, &str); 12] = [
    (31.0, "shelf_low"),
    (63.0, "peak"),
    (125.0, "peak"),
    (250.0, "peak"),
    (500.0, "peak"),
    (1000.0, "peak"),
    (2000.0, "peak"),
    (3150.0, "peak"),
    (4000.0, "peak"),
    (6300.0, "peak"),
    (10000.0, "peak"),
    (16000.0, "shelf_high"),
];

/// Apply 12-band graphic EQ to samples.
pub fn apply_eq(samples: &mut [f32], sample_rate: u32, params: &EqParams) {
    if params.is_neutral() || samples.is_empty() || sample_rate == 0 {
        return;
    }

    for (i, &(freq, kind)) in EQ_BANDS.iter().enumerate() {
        let gain_db = params.gains[i];
        if gain_db.abs() < 1e-6 {
            continue; // Skip neutral bands
        }

        let btype = match kind {
            "shelf_low" => BiquadType::LowShelf { gain_db },
            "shelf_high" => BiquadType::HighShelf { gain_db },
            _ => BiquadType::Peaking {
                gain_db,
                q: 1.41, // ~1 octave bandwidth
            },
        };

        apply_biquad(samples, sample_rate, btype, freq);
    }
}
