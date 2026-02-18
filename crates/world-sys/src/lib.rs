mod safe;
pub use safe::*;

use std::mem::MaybeUninit;
use std::os::raw::c_int;

// --- DIO (f0 estimation) ---

#[repr(C)]
#[derive(Debug)]
pub(crate) struct DioOption {
    pub(crate) f0_floor: f64,
    pub(crate) f0_ceil: f64,
    pub(crate) channels_in_octave: f64,
    pub(crate) frame_period: f64,
    pub(crate) speed: c_int,
    pub(crate) allowed_range: f64,
}

extern "C" {
    pub(crate) fn InitializeDioOption(option: *mut DioOption);

    pub(crate) fn Dio(
        x: *const f64,
        x_length: c_int,
        fs: c_int,
        option: *const DioOption,
        temporal_positions: *mut f64,
        f0: *mut f64,
    );

    pub(crate) fn GetSamplesForDIO(fs: c_int, x_length: c_int, frame_period: f64) -> c_int;
}

// --- StoneMask (f0 refinement) ---

extern "C" {
    pub(crate) fn StoneMask(
        x: *const f64,
        x_length: c_int,
        fs: c_int,
        temporal_positions: *const f64,
        f0: *const f64,
        f0_length: c_int,
        refined_f0: *mut f64,
    );
}

// --- CheapTrick (spectral envelope) ---

#[repr(C)]
#[derive(Debug)]
pub(crate) struct CheapTrickOption {
    pub(crate) q1: f64,
    pub(crate) f0_floor: f64,
    pub(crate) fft_size: c_int,
}

extern "C" {
    pub(crate) fn InitializeCheapTrickOption(fs: c_int, option: *mut CheapTrickOption);

    pub(crate) fn CheapTrick(
        x: *const f64,
        x_length: c_int,
        fs: c_int,
        temporal_positions: *const f64,
        f0: *const f64,
        f0_length: c_int,
        option: *const CheapTrickOption,
        spectrogram: *mut *mut f64,
    );

    pub(crate) fn GetFFTSizeForCheapTrick(fs: c_int, option: *const CheapTrickOption) -> c_int;
}

// --- D4C (aperiodicity) ---

#[repr(C)]
#[derive(Debug)]
pub(crate) struct D4COption {
    pub(crate) threshold: f64,
}

extern "C" {
    pub(crate) fn InitializeD4COption(option: *mut D4COption);

    pub(crate) fn D4C(
        x: *const f64,
        x_length: c_int,
        fs: c_int,
        temporal_positions: *const f64,
        f0: *const f64,
        f0_length: c_int,
        fft_size: c_int,
        option: *const D4COption,
        aperiodicity: *mut *mut f64,
    );
}

// --- Synthesis ---

extern "C" {
    pub(crate) fn Synthesis(
        f0: *const f64,
        f0_length: c_int,
        spectrogram: *const *const f64,
        aperiodicity: *const *const f64,
        fft_size: c_int,
        frame_period: f64,
        fs: c_int,
        y_length: c_int,
        y: *mut f64,
    );
}

// --- Helpers ---

/// Initialize a WORLD option struct via its C initializer function.
///
/// # Safety
///
/// `init_fn` must fully initialize the pointed-to struct.
pub(crate) unsafe fn init_option<T>(init_fn: unsafe extern "C" fn(*mut T)) -> T {
    let mut opt = MaybeUninit::<T>::uninit();
    init_fn(opt.as_mut_ptr());
    opt.assume_init()
}

/// Like [`init_option`] but for initializers that take an extra leading `c_int`
/// argument (e.g. `InitializeCheapTrickOption(fs, *option)`).
///
/// # Safety
///
/// `init_fn` must fully initialize the pointed-to struct.
pub(crate) unsafe fn init_option_with_fs<T>(
    init_fn: unsafe extern "C" fn(c_int, *mut T),
    fs: c_int,
) -> T {
    let mut opt = MaybeUninit::<T>::uninit();
    init_fn(fs, opt.as_mut_ptr());
    opt.assume_init()
}
