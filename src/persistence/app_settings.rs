use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::app::input::MouseInputSettings;
use crate::app::tool_palette::{
    ToolPaletteItem, default_tool_palette_items, normalized_tool_palette_order,
    ordered_visible_tools,
};
use crate::model::{ComparisonPhase, DEFAULT_COMPARISON_SEQUENCE, is_valid_comparison_sequence};

const SETTINGS_FILE_NAME: &str = "settings.json";
pub const FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AppSettings {
    pub format_version: u32,
    pub mouse_input: MouseInputSettings,
    pub tool_palette_items: Vec<ToolPaletteItem>,
    #[serde(default)]
    pub tool_palette_order: Vec<ToolPaletteItem>,
    pub comparison_sequence: Vec<ComparisonPhase>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            format_version: FORMAT_VERSION,
            mouse_input: MouseInputSettings::default(),
            tool_palette_items: default_tool_palette_items(),
            tool_palette_order: default_tool_palette_items(),
            comparison_sequence: DEFAULT_COMPARISON_SEQUENCE.to_vec(),
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
        normalize_app_settings(&mut settings);
        return Ok((Some(settings), true));
    }
    if header.format_version != FORMAT_VERSION {
        return Err(AppSettingsError::UnsupportedFormatVersion(
            header.format_version,
        ));
    }
    let mut settings: AppSettings =
        serde_json::from_slice(&json).map_err(AppSettingsError::Json)?;
    let normalized = normalize_app_settings(&mut settings);
    Ok((Some(settings), normalized))
}

fn normalize_app_settings(settings: &mut AppSettings) -> bool {
    normalize_comparison_sequence(settings) | normalize_tool_palette(settings)
}

fn normalize_comparison_sequence(settings: &mut AppSettings) -> bool {
    if is_valid_comparison_sequence(&settings.comparison_sequence) {
        false
    } else {
        settings.comparison_sequence = DEFAULT_COMPARISON_SEQUENCE.to_vec();
        true
    }
}

fn normalize_tool_palette(settings: &mut AppSettings) -> bool {
    let original_order = settings.tool_palette_order.clone();
    let original_visible = settings.tool_palette_items.clone();
    let order = normalized_tool_palette_order(&original_order, &original_visible);
    let visible = ordered_visible_tools(&order, &original_visible);
    let visible = if visible.is_empty() {
        default_tool_palette_items()
    } else {
        visible
    };
    settings.tool_palette_order = order;
    settings.tool_palette_items = visible;
    settings.tool_palette_order != original_order || settings.tool_palette_items != original_visible
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
    use crate::model::ComparisonPhase;

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

    #[test]
    fn settings_without_tool_palette_items_use_defaults() {
        let path = test_path();
        save_app_settings_to(&path, &AppSettings::default()).unwrap();
        let mut json: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        json.as_object_mut().unwrap().remove("tool_palette_items");
        fs::write(&path, serde_json::to_vec_pretty(&json).unwrap()).unwrap();

        let settings = load_app_settings_from(&path).unwrap().unwrap();
        assert_eq!(
            settings.tool_palette_items,
            AppSettings::default().tool_palette_items
        );

        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn invalid_loop_sequencer_settings_fall_back_to_the_default_sequence() {
        for sequence in [vec![], vec![ComparisonPhase::Mix; 11]] {
            let path = test_path();
            let settings = AppSettings {
                comparison_sequence: sequence,
                ..AppSettings::default()
            };
            save_app_settings_to(&path, &settings).unwrap();

            let loaded = load_app_settings_from(&path).unwrap().unwrap();
            assert_eq!(
                loaded.comparison_sequence,
                AppSettings::default().comparison_sequence
            );

            let _ = fs::remove_dir_all(path.parent().unwrap());
        }
    }

    #[test]
    fn old_palette_settings_keep_their_visible_order_as_the_master_order() {
        use crate::app::tool_palette::ToolPaletteItem::{Equalizer, Timbre};

        let path = test_path();
        let settings = AppSettings {
            tool_palette_items: vec![Timbre, Equalizer],
            tool_palette_order: vec![],
            ..AppSettings::default()
        };
        save_app_settings_to(&path, &settings).unwrap();

        let loaded = load_app_settings_from(&path).unwrap().unwrap();
        assert_eq!(loaded.tool_palette_items, [Timbre, Equalizer]);
        assert_eq!(&loaded.tool_palette_order[..2], &[Timbre, Equalizer]);

        let _ = fs::remove_dir_all(path.parent().unwrap());
    }
}
