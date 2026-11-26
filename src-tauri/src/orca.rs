use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

const PATH_EXECUTABLE_NAMES: [&str; 3] = ["orca-slicer", "OrcaSlicer", "orcaslicer"];
const FLATPAK_APP_ID: &str = "io.github.softfever.OrcaSlicer";
const APPIMAGE_HOME_DIRS: [&str; 8] = [
    "",
    "Applications",
    "Apps",
    "AppImages",
    ".local/bin",
    "bin",
    "Downloads",
    "Desktop",
];
const APPIMAGE_NAME_HINT: &str = "orca";
const APPIMAGE_SUFFIX: &str = ".appimage";
const APPIMAGE_IDENTITY_VARS: [&str; 4] = ["APPDIR", "APPIMAGE", "ARGV0", "OWD"];
const ENV_LIST_SEPARATOR: char = ':';
const OPT_DIR: &str = "/opt";
const OPT_PREFIX: &str = "OrcaSlicer";
const OPT_EXECUTABLE: &str = "bin/orca-slicer";

pub fn detect_orca() -> Option<String> {
    find_on_path()
        .or_else(find_flatpak)
        .or_else(find_home_appimage)
        .or_else(find_opt_install)
}

fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[cfg(unix)]
fn is_executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    const ANY_EXECUTE_BITS: u32 = 0o111;
    fs::metadata(path)
        .map(|meta| meta.is_file() && meta.permissions().mode() & ANY_EXECUTE_BITS != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable_file(path: &Path) -> bool {
    path.is_file()
}

fn find_on_path() -> Option<String> {
    let path_var: OsString = env::var_os("PATH")?;
    let dirs: Vec<PathBuf> = env::split_paths(&path_var).collect();
    PATH_EXECUTABLE_NAMES
        .iter()
        .flat_map(|name| dirs.iter().map(move |dir| dir.join(name)))
        .find(|candidate| is_executable_file(candidate))
        .map(|found| path_to_string(&found))
}

fn find_flatpak() -> Option<String> {
    let installed: bool = Command::new("flatpak")
        .args(["info", FLATPAK_APP_ID])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false);
    installed.then(|| format!("flatpak run {FLATPAK_APP_ID}"))
}

