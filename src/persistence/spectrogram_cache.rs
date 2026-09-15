use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use crate::analysis::spectrum::{MAX_MIDI_NOTE, MIN_MIDI_NOTE, SpectrogramData};
use crate::analysis::stft::StftSettings;
use crate::persistence::sidecar::AudioIdentity;

const MAGIC: [u8; 4] = *b"OBSC";
const FORMAT_VERSION: u32 = 1;
const MAX_FILENAME_BYTES: usize = 16 * 1024;
const MAX_INTENSITY_VALUES: usize = 200_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheLoadOutcome {
    NotFound,
    Hit,
    Invalid,
}

#[derive(Debug)]
pub enum SpectrogramCacheError {
    Io(io::Error),
    InvalidPath(PathBuf),
    InvalidData(&'static str),
}

impl fmt::Display for SpectrogramCacheError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "I/Oエラー: {error}"),
            Self::InvalidPath(path) => {
                write!(f, "キャッシュ保存先を作れません: {}", path.display())
            }
            Self::InvalidData(reason) => write!(f, "キャッシュ形式が不正です: {reason}"),
        }
    }
}

impl std::error::Error for SpectrogramCacheError {}

pub fn spectrogram_cache_path(audio_path: &Path) -> Result<PathBuf, SpectrogramCacheError> {
    let file_name = audio_path
        .file_name()
        .ok_or_else(|| SpectrogramCacheError::InvalidPath(audio_path.to_owned()))?;
    let mut cache_name = file_name.to_os_string();
    cache_name.push(".otomieru.spectrum.bin");
    Ok(audio_path.with_file_name(cache_name))
}

pub fn load_spectrogram_cache(
    audio_path: &Path,
    identity: &AudioIdentity,
    settings: StftSettings,
) -> Result<(CacheLoadOutcome, Option<SpectrogramData>), SpectrogramCacheError> {
    let path = spectrogram_cache_path(audio_path)?;
    let file = match OpenOptions::new().read(true).open(&path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok((CacheLoadOutcome::NotFound, None));
        }
        Err(error) => return Err(SpectrogramCacheError::Io(error)),
    };
    let mut reader = BufReader::new(file);
    match read_cache(&mut reader, identity, settings) {
        Ok(Some(spectrogram)) => Ok((CacheLoadOutcome::Hit, Some(spectrogram))),
        Ok(None) | Err(SpectrogramCacheError::InvalidData(_)) => {
            Ok((CacheLoadOutcome::Invalid, None))
        }
        Err(SpectrogramCacheError::Io(error)) if error.kind() == io::ErrorKind::UnexpectedEof => {
            Ok((CacheLoadOutcome::Invalid, None))
        }
        Err(error) => Err(error),
    }
}

pub fn save_spectrogram_cache(
    audio_path: &Path,
    identity: &AudioIdentity,
    settings: StftSettings,
    spectrogram: &SpectrogramData,
) -> Result<PathBuf, SpectrogramCacheError> {
    validate_spectrogram(spectrogram)?;
    let path = spectrogram_cache_path(audio_path)?;
    let directory = path
        .parent()
        .ok_or_else(|| SpectrogramCacheError::InvalidPath(path.clone()))?;
    fs::create_dir_all(directory).map_err(SpectrogramCacheError::Io)?;
    let temporary_path = path.with_extension("bin.tmp");
    let result = (|| {
        let file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&temporary_path)
            .map_err(SpectrogramCacheError::Io)?;
        let mut writer = BufWriter::new(file);
        write_cache(&mut writer, identity, settings, spectrogram)?;
        writer.flush().map_err(SpectrogramCacheError::Io)?;
        writer
            .get_ref()
            .sync_all()
            .map_err(SpectrogramCacheError::Io)?;
        fs::rename(&temporary_path, &path).map_err(SpectrogramCacheError::Io)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary_path);
    }
    result.map(|()| path)
}

