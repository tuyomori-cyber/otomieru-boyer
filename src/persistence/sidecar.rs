use std::collections::HashSet;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};

use crate::model::{LayerId, MemoId, PitchMemoLayer, ProjectData, ProjectSettings, Track};

pub const FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AudioIdentity {
    pub file_name: String,
    pub file_size_bytes: u64,
    pub modified_unix_ms: u64,
    pub duration_seconds: f64,
    pub sample_rate_hz: u32,
}

impl AudioIdentity {
    pub fn from_audio(path: &Path, track: &Track) -> Result<Self, SidecarError> {
        let file_name = path
            .file_name()
            .ok_or_else(|| SidecarError::InvalidAudioPath(path.to_owned()))?
            .to_string_lossy()
            .into_owned();
        let metadata = fs::metadata(path).map_err(SidecarError::Io)?;
        let modified_unix_ms = metadata
            .modified()
            .map_err(SidecarError::Io)?
            .duration_since(UNIX_EPOCH)
            .map_err(|_| SidecarError::ModifiedBeforeUnixEpoch(path.to_owned()))?
            .as_millis()
            .try_into()
            .map_err(|_| SidecarError::ModifiedTimeOutOfRange(path.to_owned()))?;

        Ok(Self {
            file_name,
            file_size_bytes: metadata.len(),
            modified_unix_ms,
            duration_seconds: track.duration_seconds,
            sample_rate_hz: track.sample_rate,
        })
    }

    fn mismatches(&self, current: &Self) -> Vec<AudioMismatch> {
        let mut mismatches = Vec::new();
        if self.file_name != current.file_name {
            mismatches.push(AudioMismatch::FileName);
        }
        if self.file_size_bytes != current.file_size_bytes {
            mismatches.push(AudioMismatch::FileSize);
        }
        if self.modified_unix_ms != current.modified_unix_ms {
            mismatches.push(AudioMismatch::ModifiedTime);
        }
        let duration_tolerance = (1.0 / current.sample_rate_hz.max(1) as f64).max(0.001);
        if (self.duration_seconds - current.duration_seconds).abs() > duration_tolerance {
            mismatches.push(AudioMismatch::Duration);
        }
        if self.sample_rate_hz != current.sample_rate_hz {
            mismatches.push(AudioMismatch::SampleRate);
        }
        mismatches
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioMismatch {
    FileName,
    FileSize,
    ModifiedTime,
    Duration,
    SampleRate,
}

impl AudioMismatch {
    pub fn label(self) -> &'static str {
        match self {
            Self::FileName => "ファイル名",
            Self::FileSize => "ファイルサイズ",
            Self::ModifiedTime => "更新日時",
            Self::Duration => "長さ",
            Self::SampleRate => "サンプルレート",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SidecarDocument {
    format_version: u32,
    audio: AudioIdentity,
    project_settings: ProjectSettings,
    layers: Vec<PitchMemoLayer>,
}

#[derive(Debug, Deserialize)]
struct SidecarHeader {
    format_version: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LoadOutcome {
    NotFound,
    Loaded {
        project: ProjectData,
        audio_mismatches: Vec<AudioMismatch>,
    },
}

#[derive(Debug)]
pub enum SidecarError {
    Io(std::io::Error),
    Json(serde_json::Error),
    UnsupportedFormatVersion(u32),
    InvalidValues(Vec<String>),
    InvalidAudioPath(PathBuf),
    ModifiedBeforeUnixEpoch(PathBuf),
    ModifiedTimeOutOfRange(PathBuf),
}

impl fmt::Display for SidecarError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "I/Oエラー: {error}"),
            Self::Json(error) => write!(f, "JSONエラー: {error}"),
            Self::UnsupportedFormatVersion(version) => {
                write!(f, "未対応の保存形式です（format_version: {version}）")
            }
            Self::InvalidValues(errors) => write!(f, "不正な値: {}", errors.join("、")),
            Self::InvalidAudioPath(path) => {
                write!(f, "音源ファイル名を取得できません: {}", path.display())
            }
            Self::ModifiedBeforeUnixEpoch(path) => {
                write!(
                    f,
                    "音源の更新日時がUnix epochより前です: {}",
                    path.display()
                )
            }
            Self::ModifiedTimeOutOfRange(path) => {
                write!(f, "音源の更新日時が範囲外です: {}", path.display())
            }
        }
    }
}

impl std::error::Error for SidecarError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Json(error) => Some(error),
            _ => None,
        }
    }
}