fn sorted_entries(dir: &Path) -> Vec<(String, PathBuf)> {
    let mut entries: Vec<(String, PathBuf)> = fs::read_dir(dir)
        .map(|read| {
            read.filter_map(Result::ok)
                .map(|entry| {
                    (
                        entry.file_name().to_string_lossy().into_owned(),
                        entry.path(),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    entries.sort();
    entries
}

fn is_orca_appimage_name(name: &str) -> bool {
    let lower: String = name.to_ascii_lowercase();
    lower.contains(APPIMAGE_NAME_HINT) && lower.ends_with(APPIMAGE_SUFFIX)
}

fn find_home_appimage() -> Option<String> {
    let home: PathBuf = PathBuf::from(env::var_os("HOME")?);
    APPIMAGE_HOME_DIRS.iter().find_map(|relative| {
        sorted_entries(&home.join(relative))
            .into_iter()
            .find(|(name, path)| is_orca_appimage_name(name) && path.is_file())
            .map(|(_, path)| path_to_string(&path))
    })
}

fn without_appdir_entries(value: &str, appdir: &str) -> Option<String> {
    if !value.contains(appdir) {
        return Some(value.to_string());
    }
    let kept: Vec<&str> = value
        .split(ENV_LIST_SEPARATOR)
        .filter(|entry| !entry.contains(appdir))
        .collect();
    if kept.is_empty() {
        None
    } else {
        Some(kept.join(&ENV_LIST_SEPARATOR.to_string()))
    }
}

fn host_environment_overrides(
    appdir: &str,
    vars: impl Iterator<Item = (String, String)>,
) -> Vec<(String, Option<String>)> {
    vars.filter_map(|(key, value)| {
        if APPIMAGE_IDENTITY_VARS.contains(&key.as_str()) {
            return Some((key, None));
        }
        let cleaned: Option<String> = without_appdir_entries(&value, appdir);
        if cleaned.as_deref() == Some(value.as_str()) {
            None
        } else {
            Some((key, cleaned))
        }
    })
    .collect()
}

fn apply_host_environment(command: &mut Command) {
    let Some(appdir) = env::var_os("APPDIR") else {
        return;
    };
    let appdir_text: String = appdir.to_string_lossy().into_owned();
    for (key, value) in host_environment_overrides(&appdir_text, env::vars()) {
        match value {
            Some(cleaned) => command.env(key, cleaned),
            None => command.env_remove(key),
        };
    }
}

fn ensure_launchable(program: &str, spec: &str) -> Result<(), String> {
    let path: &Path = Path::new(program);
    if path.is_file() && !is_executable_file(path) {
        return Err(format!(
            "\"{spec}\" is not executable. Run: chmod +x \"{program}\""
        ));
    }
    Ok(())
}

fn find_opt_install() -> Option<String> {
    sorted_entries(Path::new(OPT_DIR))
        .into_iter()
        .filter(|(name, _)| name.starts_with(OPT_PREFIX))
        .map(|(_, path)| path.join(OPT_EXECUTABLE))
        .find(|executable| executable.is_file())
        .map(|executable| path_to_string(&executable))
}

fn split_launch_spec(spec: &str) -> Option<(String, Vec<String>)> {
    let trimmed: &str = spec.trim();
    if trimmed.is_empty() {
        return None;
    }
    if Path::new(trimmed).is_file() {
        return Some((trimmed.to_string(), Vec::new()));
    }
    let mut words = trimmed.split_whitespace().map(str::to_string);
    let program: String = words.next()?;
    Some((program, words.collect()))
}

pub fn launch(spec: &str, files: &[PathBuf]) -> Result<(), String> {
    let (program, leading_args): (String, Vec<String>) = split_launch_spec(spec)
        .ok_or_else(|| "The OrcaSlicer launch command is empty".to_string())?;
    ensure_launchable(&program, spec)?;
    let mut command: Command = Command::new(&program);
    command
        .args(&leading_args)
        .args(files)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    apply_host_environment(&mut command);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child: Child = command
        .spawn()
        .map_err(|e| format!("Could not launch OrcaSlicer with \"{spec}\": {e}"))?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_flatpak_spec_into_program_and_args() {
        let parsed: Option<(String, Vec<String>)> =
            split_launch_spec("flatpak run io.github.softfever.OrcaSlicer");
        assert_eq!(
            parsed,
            Some((
                "flatpak".to_string(),
                vec![
                    "run".to_string(),
                    "io.github.softfever.OrcaSlicer".to_string()
                ]
            ))
        );
        assert_eq!(split_launch_spec("   "), None);
    }

    #[test]
    fn recognises_appimage_names_case_insensitively() {
        assert!(is_orca_appimage_name("orcaslicer.appimage"));
        assert!(is_orca_appimage_name("OrcaSlicer_Linux_V2.3.0.AppImage"));
        assert!(!is_orca_appimage_name("orcaslicer.tar.gz"));
        assert!(!is_orca_appimage_name("PrusaSlicer.AppImage"));
    }

    #[test]
    fn strips_appdir_from_child_environment() {
        let appdir: &str = "/tmp/sltpreview.AppDir";
        let vars: Vec<(String, String)> = vec![
            ("APPDIR".to_string(), appdir.to_string()),
            ("APPIMAGE".to_string(), "/x/sltpreview.AppImage".to_string()),
            ("GTK_PATH".to_string(), format!("{appdir}//usr/lib/gtk-3.0")),
            (
                "XDG_DATA_DIRS".to_string(),
                format!("{appdir}/usr/share:/usr/share:/usr/local/share"),
            ),
            ("HOME".to_string(), "/home/user".to_string()),
        ];
        let overrides: Vec<(String, Option<String>)> =
            host_environment_overrides(appdir, vars.into_iter());
        assert_eq!(
            overrides,
            vec![
                ("APPDIR".to_string(), None),
                ("APPIMAGE".to_string(), None),
                ("GTK_PATH".to_string(), None),
                (
                    "XDG_DATA_DIRS".to_string(),
                    Some("/usr/share:/usr/local/share".to_string())
                ),
            ]
        );
    }

    #[test]
    fn launch_rejects_non_executable_file() {
        let dir: PathBuf = std::env::temp_dir().join(format!(
            "sltpreview-orca-{}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).expect("temp dir");
        let fake: PathBuf = dir.join("orcaslicer.appimage");
        fs::write(&fake, b"not a program").expect("write");
        let error: String = launch(&path_to_string(&fake), &[PathBuf::from("/tmp/model.stl")])
            .expect_err("must fail");
        assert!(error.contains("chmod +x"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn launch_rejects_empty_spec() {
        assert!(launch("", &[PathBuf::from("/tmp/model.stl")]).is_err());
    }
}
