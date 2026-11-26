use std::path::PathBuf;

use tauri::{AppHandle, Emitter};

use crate::custom_preview;
use crate::library::{self, Library, SCAN_PROGRESS_EVENT, ScanProgress};
use crate::orca;
use crate::project;
use crate::rename;
use crate::settings::{self, AppSettings};

async fn run_blocking<T, F>(task: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(task)
        .await
        .map_err(|e| format!("Background task failed: {e}"))?
}

#[tauri::command]
pub fn get_settings(app: AppHandle) -> Result<AppSettings, String> {
    settings::load_settings(&app)
}

#[tauri::command]
pub fn save_settings(app: AppHandle, settings: AppSettings) -> Result<(), String> {
    settings::save_settings(&app, &settings)
}

#[tauri::command]
pub async fn detect_orca() -> Result<Option<String>, String> {
    run_blocking(|| Ok(orca::detect_orca())).await
}

#[tauri::command]
pub async fn load_library(root: String) -> Result<Library, String> {
    run_blocking(move || library::load_library(&root)).await
}

#[tauri::command]
pub async fn refresh_library(app: AppHandle, root: String) -> Result<Library, String> {
    run_blocking(move || {
        let report = |progress: ScanProgress| {
            let _ = app.emit(SCAN_PROGRESS_EVENT, progress);
        };
        library::refresh_library(&root, &report)
    })
    .await
}

fn configured_launch_spec(settings: &AppSettings) -> Option<String> {
    settings
        .orca_path
        .as_deref()
        .map(str::trim)
        .filter(|spec| !spec.is_empty())
        .map(str::to_string)
}

fn resolve_launch_spec(app: &AppHandle) -> Result<String, String> {
    let mut current: AppSettings = settings::load_settings(app)?;
    if let Some(spec) = configured_launch_spec(&current) {
        return Ok(spec);
    }
    let detected: String = orca::detect_orca().ok_or_else(|| {
        "OrcaSlicer was not found. Set its executable or launch command in Settings.".to_string()
    })?;
    current.orca_path = Some(detected.clone());
    settings::save_settings(app, &current)?;
    Ok(detected)
}

fn existing_files(paths: &[String]) -> Result<Vec<PathBuf>, String> {
    if paths.is_empty() {
        return Err("No files to open".to_string());
    }
    paths
        .iter()
        .map(|path| {
            let file: PathBuf = PathBuf::from(path);
            if file.is_file() {
                Ok(file)
            } else {
                Err(format!("File not found: {path}"))
            }
        })
        .collect()
}

fn open_files_in_orca(app: &AppHandle, paths: &[String]) -> Result<(), String> {
    let files: Vec<PathBuf> = existing_files(paths)?;
    let spec: String = resolve_launch_spec(app)?;
    orca::launch(&spec, &files)
}

#[tauri::command]
pub async fn open_in_orca(app: AppHandle, path: String) -> Result<(), String> {
    run_blocking(move || open_files_in_orca(&app, &[path])).await
}

#[tauri::command]
pub async fn open_many_in_orca(app: AppHandle, paths: Vec<String>) -> Result<(), String> {
    run_blocking(move || open_files_in_orca(&app, &paths)).await
}

#[tauri::command]
pub async fn rename_model(
    root: String,
    rel_path: String,
    new_name: String,
) -> Result<Library, String> {
    run_blocking(move || rename::rename_model(&root, &rel_path, &new_name)).await
}

#[tauri::command]
pub async fn set_project(
    root: String,
    folder_rel_path: String,
    is_project: bool,
) -> Result<Library, String> {
    run_blocking(move || project::set_project(&root, &folder_rel_path, is_project)).await
}

#[tauri::command]
pub async fn set_custom_preview(
    root: String,
    rel_path: String,
    is_project: bool,
    image_path: Option<String>,
) -> Result<Library, String> {
    run_blocking(move || {
        custom_preview::set_custom_preview(&root, &rel_path, is_project, image_path.as_deref())
    })
    .await
}
