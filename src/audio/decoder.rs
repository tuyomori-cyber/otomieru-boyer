use std::fmt;
use std::fs::File;
use std::path::Path;

use symphonia::core::audio::{AudioBufferRef, SampleBuffer, Signal};
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

#[derive(Debug, Clone, Default)]
pub struct DecodedAudio {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub channels: u16,
}

impl DecodedAudio {
    pub fn duration_seconds(&self) -> f64 {
        if self.sample_rate == 0 || self.channels == 0 {
            return 0.0;
        }

        self.samples.len() as f64 / self.sample_rate as f64 / self.channels as f64
    }
}

#[derive(Debug)]
pub enum DecoderError {
    Io(std::io::Error),
    Decode(SymphoniaError),
    NoDefaultTrack,
    MissingSampleRate,
}

impl fmt::Display for DecoderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "I/O error: {error}"),
            Self::Decode(error) => write!(f, "decode error: {error}"),
            Self::NoDefaultTrack => write!(f, "no default audio track found"),
            Self::MissingSampleRate => write!(f, "missing sample rate in codec parameters"),
        }
    }
}

impl std::error::Error for DecoderError {}

impl From<std::io::Error> for DecoderError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<SymphoniaError> for DecoderError {
    fn from(value: SymphoniaError) -> Self {
        Self::Decode(value)
    }
}

pub fn decode_file(path: impl AsRef<Path>) -> Result<DecodedAudio, DecoderError> {
    let path = path.as_ref();
    let file = File::open(path)?;
    let media_source = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(extension) = path.extension().and_then(|ext| ext.to_str()) {
        hint.with_extension(extension);
    }

    let probed = symphonia::default::get_probe().format(
        &hint,
        media_source,
        &FormatOptions::default(),
        &MetadataOptions::default(),
    )?;

    let mut format = probed.format;
    let track = format.default_track().ok_or(DecoderError::NoDefaultTrack)?;
    let mut decoder =
        symphonia::default::get_codecs().make(&track.codec_params, &DecoderOptions::default())?;

    let fallback_sample_rate = track.codec_params.sample_rate;
    let fallback_channels = track.codec_params.channels.map(|channels| channels.count() as u16);
    let track_id = track.id;

    let mut samples = Vec::new();
    let mut detected_sample_rate: Option<u32> = None;
    let mut detected_channels: Option<u16> = None;

    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(SymphoniaError::IoError(error))
                if error.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break;
            }
            Err(error) => return Err(error.into()),
        };

        if packet.track_id() != track_id {
            continue;
        }

        let decoded = match decoder.decode(&packet) {
            Ok(decoded) => decoded,
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(SymphoniaError::IoError(error))
                if error.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break;
            }
            Err(error) => return Err(error.into()),
        };

        let spec = *decoded.spec();
        detected_sample_rate = Some(spec.rate);
        detected_channels = Some(spec.channels.count() as u16);
        append_samples(&mut samples, decoded);
    }

    let sample_rate = detected_sample_rate
        .or(fallback_sample_rate)
        .ok_or(DecoderError::MissingSampleRate)?;
    let channels = detected_channels
        .or(fallback_channels)
        .unwrap_or(1);

    Ok(DecodedAudio {
        samples,
        sample_rate,
        channels,
    })
}

fn append_samples(samples: &mut Vec<f32>, decoded: AudioBufferRef<'_>) {
    match decoded {
        AudioBufferRef::F32(buffer) => {
            let mut sample_buffer =
                SampleBuffer::<f32>::new(buffer.frames() as u64, *buffer.spec());
            sample_buffer.copy_interleaved_ref(AudioBufferRef::F32(buffer));
            samples.extend_from_slice(sample_buffer.samples());
        }
        _ => {
            let capacity = decoded.capacity() as u64;
            let spec = *decoded.spec();
            let mut sample_buffer = SampleBuffer::<f32>::new(capacity, spec);
            sample_buffer.copy_interleaved_ref(decoded);
            samples.extend_from_slice(sample_buffer.samples());
        }
    }
}
