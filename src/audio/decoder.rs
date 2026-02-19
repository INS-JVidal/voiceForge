use std::fmt;
use std::fs::File;
use std::path::Path;

use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

/// Decoded audio data in interleaved f32 PCM format.
#[derive(Debug, Clone)]
pub struct AudioData {
    /// Interleaved PCM samples (ch0_f0, ch1_f0, ch0_f1, ch1_f1, ...).
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub channels: u16,
}

impl AudioData {
    /// Duration in seconds.
    #[must_use]
    pub fn duration_secs(&self) -> f64 {
        if self.sample_rate == 0 || self.channels == 0 {
            return 0.0;
        }
        self.samples.len() as f64 / (self.sample_rate as f64 * self.channels as f64)
    }

    /// Total number of frames (samples per channel).
    #[must_use]
    pub fn frame_count(&self) -> usize {
        if self.channels == 0 {
            return 0;
        }
        self.samples.len() / self.channels as usize
    }
}

/// Errors that can occur during audio decoding.
#[derive(Debug)]
pub enum DecoderError {
    /// File could not be opened.
    Io(std::io::Error),
    /// Format not recognized or no audio tracks found.
    UnsupportedFormat(String),
    /// Codec not supported.
    UnsupportedCodec(String),
    /// Decoding failed.
    Decode(String),
}

impl fmt::Display for DecoderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DecoderError::Io(e) => write!(f, "I/O error: {e}"),
            DecoderError::UnsupportedFormat(msg) => write!(f, "unsupported format: {msg}"),
            DecoderError::UnsupportedCodec(msg) => write!(f, "unsupported codec: {msg}"),
            DecoderError::Decode(msg) => write!(f, "decode error: {msg}"),
        }
    }
}

impl std::error::Error for DecoderError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            DecoderError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for DecoderError {
    fn from(e: std::io::Error) -> Self {
        DecoderError::Io(e)
    }
}

/// Decode an audio file into interleaved f32 PCM.
///
/// Supports WAV, MP3, and FLAC (depending on symphonia features).
pub fn decode_file(path: &Path) -> Result<AudioData, DecoderError> {
    let file = File::open(path)?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|e| DecoderError::UnsupportedFormat(e.to_string()))?;

    let mut format = probed.format;

    let track = format
        .default_track()
        .or_else(|| {
            format
                .tracks()
                .iter()
                .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        })
        .ok_or_else(|| DecoderError::UnsupportedFormat("no audio tracks found".into()))?;

    let track_id = track.id;

    let sample_rate = track
        .codec_params
        .sample_rate
        .ok_or_else(|| DecoderError::UnsupportedFormat("unknown sample rate".into()))?;

    // #7: Return error if channel layout is unknown rather than silently defaulting.
    let channels = track
        .codec_params
        .channels
        .map(|ch| ch.count() as u16)
        .ok_or_else(|| DecoderError::UnsupportedFormat("unknown channel layout".into()))?;

    if channels == 0 {
        return Err(DecoderError::UnsupportedFormat("zero channels".into()));
    }

    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| DecoderError::UnsupportedCodec(e.to_string()))?;

    let mut samples = Vec::new();
    let mut sample_buf: Option<SampleBuffer<f32>> = None;

    loop {
        let packet = match format.next_packet() {
            Ok(pkt) => pkt,
            Err(SymphoniaError::IoError(ref e))
                if e.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break;
            }
            Err(SymphoniaError::ResetRequired) => break,
            Err(e) => return Err(DecoderError::Decode(e.to_string())),
        };

        if packet.track_id() != track_id {
            continue;
        }

        let audio_buf = match decoder.decode(&packet) {
            Ok(buf) => buf,
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(e) => return Err(DecoderError::Decode(e.to_string())),
        };

        let spec = *audio_buf.spec();
        // H-1: Use frames() (actual decoded count) not capacity() (allocated size)
        // to avoid reading zero-padded silence between packets.
        let frames = audio_buf.frames() as u64;
        let needed_samples = frames as usize * spec.channels.count();

        // Recreate the buffer if the current packet needs more interleaved samples.
        // #3: The unwrap is safe because we just assigned Some on the previous line,
        // but use expect() to document the invariant explicitly.
        let buf = match &mut sample_buf {
            Some(existing) if existing.capacity() >= needed_samples => existing,
            _ => {
                sample_buf = Some(SampleBuffer::<f32>::new(frames, spec));
                sample_buf
                    .as_mut()
                    .expect("sample_buf was just assigned Some")
            }
        };
        buf.copy_interleaved_ref(audio_buf);
        samples.extend_from_slice(buf.samples());
    }

    if samples.is_empty() {
        return Err(DecoderError::Decode("no audio samples decoded".into()));
    }

    Ok(AudioData {
        samples,
        sample_rate,
        channels,
    })
}
