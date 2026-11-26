use std::fs;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static TEMP_SERIAL: AtomicU64 = AtomicU64::new(0);

pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let dir: &Path = path
        .parent()
        .ok_or_else(|| format!("Invalid output path {}", path.display()))?;
    fs::create_dir_all(dir).map_err(|e| format!("Cannot create {}: {e}", dir.display()))?;
    let temp: PathBuf = temp_sibling(path);
    let result: Result<(), String> = write_then_rename(&temp, path, bytes);
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

pub fn temp_sibling(path: &Path) -> PathBuf {
    let dir: &Path = path.parent().unwrap_or_else(|| Path::new(""));
    let file_name: String = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let serial: u64 = TEMP_SERIAL.fetch_add(1, Ordering::Relaxed);
    dir.join(format!(".{file_name}.{}.{serial}.tmp", std::process::id()))
}

pub fn remove_if_exists(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("Cannot delete {}: {error}", path.display())),
    }
}

pub fn modified_ms(path: &Path) -> Option<u64> {
    fs::metadata(path)
        .ok()
        .filter(|metadata| metadata.is_file())
        .and_then(|metadata| metadata.modified().ok())
        .map(system_time_ms)
}

fn write_then_rename(temp: &Path, target: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file: fs::File =
        fs::File::create(temp).map_err(|e| format!("Cannot create {}: {e}", temp.display()))?;
    file.write_all(bytes)
        .and_then(|()| file.flush())
        .map_err(|e| format!("Cannot write {}: {e}", temp.display()))?;
    drop(file);
    fs::rename(temp, target).map_err(|e| format!("Cannot replace {}: {e}", target.display()))
}

pub fn system_time_ms(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .map(|elapsed| u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

pub fn now_ms() -> u64 {
    system_time_ms(SystemTime::now())
}
