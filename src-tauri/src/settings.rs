use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::fsutil::write_atomic;

const SETTINGS_FILE_NAME: &str = "settings.json";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    #[serde(default)]
    pub last_root: Option<String>,
    #[serde(default)]
    pub orca_path: Option<String>,
}

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map(|dir| dir.join(SETTINGS_FILE_NAME))
        .map_err(|e| format!("Cannot locate the config folder: {e}"))
}

pub fn load_settings(app: &AppHandle) -> Result<AppSettings, String> {
    read_settings_file(&settings_path(app)?)
}

pub fn save_settings(app: &AppHandle, settings: &AppSettings) -> Result<(), String> {
    write_settings_file(&settings_path(app)?, settings)
}

fn read_settings_file(path: &Path) -> Result<AppSettings, String> {
    match fs::read(path) {
        Ok(bytes) => Ok(serde_json::from_slice::<AppSettings>(&bytes).unwrap_or_default()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(AppSettings::default()),
        Err(error) => Err(format!("Cannot read {}: {error}", path.display())),
    }
}

fn write_settings_file(path: &Path, settings: &AppSettings) -> Result<(), String> {
    let bytes: Vec<u8> = serde_json::to_vec_pretty(settings)
        .map_err(|e| format!("Cannot serialize settings: {e}"))?;
    write_atomic(path, &bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::unique_temp_dir;

    #[test]
    fn settings_round_trip_and_defaults() {
        let dir: PathBuf = unique_temp_dir("settings");
        let path: PathBuf = dir.join("nested").join(SETTINGS_FILE_NAME);
        assert_eq!(read_settings_file(&path), Ok(AppSettings::default()));
        let settings = AppSettings {
            last_root: Some("/models".to_string()),
            orca_path: Some("flatpak run io.github.softfever.OrcaSlicer".to_string()),
        };
        assert_eq!(write_settings_file(&path, &settings), Ok(()));
        assert_eq!(read_settings_file(&path), Ok(settings));
        let raw: String = fs::read_to_string(&path).unwrap_or_default();
        assert!(raw.contains("lastRoot"));
        assert!(raw.contains("orcaPath"));
        let _ = fs::remove_dir_all(dir);
    }
}