fn write_cache(
    writer: &mut impl Write,
    identity: &AudioIdentity,
    settings: StftSettings,
    spectrogram: &SpectrogramData,
) -> Result<(), SpectrogramCacheError> {
    let file_name = identity.file_name.as_bytes();
    if file_name.len() > MAX_FILENAME_BYTES {
        return Err(SpectrogramCacheError::InvalidData("音源名が長すぎます"));
    }
    writer
        .write_all(&MAGIC)
        .map_err(SpectrogramCacheError::Io)?;
    write_u32(writer, FORMAT_VERSION)?;
    write_u32(writer, file_name.len() as u32)?;
    writer
        .write_all(file_name)
        .map_err(SpectrogramCacheError::Io)?;
    write_u64(writer, identity.file_size_bytes)?;
    write_u64(writer, identity.modified_unix_ms)?;
    write_f64(writer, identity.duration_seconds)?;
    write_u32(writer, identity.sample_rate_hz)?;
    write_u64(writer, settings.window_size as u64)?;
    write_u64(writer, settings.hop_size as u64)?;
    write_u64(writer, spectrogram.frames as u64)?;
    write_u64(writer, spectrogram.pitches as u64)?;
    write_u64(writer, spectrogram.min_midi_note as u64)?;
    write_u64(writer, spectrogram.max_midi_note as u64)?;
    write_f64(writer, spectrogram.frame_duration_seconds)?;
    write_u64(writer, spectrogram.intensities.len() as u64)?;
    for intensity in &spectrogram.intensities {
        writer
            .write_all(&intensity.to_le_bytes())
            .map_err(SpectrogramCacheError::Io)?;
    }
    Ok(())
}

fn read_cache(
    reader: &mut impl Read,
    identity: &AudioIdentity,
    settings: StftSettings,
) -> Result<Option<SpectrogramData>, SpectrogramCacheError> {
    let mut magic = [0_u8; 4];
    reader
        .read_exact(&mut magic)
        .map_err(SpectrogramCacheError::Io)?;
    if magic != MAGIC || read_u32(reader)? != FORMAT_VERSION {
        return Ok(None);
    }
    let file_name_len = read_u32(reader)? as usize;
    if file_name_len > MAX_FILENAME_BYTES {
        return Err(SpectrogramCacheError::InvalidData("音源名の長さ"));
    }
    let mut file_name = vec![0_u8; file_name_len];
    reader
        .read_exact(&mut file_name)
        .map_err(SpectrogramCacheError::Io)?;
    let stored_file_name = std::str::from_utf8(&file_name)
        .map_err(|_| SpectrogramCacheError::InvalidData("音源名の文字コード"))?;
    // 比較の途中で short-circuit すると以降のフィールドを読まず、ストリームの
    // 位置がずれる。全フィールドを先に読むことで不一致キャッシュも安全に扱う。
    let stored_file_size = read_u64(reader)?;
    let stored_modified_unix_ms = read_u64(reader)?;
    let stored_duration_seconds = read_f64(reader)?;
    let stored_sample_rate_hz = read_u32(reader)?;
    let stored_window_size = read_u64(reader)?;
    let stored_hop_size = read_u64(reader)?;
    let matches_identity = stored_file_name == identity.file_name
        && stored_file_size == identity.file_size_bytes
        && stored_modified_unix_ms == identity.modified_unix_ms
        && stored_duration_seconds == identity.duration_seconds
        && stored_sample_rate_hz == identity.sample_rate_hz;
    let matches_analysis = stored_window_size == settings.window_size as u64
        && stored_hop_size == settings.hop_size as u64;
    let frames = read_usize(reader)?;
    let pitches = read_usize(reader)?;
    let min_midi_note = read_usize(reader)?;
    let max_midi_note = read_usize(reader)?;
    let frame_duration_seconds = read_f64(reader)?;
    let intensity_len = read_usize(reader)?;
    if !matches_identity
        || !matches_analysis
        || min_midi_note != MIN_MIDI_NOTE
        || max_midi_note != MAX_MIDI_NOTE
        || !(pitches == MAX_MIDI_NOTE - MIN_MIDI_NOTE + 1 || (frames == 0 && pitches == 0))
        || !frame_duration_seconds.is_finite()
        || intensity_len != frames.saturating_mul(pitches)
        || intensity_len > MAX_INTENSITY_VALUES
    {
        return Ok(None);
    }
    let mut intensities = Vec::with_capacity(intensity_len);
    for _ in 0..intensity_len {
        let value = read_f32(reader)?;
        if !value.is_finite() || !(0.0..=1.0).contains(&value) {
            return Err(SpectrogramCacheError::InvalidData("強度値"));
        }
        intensities.push(value);
    }
    Ok(Some(SpectrogramData {
        frames,
        pitches,
        min_midi_note,
        max_midi_note,
        frame_duration_seconds,
        intensities,
    }))
}

