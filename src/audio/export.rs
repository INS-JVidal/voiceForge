use std::path::Path;

use hound::{SampleFormat, WavSpec, WavWriter};

/// Error writing a WAV file.
#[derive(Debug)]
pub struct ExportError(String);

impl std::fmt::Display for ExportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "export error: {}", self.0)
    }
}

impl std::error::Error for ExportError {}

/// Write interleaved f32 samples to a 16-bit PCM WAV file.
pub fn export_wav(
    samples: &[f32],
    sample_rate: u32,
    channels: u16,
    path: &Path,
) -> Result<(), ExportError> {
    let spec = WavSpec {
        channels,
        sample_rate,
        bits_per_sample: 16,
        sample_format: SampleFormat::Int,
    };
    let mut writer = WavWriter::create(path, spec)
        .map_err(|e| ExportError(format!("cannot create file: {e}")))?;

    for &s in samples {
        let val = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
        writer
            .write_sample(val)
            .map_err(|e| ExportError(format!("write error: {e}")))?;
    }

    writer
        .finalize()
        .map_err(|e| ExportError(format!("finalize error: {e}")))?;

    Ok(())
}

/// Build the default output path: same directory as source, `{stem}_processed.wav`.
/// If the file already exists, try `{stem}_processed_2.wav`, `_3`, etc.
pub fn default_export_path(source_path: &str) -> String {
    let p = Path::new(source_path);
    let dir = p.parent().unwrap_or(Path::new("."));
    let stem = p
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");

    // M-14: Use a bounded range to avoid u32 overflow in release builds.
    for n in 1_u32..=9999 {
        let name = if n == 1 {
            format!("{stem}_processed.wav")
        } else {
            format!("{stem}_processed_{n}.wav")
        };
        let candidate = dir.join(&name);
        if !candidate.exists() {
            return candidate.to_string_lossy().into_owned();
        }
    }
    // Fallback if all 9999 candidates exist.
    dir.join(format!("{stem}_processed.wav"))
        .to_string_lossy()
        .into_owned()
}
