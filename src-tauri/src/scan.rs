use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Component, Path, PathBuf};

use walkdir::{DirEntry, WalkDir};

use crate::fsutil::system_time_ms;
use crate::mesh::model_kind;

pub const PREVIEW_DIR_NAME: &str = "img";
const PREVIEW_EXTENSION: &str = "png";
const CUSTOM_PREVIEW_SUFFIX: &str = ".custom.png";
const RESERVED_NAME_PREFIX: &str = "_project";
const HIDDEN_NAME_PREFIX: char = '.';

pub struct ScannedFile {
    pub rel: String,
    pub folder_rel: String,
    pub stem: String,
    pub extension: String,
    pub size_bytes: u64,
    pub modified_ms: u64,
}

pub struct WalkResult {
    pub dirs: Vec<String>,
    pub files: Vec<ScannedFile>,
}

fn is_skipped(entry: &DirEntry) -> bool {
    if entry.depth() == 0 {
        return false;
    }
    let name = entry.file_name().to_string_lossy();
    name.starts_with(HIDDEN_NAME_PREFIX) || (entry.file_type().is_dir() && name == PREVIEW_DIR_NAME)
}

pub fn rel_path(root: &Path, path: &Path) -> Option<String> {
    let relative: &Path = path.strip_prefix(root).ok()?;
    let segments: Vec<String> = relative
        .components()
        .filter_map(|component| match component {
            Component::Normal(segment) => Some(segment.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect();
    Some(segments.join("/"))
}

pub fn parent_rel(rel: &str) -> &str {
    rel.rsplit_once('/').map(|(parent, _)| parent).unwrap_or("")
}

pub fn last_segment(rel: &str) -> &str {
    rel.rsplit_once('/').map(|(_, name)| name).unwrap_or(rel)
}

pub fn is_reserved_name(name: &str) -> bool {
    name.starts_with(HIDDEN_NAME_PREFIX) || name.starts_with(RESERVED_NAME_PREFIX)
}

pub fn is_model_candidate_name(name: &str) -> bool {
    !is_reserved_name(name) && model_kind(Path::new(name)).is_some()
}

fn scanned_file(root: &Path, entry: &DirEntry) -> Option<ScannedFile> {
    let path: &Path = entry.path();
    if !is_model_candidate_name(&entry.file_name().to_string_lossy()) {
        return None;
    }
    let metadata = entry.metadata().ok()?;
    let rel: String = rel_path(root, path)?;
    Some(ScannedFile {
        folder_rel: parent_rel(&rel).to_string(),
        stem: path.file_stem()?.to_string_lossy().into_owned(),
        extension: path.extension()?.to_string_lossy().into_owned(),
        size_bytes: metadata.len(),
        modified_ms: metadata.modified().map(system_time_ms).unwrap_or(0),
        rel,
    })
}

pub fn walk(root: &Path, include_files: bool) -> WalkResult {
    let mut result = WalkResult {
        dirs: Vec::new(),
        files: Vec::new(),
    };
    let entries = WalkDir::new(root)
        .min_depth(1)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|entry| !is_skipped(entry))
        .filter_map(Result::ok);
    for entry in entries {
        let file_type = entry.file_type();
        if file_type.is_dir() {
            if let Some(rel) = rel_path(root, entry.path()) {
                result.dirs.push(rel);
            }
        } else if include_files
            && file_type.is_file()
            && let Some(file) = scanned_file(root, &entry)
        {
            result.files.push(file);
        }
    }
    result
}

struct PreviewSource<'a> {
    folder_rel: &'a str,
    stem: &'a str,
    extension: &'a str,
}

fn join_rel(folder_rel: &str, name: &str) -> String {
    if folder_rel.is_empty() {
        name.to_string()
    } else {
        format!("{folder_rel}/{name}")
    }
}

pub fn preview_dir_rel(folder_rel: &str) -> String {
    join_rel(folder_rel, PREVIEW_DIR_NAME)
}

fn preview_rels(sources: &[PreviewSource]) -> Vec<String> {
    let mut stem_counts: HashMap<(&str, &str), usize> = HashMap::new();
    for source in sources {
        *stem_counts
            .entry((source.folder_rel, source.stem))
            .or_insert(0) += 1;
    }
    sources
        .iter()
        .map(|source| {
            let shared: bool = stem_counts
                .get(&(source.folder_rel, source.stem))
                .is_some_and(|count| *count > 1);
            let png_name: String = if shared {
                format!("{}.{}.{PREVIEW_EXTENSION}", source.stem, source.extension)
            } else {
                format!("{}.{PREVIEW_EXTENSION}", source.stem)
            };
            join_rel(&preview_dir_rel(source.folder_rel), &png_name)
        })
        .collect()
}

pub fn preview_rel_paths(files: &[ScannedFile]) -> Vec<String> {
    let sources: Vec<PreviewSource> = files
        .iter()
        .map(|file| PreviewSource {
            folder_rel: &file.folder_rel,
            stem: &file.stem,
            extension: &file.extension,
        })
        .collect();
    preview_rels(&sources)
}

pub fn custom_rel_for(rendered_rel: &str) -> String {
    let base: &str = rendered_rel
        .strip_suffix(&format!(".{PREVIEW_EXTENSION}"))
        .unwrap_or(rendered_rel);
    format!("{base}{CUSTOM_PREVIEW_SUFFIX}")
}

pub fn folder_model_names(root: &Path, folder_rel: &str) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(absolute(root, folder_rel))
        .map(|read| {
            read.filter_map(Result::ok)
                .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .filter(|name| is_model_candidate_name(name))
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

fn split_name(name: &str) -> (&str, &str) {
    name.rsplit_once('.').unwrap_or((name, ""))
}

pub fn preview_rels_for_names(folder_rel: &str, names: &[String]) -> BTreeMap<String, String> {
    let sources: Vec<PreviewSource> = names
        .iter()
        .map(|name| {
            let (stem, extension): (&str, &str) = split_name(name);
            PreviewSource {
                folder_rel,
                stem,
                extension,
            }
        })
        .collect();
    names.iter().cloned().zip(preview_rels(&sources)).collect()
}

pub fn folder_preview_rels(root: &Path, folder_rel: &str) -> BTreeMap<String, String> {
    preview_rels_for_names(folder_rel, &folder_model_names(root, folder_rel))
}

pub fn checked_absolute(root: &Path, rel: &str) -> Result<PathBuf, String> {
    let invalid: bool = rel.starts_with('/')
        || rel
            .split('/')
            .any(|segment| segment.is_empty() || segment == "." || segment == "..");
    if invalid && !rel.is_empty() {
        return Err(format!("Invalid relative path: {rel}"));
    }
    Ok(absolute(root, rel))
}

pub fn absolute(root: &Path, rel: &str) -> PathBuf {
    rel.split('/')
        .filter(|segment| !segment.is_empty())
        .fold(root.to_path_buf(), |path, segment| path.join(segment))
}

pub fn is_model_file(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|name| is_model_candidate_name(&name.to_string_lossy()))
        && path.is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(rel: &str) -> ScannedFile {
        let path: &Path = Path::new(rel);
        ScannedFile {
            rel: rel.to_string(),
            folder_rel: parent_rel(rel).to_string(),
            stem: path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default(),
            extension: path
                .extension()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default(),
            size_bytes: 0,
            modified_ms: 0,
        }
    }

    #[test]
    fn preview_names_disambiguate_shared_stems() {
        let files: Vec<ScannedFile> = vec![
            file("toys/cube.stl"),
            file("toys/cube.3mf"),
            file("toys/ball.stl"),
            file("top.STL"),
        ];
        assert_eq!(
            preview_rel_paths(&files),
            vec![
                "toys/img/cube.stl.png".to_string(),
                "toys/img/cube.3mf.png".to_string(),
                "toys/img/ball.png".to_string(),
                "img/top.png".to_string(),
            ]
        );
    }
}