fn validate_spectrogram(spectrogram: &SpectrogramData) -> Result<(), SpectrogramCacheError> {
    if spectrogram.min_midi_note != MIN_MIDI_NOTE
        || spectrogram.max_midi_note != MAX_MIDI_NOTE
        || !(spectrogram.pitches == MAX_MIDI_NOTE - MIN_MIDI_NOTE + 1
            || (spectrogram.frames == 0 && spectrogram.pitches == 0))
        || spectrogram.intensities.len() != spectrogram.frames.saturating_mul(spectrogram.pitches)
        || spectrogram.intensities.len() > MAX_INTENSITY_VALUES
        || !spectrogram.frame_duration_seconds.is_finite()
        || spectrogram
            .intensities
            .iter()
            .any(|value| !value.is_finite() || !(0.0..=1.0).contains(value))
    {
        return Err(SpectrogramCacheError::InvalidData("スペクトログラム値"));
    }
    Ok(())
}

fn write_u32(writer: &mut impl Write, value: u32) -> Result<(), SpectrogramCacheError> {
    writer
        .write_all(&value.to_le_bytes())
        .map_err(SpectrogramCacheError::Io)
}

fn write_u64(writer: &mut impl Write, value: u64) -> Result<(), SpectrogramCacheError> {
    writer
        .write_all(&value.to_le_bytes())
        .map_err(SpectrogramCacheError::Io)
}

fn write_f64(writer: &mut impl Write, value: f64) -> Result<(), SpectrogramCacheError> {
    writer
        .write_all(&value.to_le_bytes())
        .map_err(SpectrogramCacheError::Io)
}

fn read_u32(reader: &mut impl Read) -> Result<u32, SpectrogramCacheError> {
    let mut bytes = [0_u8; 4];
    reader
        .read_exact(&mut bytes)
        .map_err(SpectrogramCacheError::Io)?;
    Ok(u32::from_le_bytes(bytes))
}

fn read_u64(reader: &mut impl Read) -> Result<u64, SpectrogramCacheError> {
    let mut bytes = [0_u8; 8];
    reader
        .read_exact(&mut bytes)
        .map_err(SpectrogramCacheError::Io)?;
    Ok(u64::from_le_bytes(bytes))
}

fn read_f64(reader: &mut impl Read) -> Result<f64, SpectrogramCacheError> {
    let mut bytes = [0_u8; 8];
    reader
        .read_exact(&mut bytes)
        .map_err(SpectrogramCacheError::Io)?;
    Ok(f64::from_le_bytes(bytes))
}

fn read_f32(reader: &mut impl Read) -> Result<f32, SpectrogramCacheError> {
    let mut bytes = [0_u8; 4];
    reader
        .read_exact(&mut bytes)
        .map_err(SpectrogramCacheError::Io)?;
    Ok(f32::from_le_bytes(bytes))
}

