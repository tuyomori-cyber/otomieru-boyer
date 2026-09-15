use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::app::input::MouseInputSettings;

const SETTINGS_FILE_NAME: &str = "settings.json";
pub const FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AppSettings {
    pub format_version: u32,
    pub mouse_input: MouseInputSettings,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            format_version: FORMAT_VERSION,
            mouse_input: MouseInputSettings::default(),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
struct AppSettingsHeader {
    #[serde(default)]
    format_version: u32,
}

#[derive(Debug)]
pub enum AppSettingsError {
    Io(std::io::Error),
    Json(serde_json::Error),
    UnsupportedFormatVersion(u32),
    ConfigDirectoryUnavailable,
}

impl fmt::Display for AppSettingsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "I/Oエラー: {error}"),
            Self::Json(error) => write!(f, "JSONエラー: {error}"),
            Self::UnsupportedFormatVersion(version) => {
                write!(f, "未対応の操作設定形式です（format_version: {version}）")
            }
            Self::ConfigDirectoryUnavailable => write!(f, "設定ディレクトリを特定できません"),
        }
    }
}

impl std::error::Error for AppSettingsError {}

pub fn load_app_settings() -> Result<Option<AppSettings>, AppSettingsError> {
    let Some(path) = app_settings_path() else {
        return Err(AppSettingsError::ConfigDirectoryUnavailable);
    };
    let (settings, migrated) = load_app_settings_with_migration(&path)?;
    if migrated && let Some(settings) = &settings {
        save_app_settings_to(&path, settings)?;
    }
    Ok(settings)
}

pub fn save_app_settings(settings: &AppSettings) -> Result<(), AppSettingsError> {
    let Some(path) = app_settings_path() else {
        return Err(AppSettingsError::ConfigDirectoryUnavailable);
    };
    save_app_settings_to(&path, settings)
}

fn app_settings_path() -> Option<PathBuf> {
    let config_root = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(config_root.join("otomieru-boyer").join(SETTINGS_FILE_NAME))
}

#[cfg(test)]
fn load_app_settings_from(path: &Path) -> Result<Option<AppSettings>, AppSettingsError> {
    load_app_settings_with_migration(path).map(|(settings, _)| settings)
}

fn load_app_settings_with_migration(
    path: &Path,
) -> Result<(Option<AppSettings>, bool), AppSettingsError> {
    let json = match fs::read(path) {
        Ok(json) => json,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok((None, false)),
        Err(error) => return Err(AppSettingsError::Io(error)),
    };
    let header: AppSettingsHeader =
        serde_json::from_slice(&json).map_err(AppSettingsError::Json)?;
    if header.format_version == 0 {
        let mut settings: AppSettings =
            serde_json::from_slice(&json).map_err(AppSettingsError::Json)?;
        settings.format_version = FORMAT_VERSION;
        return Ok((Some(settings), true));
    }
    if header.format_version != FORMAT_VERSION {
        return Err(AppSettingsError::UnsupportedFormatVersion(
            header.format_version,
        ));
    }
    serde_json::from_slice(&json)
        .map(|settings| (Some(settings), false))
        .map_err(AppSettingsError::Json)
}

fn save_app_settings_to(path: &Path, settings: &AppSettings) -> Result<(), AppSettingsError> {
    let directory = path
        .parent()
        .ok_or(AppSettingsError::ConfigDirectoryUnavailable)?;
    fs::create_dir_all(directory).map_err(AppSettingsError::Io)?;
    let temporary_path = path.with_extension("json.tmp");
    let result = (|| {
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&temporary_path)
            .map_err(AppSettingsError::Io)?;
        let mut writer = BufWriter::new(file);
        serde_json::to_writer_pretty(&mut writer, settings).map_err(AppSettingsError::Json)?;
        writer.write_all(b"\n").map_err(AppSettingsError::Io)?;
        writer.flush().map_err(AppSettingsError::Io)?;
        writer.get_ref().sync_all().map_err(AppSettingsError::Io)?;
        fs::rename(&temporary_path, path).map_err(AppSettingsError::Io)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary_path);
    }
    result
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{AppSettings, AppSettingsError, load_app_settings_from, save_app_settings_to};

    fn test_path() -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir()
            .join(format!(
                "otomieru-app-settings-{}-{unique}",
                std::process::id()
            ))
            .join("settings.json")
    }

    #[test]
    fn settings_round_trip_without_a_project_sidecar() {
        let path = test_path();
        let settings = AppSettings::default();

        save_app_settings_to(&path, &settings).unwrap();
        assert!(
            fs::read_to_string(&path)
                .unwrap()
                .contains("\"format_version\": 1")
        );
        assert_eq!(load_app_settings_from(&path).unwrap(), Some(settings));

        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn unsupported_format_version_is_rejected() {
        let path = test_path();
        save_app_settings_to(&path, &AppSettings::default()).unwrap();
        let json = fs::read_to_string(&path)
            .unwrap()
            .replace("\"format_version\": 1", "\"format_version\": 99");
        fs::write(&path, json).unwrap();

        assert!(matches!(
            load_app_settings_from(&path),
            Err(AppSettingsError::UnsupportedFormatVersion(99))
        ));

        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn unversioned_settings_are_migrated_to_version_one() {
        let path = test_path();
        save_app_settings_to(&path, &AppSettings::default()).unwrap();
        let json = fs::read_to_string(&path)
            .unwrap()
            .replace("  \"format_version\": 1,\n", "");
        fs::write(&path, json).unwrap();

        let settings = load_app_settings_from(&path).unwrap().unwrap();
        assert_eq!(settings.format_version, super::FORMAT_VERSION);

        let _ = fs::remove_dir_all(path.parent().unwrap());
    }
}
