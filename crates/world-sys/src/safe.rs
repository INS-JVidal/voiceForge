use crate::{
    init_option, init_option_with_fs, CheapTrick, D4C, Dio, GetFFTSizeForCheapTrick,
    GetSamplesForDIO, InitializeCheapTrickOption, InitializeD4COption, InitializeDioOption,
    StoneMask, Synthesis,
};
use std::os::raw::c_int;

/// Parameters extracted by WORLD analysis.
#[derive(Debug, Clone)]
pub struct WorldParams {
    pub f0: Vec<f64>,
    pub temporal_positions: Vec<f64>,
    /// Spectrogram: frame_count rows, each of length fft_size/2 + 1.
    pub spectrogram: Vec<Vec<f64>>,
    /// Aperiodicity: frame_count rows, each of length fft_size/2 + 1.
    pub aperiodicity: Vec<Vec<f64>>,
    pub fft_size: usize,
    pub frame_period: f64,
}

impl WorldParams {
    /// Validate internal consistency of parameters.
    ///
    /// # Panics
    ///
    /// Panics if dimensions are inconsistent.
    fn validate(&self) {
        let frame_count = self.f0.len();
        assert!(frame_count > 0, "f0 must not be empty");
        assert!(self.fft_size > 0, "fft_size must be positive");

        let sp_width = self.fft_size / 2 + 1;

        assert_eq!(
            self.temporal_positions.len(),
            frame_count,
            "temporal_positions length ({}) must match f0 length ({frame_count})",
            self.temporal_positions.len(),
        );
        assert_eq!(
            self.spectrogram.len(),
            frame_count,
            "spectrogram row count ({}) must match f0 length ({frame_count})",
            self.spectrogram.len(),
        );
        assert_eq!(
            self.aperiodicity.len(),
            frame_count,
            "aperiodicity row count ({}) must match f0 length ({frame_count})",
            self.aperiodicity.len(),
        );

        for (i, row) in self.spectrogram.iter().enumerate() {
            assert_eq!(
                row.len(),
                sp_width,
                "spectrogram[{i}] width ({}) must be fft_size/2+1 ({sp_width})",
                row.len(),
            );
        }
        for (i, row) in self.aperiodicity.iter().enumerate() {
            assert_eq!(
                row.len(),
                sp_width,
                "aperiodicity[{i}] width ({}) must be fft_size/2+1 ({sp_width})",
                row.len(),
            );
        }
    }
}

/// Analyze audio using WORLD vocoder (DIO -> StoneMask -> CheapTrick -> D4C).
///
/// # Panics
///
/// Panics if `audio` is empty, `sample_rate` is not positive, or `audio` length
/// exceeds `i32::MAX`.
#[must_use]
pub fn analyze(audio: &[f64], sample_rate: i32) -> WorldParams {
    assert!(!audio.is_empty(), "audio must not be empty");
    assert!(sample_rate > 0, "sample_rate must be positive");
    assert!(
        audio.len() <= c_int::MAX as usize,
        "audio length ({}) exceeds i32::MAX",
        audio.len(),
    );

    let x_length = audio.len() as c_int;
    let fs = sample_rate;

    // Initialize DIO options
    let dio_option = unsafe { init_option(InitializeDioOption) };
    let frame_period = dio_option.frame_period;

    // Get number of frames
    let f0_length_raw = unsafe { GetSamplesForDIO(fs, x_length, frame_period) };
    assert!(
        f0_length_raw > 0,
        "GetSamplesForDIO returned non-positive value: {f0_length_raw}",
    );
    let f0_length = f0_length_raw as usize;

    // Run DIO for f0 estimation
    let mut temporal_positions = vec![0.0f64; f0_length];
    let mut f0 = vec![0.0f64; f0_length];
    unsafe {
        Dio(
            audio.as_ptr(),
            x_length,
            fs,
            &dio_option,
            temporal_positions.as_mut_ptr(),
            f0.as_mut_ptr(),
        );
    }

    // Refine f0 with StoneMask
    let mut refined_f0 = vec![0.0f64; f0_length];
    unsafe {
        StoneMask(
            audio.as_ptr(),
            x_length,
            fs,
            temporal_positions.as_ptr(),
            f0.as_ptr(),
            f0_length_raw,
            refined_f0.as_mut_ptr(),
        );
    }

    // Initialize CheapTrick options and get FFT size
    let mut ct_option = unsafe { init_option_with_fs(InitializeCheapTrickOption, fs) };
    let fft_size = unsafe { GetFFTSizeForCheapTrick(fs, &ct_option) } as usize;
    ct_option.fft_size = fft_size as c_int;

    let sp_width = fft_size / 2 + 1;

    // Allocate spectrogram (array of pointers to rows)
    let mut sp_rows: Vec<Vec<f64>> = (0..f0_length).map(|_| vec![0.0f64; sp_width]).collect();
    let mut sp_ptrs: Vec<*mut f64> = sp_rows.iter_mut().map(|row| row.as_mut_ptr()).collect();

    unsafe {
        CheapTrick(
            audio.as_ptr(),
            x_length,
            fs,
            temporal_positions.as_ptr(),
            refined_f0.as_ptr(),
            f0_length_raw,
            &ct_option,
            sp_ptrs.as_mut_ptr(),
        );
    }

    // Initialize D4C options
    let d4c_option = unsafe { init_option(InitializeD4COption) };

    // Allocate aperiodicity (array of pointers to rows)
    let mut ap_rows: Vec<Vec<f64>> = (0..f0_length).map(|_| vec![0.0f64; sp_width]).collect();
    let mut ap_ptrs: Vec<*mut f64> = ap_rows.iter_mut().map(|row| row.as_mut_ptr()).collect();

    unsafe {
        D4C(
            audio.as_ptr(),
            x_length,
            fs,
            temporal_positions.as_ptr(),
            refined_f0.as_ptr(),
            f0_length_raw,
            fft_size as c_int,
            &d4c_option,
            ap_ptrs.as_mut_ptr(),
        );
    }

    WorldParams {
        f0: refined_f0,
        temporal_positions,
        spectrogram: sp_rows,
        aperiodicity: ap_rows,
        fft_size,
        frame_period,
    }
}

/// Synthesize audio from WORLD parameters.
///
/// Returns the reconstructed audio waveform.
///
/// # Panics
///
/// Panics if `sample_rate` is not positive or `params` has inconsistent
/// dimensions (e.g. spectrogram/aperiodicity row count doesn't match f0 length,
/// or row widths don't match fft_size/2+1).
#[must_use]
pub fn synthesize(params: &WorldParams, sample_rate: i32) -> Vec<f64> {
    assert!(sample_rate > 0, "sample_rate must be positive");
    params.validate();

    let fs = sample_rate;
    let f0_length = params.f0.len() as c_int;

    // Calculate output length
    let y_length =
        ((params.f0.len() as f64 - 1.0) * params.frame_period / 1000.0 * sample_rate as f64)
            as usize
            + 1;

    let mut y = vec![0.0f64; y_length];

    // Build pointer arrays for spectrogram and aperiodicity
    let sp_ptrs: Vec<*const f64> = params.spectrogram.iter().map(|row| row.as_ptr()).collect();
    let ap_ptrs: Vec<*const f64> = params.aperiodicity.iter().map(|row| row.as_ptr()).collect();

    unsafe {
        Synthesis(
            params.f0.as_ptr(),
            f0_length,
            sp_ptrs.as_ptr(),
            ap_ptrs.as_ptr(),
            params.fft_size as c_int,
            params.frame_period,
            fs,
            y_length as c_int,
            y.as_mut_ptr(),
        );
    }

    y
}