fn read_usize(reader: &mut impl Read) -> Result<usize, SpectrogramCacheError> {
    read_u64(reader)?
        .try_into()
        .map_err(|_| SpectrogramCacheError::InvalidData("整数範囲"))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    fn test_path(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir()
            .join(format!("otomieru-boyer-cache-{unique}"))
            .join(name)
    }

    fn identity() -> AudioIdentity {
        AudioIdentity {
            file_name: "song.wav".to_owned(),
            file_size_bytes: 12_345,
            modified_unix_ms: 678_900,
            duration_seconds: 12.5,
            sample_rate_hz: 44_100,
        }
    }

    fn spectrogram() -> SpectrogramData {
        SpectrogramData {
            frames: 2,
            pitches: MAX_MIDI_NOTE - MIN_MIDI_NOTE + 1,
            min_midi_note: MIN_MIDI_NOTE,
            max_midi_note: MAX_MIDI_NOTE,
            frame_duration_seconds: 512.0 / 44_100.0,
            intensities: vec![0.25; 2 * (MAX_MIDI_NOTE - MIN_MIDI_NOTE + 1)],
        }
    }

    #[test]
    fn cache_round_trip_returns_cached_spectrogram() {
        let audio_path = test_path("song.wav");
        let identity = identity();
        let source = spectrogram();
        save_spectrogram_cache(&audio_path, &identity, StftSettings::default(), &source)
            .expect("save cache");

        let (outcome, loaded) =
            load_spectrogram_cache(&audio_path, &identity, StftSettings::default())
                .expect("load cache");

        assert_eq!(outcome, CacheLoadOutcome::Hit);
        let loaded = loaded.expect("cached spectrogram");
        assert_eq!(loaded.frames, source.frames);
        assert_eq!(loaded.pitches, source.pitches);
        assert_eq!(loaded.intensities, source.intensities);
        fs::remove_dir_all(audio_path.parent().expect("parent")).expect("cleanup");
    }

    #[test]
    fn audio_identity_mismatch_invalidates_cache() {
        let audio_path = test_path("song.wav");
        let identity = identity();
        save_spectrogram_cache(
            &audio_path,
            &identity,
            StftSettings::default(),
            &spectrogram(),
        )
        .expect("save cache");
        let changed_identity = AudioIdentity {
            file_size_bytes: identity.file_size_bytes + 1,
            ..identity
        };

        let (outcome, loaded) =
            load_spectrogram_cache(&audio_path, &changed_identity, StftSettings::default())
                .expect("load cache");

        assert_eq!(outcome, CacheLoadOutcome::Invalid);
        assert!(loaded.is_none());
        fs::remove_dir_all(audio_path.parent().expect("parent")).expect("cleanup");
    }

    #[test]
    fn analysis_settings_mismatch_invalidates_cache() {
        let audio_path = test_path("song.wav");
        let identity = identity();
        save_spectrogram_cache(
            &audio_path,
            &identity,
            StftSettings::default(),
            &spectrogram(),
        )
        .expect("save cache");
        let changed_settings = StftSettings {
            window_size: 2_048,
            hop_size: 256,
        };

        let (outcome, loaded) =
            load_spectrogram_cache(&audio_path, &identity, changed_settings).expect("load cache");

        assert_eq!(outcome, CacheLoadOutcome::Invalid);
        assert!(loaded.is_none());
        fs::remove_dir_all(audio_path.parent().expect("parent")).expect("cleanup");
    }

    #[test]
    fn corrupt_cache_is_invalid_and_does_not_stop_loading() {
        let audio_path = test_path("song.wav");
        let identity = identity();
        let cache_path = save_spectrogram_cache(
            &audio_path,
            &identity,
            StftSettings::default(),
            &spectrogram(),
        )
        .expect("save cache");
        fs::write(&cache_path, b"OBSC\x01").expect("corrupt cache");

        let (outcome, loaded) =
            load_spectrogram_cache(&audio_path, &identity, StftSettings::default())
                .expect("corrupt cache is recoverable");

        assert_eq!(outcome, CacheLoadOutcome::Invalid);
        assert!(loaded.is_none());
        fs::remove_dir_all(audio_path.parent().expect("parent")).expect("cleanup");
    }

    #[test]
    fn invalid_save_does_not_replace_existing_cache() {
        let audio_path = test_path("song.wav");
        let identity = identity();
        let cache_path = save_spectrogram_cache(
            &audio_path,
            &identity,
            StftSettings::default(),
            &spectrogram(),
        )
        .expect("save cache");
        let original = fs::read(&cache_path).expect("read cache");
        let invalid = SpectrogramData {
            frame_duration_seconds: f64::NAN,
            ..SpectrogramData::empty()
        };
        assert!(
            save_spectrogram_cache(&audio_path, &identity, StftSettings::default(), &invalid,)
                .is_err()
        );
        assert_eq!(fs::read(&cache_path).expect("read cache"), original);
        fs::remove_dir_all(audio_path.parent().expect("parent")).expect("cleanup");
    }
}