pub fn sidecar_path(audio_path: &Path) -> Result<PathBuf, SidecarError> {
    let file_name = audio_path
        .file_name()
        .ok_or_else(|| SidecarError::InvalidAudioPath(audio_path.to_owned()))?;
    let mut sidecar_name = file_name.to_os_string();
    sidecar_name.push(".otomieru.json");
    Ok(audio_path.with_file_name(sidecar_name))
}

pub fn load_sidecar(
    audio_path: &Path,
    current_audio: &AudioIdentity,
) -> Result<LoadOutcome, SidecarError> {
    let path = sidecar_path(audio_path)?;
    let json = match fs::read(&path) {
        Ok(json) => json,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(LoadOutcome::NotFound);
        }
        Err(error) => return Err(SidecarError::Io(error)),
    };
    // 新しい形式に未知の項目が増えていても、まず形式番号を正しく報告する。
    let header: SidecarHeader = serde_json::from_slice(&json).map_err(SidecarError::Json)?;
    if header.format_version != FORMAT_VERSION {
        return Err(SidecarError::UnsupportedFormatVersion(
            header.format_version,
        ));
    }
    let document: SidecarDocument = serde_json::from_slice(&json).map_err(SidecarError::Json)?;
    validate_document(&document)?;

    Ok(LoadOutcome::Loaded {
        audio_mismatches: document.audio.mismatches(current_audio),
        project: ProjectData {
            project_settings: document.project_settings,
            layers: document.layers,
        },
    })
}

pub fn save_sidecar(
    audio_path: &Path,
    audio: AudioIdentity,
    project: &ProjectData,
) -> Result<PathBuf, SidecarError> {
    let path = sidecar_path(audio_path)?;
    let document = SidecarDocument {
        format_version: FORMAT_VERSION,
        audio,
        project_settings: project.project_settings.clone(),
        layers: project.layers.clone(),
    };
    validate_document(&document)?;

    let temporary_path = temporary_sidecar_path(&path)?;
    let result = (|| {
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&temporary_path)
            .map_err(SidecarError::Io)?;
        let mut writer = BufWriter::new(file);
        serde_json::to_writer_pretty(&mut writer, &document).map_err(SidecarError::Json)?;
        writer.write_all(b"\n").map_err(SidecarError::Io)?;
        writer.flush().map_err(SidecarError::Io)?;
        writer.get_ref().sync_all().map_err(SidecarError::Io)?;
        fs::rename(&temporary_path, &path).map_err(SidecarError::Io)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary_path);
    }
    result?;
    Ok(path)
}

fn temporary_sidecar_path(path: &Path) -> Result<PathBuf, SidecarError> {
    let file_name = path
        .file_name()
        .ok_or_else(|| SidecarError::InvalidAudioPath(path.to_owned()))?;
    let mut temporary_name = file_name.to_os_string();
    temporary_name.push(".tmp");
    Ok(path.with_file_name(temporary_name))
}

fn validate_document(document: &SidecarDocument) -> Result<(), SidecarError> {
    let mut errors = Vec::new();
    let audio = &document.audio;
    if audio.file_name.trim().is_empty() {
        errors.push("audio.file_nameが空です".to_owned());
    }
    validate_finite_positive(
        audio.duration_seconds,
        "audio.duration_seconds",
        &mut errors,
    );
    if audio.sample_rate_hz == 0 {
        errors.push("audio.sample_rate_hzは1以上にしてください".to_owned());
    }

    let settings = &document.project_settings;
    validate_range(
        settings.preview_reference_a4_hz,
        430.0,
        450.0,
        "project_settings.preview_reference_a4_hz",
        &mut errors,
    );
    for (index, gain) in settings.equalizer_gains_db.iter().copied().enumerate() {
        validate_range(
            gain,
            -12.0,
            12.0,
            &format!("project_settings.equalizer_gains_db[{index}]"),
            &mut errors,
        );
    }
    let analysis = &settings.fundamental_analysis;
    validate_range(
        analysis.emphasis,
        0.0,
        100.0,
        "project_settings.fundamental_analysis.emphasis",
        &mut errors,
    );
    if analysis.scale_root >= 12 {
        errors.push(
            "project_settings.fundamental_analysis.scale_rootは0〜11にしてください".to_owned(),
        );
    }
    validate_range(
        analysis.unemphasized_pitch_attenuation,
        0.0,
        100.0,
        "project_settings.fundamental_analysis.unemphasized_pitch_attenuation",
        &mut errors,
    );

    let mut layer_ids = HashSet::<LayerId>::new();
    let mut memo_ids = HashSet::<MemoId>::new();
    if document.layers.is_empty() {
        errors.push("layersには1件以上のレイヤーが必要です".to_owned());
    }
    for (layer_index, layer) in document.layers.iter().enumerate() {
        let prefix = format!("layers[{layer_index}]");
        if layer.id.get() == 0 {
            errors.push(format!("{prefix}.idは1以上にしてください"));
        } else if !layer_ids.insert(layer.id) {
            errors.push(format!("{prefix}.idが重複しています"));
        }
        if layer.name.trim().is_empty() {
            errors.push(format!("{prefix}.nameが空です"));
        }
        for (memo_index, memo) in layer.memos.iter().enumerate() {
            let memo_prefix = format!("{prefix}.memos[{memo_index}]");
            if memo.id.get() == 0 {
                errors.push(format!("{memo_prefix}.idは1以上にしてください"));
            } else if !memo_ids.insert(memo.id) {
                errors.push(format!("{memo_prefix}.idが重複しています"));
            }
            validate_finite_nonnegative(
                memo.start_sec,
                &format!("{memo_prefix}.start_sec"),
                &mut errors,
            );
            validate_finite_positive(
                memo.duration_sec,
                &format!("{memo_prefix}.duration_sec"),
                &mut errors,
            );
            if memo.pitch_midi < 0 || memo.pitch_midi > 127 {
                errors.push(format!("{memo_prefix}.pitch_midiは0〜127にしてください"));
            }
            let end_sec = memo.start_sec + memo.duration_sec;
            if end_sec.is_finite()
                && audio.duration_seconds.is_finite()
                && end_sec > audio.duration_seconds + 0.001
            {
                errors.push(format!("{memo_prefix}が音源の長さを超えています"));
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(SidecarError::InvalidValues(errors))
    }
}

fn validate_range(value: f32, min: f32, max: f32, path: &str, errors: &mut Vec<String>) {
    if !value.is_finite() || !(min..=max).contains(&value) {
        errors.push(format!("{path}は{min}〜{max}の有限値にしてください"));
    }
}

fn validate_finite_nonnegative(value: f64, path: &str, errors: &mut Vec<String>) {
    if !value.is_finite() || value < 0.0 {
        errors.push(format!("{path}は0以上の有限値にしてください"));
    }
}

fn validate_finite_positive(value: f64, path: &str, errors: &mut Vec<String>) {
    if !value.is_finite() || value <= 0.0 {
        errors.push(format!("{path}は0より大きい有限値にしてください"));
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{
        AudioIdentity, AudioMismatch, LoadOutcome, SidecarError, load_sidecar, save_sidecar,
        sidecar_path,
    };
    use crate::model::ProjectState;

    fn test_directory(name: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "otomieru-sidecar-{}-{name}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn identity(file_name: &str) -> AudioIdentity {
        AudioIdentity {
            file_name: file_name.to_owned(),
            file_size_bytes: 123,
            modified_unix_ms: 456,
            duration_seconds: 10.0,
            sample_rate_hz: 44_100,
        }
    }

    #[test]
    fn sidecar_keeps_the_audio_extension() {
        let path = sidecar_path(Path::new("music/song.flac")).unwrap();
        assert_eq!(path, Path::new("music/song.flac.otomieru.json"));
    }

    #[test]
    fn project_round_trips_through_json() {
        let directory = test_directory("round-trip");
        let audio_path = directory.join("song.flac");
        fs::write(&audio_path, b"audio").unwrap();
        let mut project = ProjectState::new();
        let layer_id = project.editing.selected_layer_id.unwrap();
        project.add_memo(layer_id, 1.25, 0.5, 60).unwrap();
        let audio = identity("song.flac");

        save_sidecar(&audio_path, audio.clone(), &project.data).unwrap();
        let json = fs::read_to_string(sidecar_path(&audio_path).unwrap()).unwrap();
        let loaded = load_sidecar(&audio_path, &audio).unwrap();

        assert!(json.contains("\"format_version\": 1"));
        assert!(json.contains("\"project_settings\""));
        assert!(!json.contains("editing"));
        assert!(!json.contains("\"visible\""));
        assert!(!json.contains("\"muted\""));
        assert!(!json.contains("\"volume\""));
        assert!(!json.contains("\"opacity\""));
        assert_eq!(
            loaded,
            LoadOutcome::Loaded {
                project: project.data,
                audio_mismatches: Vec::new(),
            }
        );
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn sidecar_does_not_persist_freeze_runtime_state() {
        let directory = test_directory("freeze-runtime");
        let audio_path = directory.join("song.flac");
        fs::write(&audio_path, b"audio").unwrap();

        save_sidecar(
            &audio_path,
            identity("song.flac"),
            &ProjectState::new().data,
        )
        .unwrap();
        let json = fs::read_to_string(sidecar_path(&audio_path).unwrap())
            .unwrap()
            .to_lowercase();

        assert!(!json.contains("the_world"));
        assert!(!json.contains("freeze"));
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn audio_mismatch_is_reported_without_rejecting_the_project() {
        let directory = test_directory("mismatch");
        let audio_path = directory.join("song.flac");
        fs::write(&audio_path, b"audio").unwrap();
        let project = ProjectState::new();
        let saved_audio = identity("song.flac");
        save_sidecar(&audio_path, saved_audio, &project.data).unwrap();
        let mut current_audio = identity("song.flac");
        current_audio.file_size_bytes += 1;

        let loaded = load_sidecar(&audio_path, &current_audio).unwrap();

        let LoadOutcome::Loaded {
            audio_mismatches, ..
        } = loaded
        else {
            panic!("sidecar should exist");
        };
        assert_eq!(audio_mismatches, vec![AudioMismatch::FileSize]);
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn unsupported_format_version_is_rejected() {
        let directory = test_directory("version");
        let audio_path = directory.join("song.flac");
        fs::write(&audio_path, b"audio").unwrap();
        let project = ProjectState::new();
        let audio = identity("song.flac");
        let path = save_sidecar(&audio_path, audio.clone(), &project.data).unwrap();
        let json = fs::read_to_string(&path).unwrap();
        let future_json = json
            .replace("\"format_version\": 1", "\"format_version\": 99")
            .replace("\"audio\": {", "\"future_field\": true,\n  \"audio\": {");
        fs::write(&path, future_json).unwrap();

        let error = load_sidecar(&audio_path, &audio).unwrap_err();

        assert!(matches!(error, SidecarError::UnsupportedFormatVersion(99)));
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn invalid_project_values_are_rejected() {
        let directory = test_directory("invalid");
        let audio_path = directory.join("song.flac");
        fs::write(&audio_path, b"audio").unwrap();
        let project = ProjectState::new();
        let audio = identity("song.flac");
        let path = save_sidecar(&audio_path, audio.clone(), &project.data).unwrap();
        let json = fs::read_to_string(&path).unwrap();
        fs::write(
            &path,
            json.replace(
                "\"preview_reference_a4_hz\": 440.0",
                "\"preview_reference_a4_hz\": 999.0",
            ),
        )
        .unwrap();

        let error = load_sidecar(&audio_path, &audio).unwrap_err();

        assert!(matches!(error, SidecarError::InvalidValues(_)));
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn missing_sidecar_is_not_an_error() {
        let directory = test_directory("missing");
        let audio_path = directory.join("song.flac");

        let outcome = load_sidecar(&audio_path, &identity("song.flac")).unwrap();

        assert_eq!(outcome, LoadOutcome::NotFound);
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn malformed_json_is_reported() {
        let directory = test_directory("malformed");
        let audio_path = directory.join("song.flac");
        let path = sidecar_path(&audio_path).unwrap();
        fs::write(path, b"{not json}").unwrap();

        let error = load_sidecar(&audio_path, &identity("song.flac")).unwrap_err();

        assert!(matches!(error, SidecarError::Json(_)));
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn write_failure_is_reported_without_overwriting_an_existing_sidecar() {
        let directory = test_directory("write-error");
        let audio_path = directory.join("song.flac");
        let path = sidecar_path(&audio_path).unwrap();
        fs::write(&path, b"existing project").unwrap();
        let temporary_path = path.with_file_name(format!(
            "{}.tmp",
            path.file_name().unwrap().to_string_lossy()
        ));
        fs::create_dir(&temporary_path).unwrap();
        let project = ProjectState::new();

        let error = save_sidecar(&audio_path, identity("song.flac"), &project.data).unwrap_err();

        assert!(matches!(error, SidecarError::Io(_)));
        assert_eq!(fs::read(&path).unwrap(), b"existing project");
        let _ = fs::remove_dir_all(directory);
    }
}
