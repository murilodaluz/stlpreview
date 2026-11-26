use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::cache::{self, CacheEntry, CacheFile};
use crate::fsutil::{modified_ms, now_ms};
use crate::mesh::model_kind;
use crate::project::{self, ProjectPreview};
use crate::render::render_model;
use crate::scan::{
    self, ScannedFile, absolute, custom_rel_for, is_model_file, last_segment, parent_rel,
};

pub const SCAN_PROGRESS_EVENT: &str = "scan-progress";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModelKind {
    #[serde(rename = "stl")]
    Stl,
    #[serde(rename = "3mf")]
    ThreeMf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderNode {
    pub name: String,
    pub rel_path: String,
    pub children: Vec<FolderNode>,
    pub file_count: u32,
    pub total_count: u32,
    pub is_project: bool,
    pub preview_path: Option<String>,
    pub preview_rendered_ms: Option<u64>,
    pub custom_preview: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelItem {
    pub path: String,
    pub rel_path: String,
    pub folder_rel_path: String,
    pub name: String,
    pub kind: ModelKind,
    pub size_bytes: u64,
    pub modified_ms: u64,
    pub preview_path: Option<String>,
    pub preview_rendered_ms: Option<u64>,
    pub triangles: Option<u64>,
    pub error: Option<String>,
    pub custom_preview: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Library {
    pub root: String,
    pub tree: FolderNode,
    pub items: Vec<ModelItem>,
    pub scanned_at_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanProgress {
    pub done: u32,
    pub total: u32,
    pub current: String,
}

pub enum CompositeScope {
    Skip,
    All,
    Around(Vec<String>),
}

fn is_within(folder_rel: &str, ancestor_rel: &str) -> bool {
    folder_rel == ancestor_rel
        || (ancestor_rel.is_empty()
            || folder_rel
                .strip_prefix(ancestor_rel)
                .is_some_and(|rest| rest.starts_with('/')))
}

impl CompositeScope {
    fn includes(&self, project_rel: &str) -> bool {
        match self {
            CompositeScope::Skip => false,
            CompositeScope::All => true,
            CompositeScope::Around(rels) => rels.iter().any(|rel| is_within(rel, project_rel)),
        }
    }
}

pub fn validate_root(root: &str) -> Result<PathBuf, String> {
    let path: PathBuf = PathBuf::from(root);
    if !path.is_dir() {
        return Err(format!("Folder not found: {root}"));
    }
    fs::canonicalize(&path).map_err(|e| format!("Cannot resolve folder {root}: {e}"))
}

pub fn load_library(root: &str) -> Result<Library, String> {
    let root_path: PathBuf = validate_root(root)?;
    load_with_composites(root, &root_path, CompositeScope::Skip)
}

pub fn load_with_composites(
    root: &str,
    root_path: &Path,
    scope: CompositeScope,
) -> Result<Library, String> {
    let cache: CacheFile = cache::load_cache(root_path)?;
    let dirs: Vec<String> = scan::walk(root_path, false).dirs;
    build_library(root, root_path, &dirs, &cache, &scope)
}

struct PlannedFile<'a> {
    file: &'a ScannedFile,
    preview_rel: String,
    fresh: Option<CacheEntry>,
}

fn plan_files<'a>(
    root: &Path,
    files: &'a [ScannedFile],
    previous: &CacheFile,
) -> Vec<PlannedFile<'a>> {
    files
        .iter()
        .zip(scan::preview_rel_paths(files))
        .map(|(file, preview_rel)| {
            let fresh: Option<CacheEntry> = previous
                .entries
                .get(&file.rel)
                .filter(|entry| {
                    entry.size_bytes == file.size_bytes
                        && entry.modified_ms == file.modified_ms
                        && entry.preview_rel.as_deref() == Some(preview_rel.as_str())
                        && absolute(root, &preview_rel).is_file()
                })
                .cloned();
            PlannedFile {
                file,
                preview_rel,
                fresh,
            }
        })
        .collect()
}

fn render_entry(root: &Path, planned: &PlannedFile) -> CacheEntry {
    let outcome: Result<u64, String> = render_model(
        &absolute(root, &planned.file.rel),
        &absolute(root, &planned.preview_rel),
    );
    let base = CacheEntry {
        size_bytes: planned.file.size_bytes,
        modified_ms: planned.file.modified_ms,
        ..CacheEntry::default()
    };
    match outcome {
        Ok(triangles) => CacheEntry {
            preview_rel: Some(planned.preview_rel.clone()),
            preview_rendered_ms: Some(now_ms()),
            triangles: Some(triangles),
            ..base
        },
        Err(error) => CacheEntry {
            error: Some(error),
            ..base
        },
    }
}

fn reconcile_custom_preview(
    root: &Path,
    planned: &PlannedFile,
    previous: Option<&CacheEntry>,
) -> Option<String> {
    let expected_rel: String = custom_rel_for(&planned.preview_rel);
    let expected: PathBuf = absolute(root, &expected_rel);
    let recorded: Option<PathBuf> = previous
        .and_then(|entry| entry.custom_preview_rel.as_deref())
        .filter(|recorded_rel| *recorded_rel != expected_rel)
        .map(|recorded_rel| absolute(root, recorded_rel))
        .filter(|recorded| recorded.is_file());
    if let Some(recorded) = recorded
        && !expected.exists()
    {
        let _ = fs::rename(&recorded, &expected);
    }
    expected.is_file().then_some(expected_rel)
}

pub fn refresh_library(
    root: &str,
    report: &(dyn Fn(ScanProgress) + Sync),
) -> Result<Library, String> {
    let root_path: PathBuf = validate_root(root)?;
    let previous: CacheFile = cache::load_cache(&root_path)?;
    let walked = scan::walk(&root_path, true);
    let planned: Vec<PlannedFile> = plan_files(&root_path, &walked.files, &previous);
    let stale: Vec<&PlannedFile> = planned.iter().filter(|plan| plan.fresh.is_none()).collect();
    let total: u32 = u32::try_from(stale.len()).unwrap_or(u32::MAX);
    let done = AtomicU32::new(0);
    report(ScanProgress {
        done: 0,
        total,
        current: String::new(),
    });
    let mut rendered: HashMap<String, CacheEntry> = stale
        .par_iter()
        .map(|plan| {
            let entry: CacheEntry = render_entry(&root_path, plan);
            let finished: u32 = done.fetch_add(1, Ordering::SeqCst) + 1;
            report(ScanProgress {
                done: finished,
                total,
                current: plan.file.rel.clone(),
            });
            (plan.file.rel.clone(), entry)
        })
        .collect();
    let entries: BTreeMap<String, CacheEntry> = planned
        .iter()
        .filter_map(|plan| {
            let base: CacheEntry = plan
                .fresh
                .clone()
                .or_else(|| rendered.remove(&plan.file.rel))?;
            let custom_preview_rel: Option<String> =
                reconcile_custom_preview(&root_path, plan, previous.entries.get(&plan.file.rel));
            Some((
                plan.file.rel.clone(),
                CacheEntry {
                    custom_preview_rel,
                    ..base
                },
            ))
        })
        .collect();
    let cache = CacheFile {
        version: cache::CACHE_VERSION,
        scanned_at_ms: Some(now_ms()),
        entries,
    };
    cache::save_cache(&root_path, &cache)?;
    build_library(root, &root_path, &walked.dirs, &cache, &CompositeScope::All)
}

pub fn custom_rels_by_model<'a>(
    root_path: &Path,
    rels: impl Iterator<Item = &'a str>,
) -> HashMap<String, String> {
    let mut by_folder: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for rel in rels {
        by_folder.entry(parent_rel(rel)).or_default().push(rel);
    }
    by_folder
        .into_iter()
        .flat_map(|(folder_rel, members)| {
            let names: BTreeMap<String, String> = scan::folder_preview_rels(root_path, folder_rel);
            members
                .into_iter()
                .filter_map(|rel| {
                    names
                        .get(last_segment(rel))
                        .map(|rendered_rel| (rel.to_string(), custom_rel_for(rendered_rel)))
                })
                .collect::<Vec<(String, String)>>()
        })
        .collect()
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn model_item(
    root_path: &Path,
    rel: &str,
    entry: &CacheEntry,
    custom_rel: Option<&String>,
) -> Option<ModelItem> {
    let path: PathBuf = absolute(root_path, rel);
    if !is_model_file(&path) {
        return None;
    }
    let custom: Option<(PathBuf, u64)> = custom_rel
        .map(|custom_rel| absolute(root_path, custom_rel))
        .and_then(|custom_path| modified_ms(&custom_path).map(|modified| (custom_path, modified)));
    let rendered: Option<(PathBuf, Option<u64>)> = entry
        .preview_rel
        .as_deref()
        .map(|preview_rel| absolute(root_path, preview_rel))
        .filter(|preview| preview.is_file())
        .map(|preview| (preview, entry.preview_rendered_ms));
    let custom_preview: bool = custom.is_some();
    let (preview_path, preview_rendered_ms): (Option<String>, Option<u64>) =
        match (custom, rendered) {
            (Some((custom_path, modified)), _) => (Some(path_string(&custom_path)), Some(modified)),
            (None, Some((preview, rendered_ms))) => (Some(path_string(&preview)), rendered_ms),
            (None, None) => (None, None),
        };
    Some(ModelItem {
        path: path_string(&path),
        rel_path: rel.to_string(),
        folder_rel_path: parent_rel(rel).to_string(),
        name: last_segment(rel).to_string(),
        kind: model_kind(&path)?,
        size_bytes: entry.size_bytes,
        modified_ms: entry.modified_ms,
        preview_path,
        preview_rendered_ms,
        triangles: entry.triangles,
        error: entry.error.clone(),
        custom_preview,
    })
}

fn rebuild_composites(
    root_path: &Path,
    projects: &[&str],
    items: &[ModelItem],
    scope: &CompositeScope,
) -> Result<(), String> {
    projects
        .par_iter()
        .filter(|project_rel| scope.includes(project_rel))
        .map(|project_rel| {
            let previews: Vec<PathBuf> = items
                .iter()
                .filter(|item| is_within(&item.folder_rel_path, project_rel))
                .filter_map(|item| item.preview_path.as_deref().map(PathBuf::from))
                .collect();
            project::write_composite(&absolute(root_path, project_rel), &previews)
        })
        .collect::<Result<Vec<()>, String>>()
        .map(|_| ())
}

pub fn build_library(
    root: &str,
    root_path: &Path,
    dirs: &[String],
    cache: &CacheFile,
    scope: &CompositeScope,
) -> Result<Library, String> {
    let custom_rels: HashMap<String, String> =
        custom_rels_by_model(root_path, cache.entries.keys().map(String::as_str));
    let items: Vec<ModelItem> = cache
        .entries
        .iter()
        .filter_map(|(rel, entry)| model_item(root_path, rel, entry, custom_rels.get(rel)))
        .collect();
    let projects: Vec<&str> = dirs
        .iter()
        .map(String::as_str)
        .filter(|dir| project::is_project(&absolute(root_path, dir)))
        .collect();
    rebuild_composites(root_path, &projects, &items, scope)?;
    let project_previews: HashMap<&str, ProjectPreview> = projects
        .iter()
        .map(|project_rel| {
            (
                *project_rel,
                project::project_preview(&absolute(root_path, project_rel)),
            )
        })
        .collect();
    let mut file_counts: HashMap<&str, u32> = HashMap::new();
    for item in &items {
        *file_counts
            .entry(item.folder_rel_path.as_str())
            .or_insert(0) += 1;
    }
    let root_name: String = root_path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| root.to_string());
    let context = TreeContext {
        children_of: children_of(dirs),
        file_counts: &file_counts,
        project_previews: &project_previews,
    };
    let tree: FolderNode = build_node(root_name, "", &context);
    Ok(Library {
        root: root.to_string(),
        tree,
        items,
        scanned_at_ms: cache.scanned_at_ms,
    })
}

struct TreeContext<'a> {
    children_of: HashMap<&'a str, Vec<&'a str>>,
    file_counts: &'a HashMap<&'a str, u32>,
    project_previews: &'a HashMap<&'a str, ProjectPreview>,
}

fn children_of(dirs: &[String]) -> HashMap<&str, Vec<&str>> {
    let mut children: HashMap<&str, Vec<&str>> = HashMap::new();
    for dir in dirs {
        children
            .entry(parent_rel(dir))
            .or_default()
            .push(dir.as_str());
    }
    children
}

fn build_node(name: String, rel: &str, context: &TreeContext) -> FolderNode {
    let mut children: Vec<FolderNode> = context
        .children_of
        .get(rel)
        .map(|kids| {
            kids.iter()
                .map(|kid| build_node(last_segment(kid).to_string(), kid, context))
                .collect()
        })
        .unwrap_or_default();
    children.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| a.name.cmp(&b.name))
    });
    let file_count: u32 = context.file_counts.get(rel).copied().unwrap_or(0);
    let total_count: u32 = file_count + children.iter().map(|child| child.total_count).sum::<u32>();
    let preview: Option<&ProjectPreview> = context.project_previews.get(rel);
    FolderNode {
        name,
        rel_path: rel.to_string(),
        children,
        file_count,
        total_count,
        is_project: preview.is_some(),
        preview_path: preview
            .and_then(|found| found.path.as_deref())
            .map(path_string),
        preview_rendered_ms: preview.and_then(|found| found.modified_ms),
        custom_preview: preview.is_some_and(|found| found.custom),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{PROJECT_MARKER_NAME, composite_path, custom_path};
    use crate::testutil::{
        ascii_stl, binary_stl, cube_triangles, find_item, find_node, minimal_3mf, quiet_refresh,
        root_string, solid_png, unique_temp_dir, write_file,
    };
    use std::sync::Mutex;

    fn collect_progress(root: &str) -> (Library, Vec<ScanProgress>) {
        let events: Mutex<Vec<ScanProgress>> = Mutex::new(Vec::new());
        let report = |progress: ScanProgress| {
            if let Ok(mut guard) = events.lock() {
                guard.push(progress);
            }
        };
        let library: Library = refresh_library(root, &report).expect("refresh succeeds");
        let collected: Vec<ScanProgress> = events.into_inner().expect("events");
        (library, collected)
    }

    #[test]
    fn refresh_renders_diffs_and_load_reads_cache() {
        let root: PathBuf = unique_temp_dir("library");
        let toys: PathBuf = root.join("toys");
        write_file(&toys, "cube.stl", &binary_stl(&cube_triangles(2.0)));
        minimal_3mf(&toys, "cube.3mf", true, false);
        write_file(&root, "top.stl", ascii_stl(&cube_triangles(1.0)).as_bytes());
        write_file(
            &root.join("img"),
            "ignored.stl",
            &binary_stl(&cube_triangles(1.0)),
        );
        write_file(
            &root.join(".hidden"),
            "ignored.stl",
            &binary_stl(&cube_triangles(1.0)),
        );
        write_file(
            &root.join("empty").join("deeper"),
            "notes.txt",
            b"not a model",
        );
        let root_str: String = root.to_string_lossy().into_owned();

        let (library, events) = collect_progress(&root_str);
        assert_eq!(library.items.len(), 3);
        assert_eq!(events.len(), 4);
        assert_eq!(
            events.last().map(|event| (event.done, event.total)),
            Some((3, 3))
        );
        assert!(library.scanned_at_ms.is_some());
        let rel_paths: Vec<&str> = library
            .items
            .iter()
            .map(|item| item.rel_path.as_str())
            .collect();
        assert_eq!(rel_paths, vec!["top.stl", "toys/cube.3mf", "toys/cube.stl"]);
        for item in &library.items {
            assert_eq!(item.error, None);
            let preview: &str = item.preview_path.as_deref().expect("preview path");
            assert!(Path::new(preview).is_file());
        }
        assert!(toys.join("img").join("cube.stl.png").is_file());
        assert!(toys.join("img").join("cube.3mf.png").is_file());
        assert!(root.join("img").join("top.png").is_file());
        assert!(cache::cache_path(&root).is_file());
        assert_eq!(library.tree.total_count, 3);
        assert_eq!(library.tree.file_count, 1);
        let child_names: Vec<&str> = library
            .tree
            .children
            .iter()
            .map(|c| c.name.as_str())
            .collect();
        assert_eq!(child_names, vec!["empty", "toys"]);
        assert_eq!(library.tree.children[1].file_count, 2);
        assert_eq!(
            library.tree.children[0].children[0].rel_path,
            "empty/deeper"
        );

        let (_, second_events) = collect_progress(&root_str);
        assert_eq!(second_events.len(), 1);
        assert_eq!(second_events[0].total, 0);

        let _ = std::fs::remove_file(root.join("top.stl"));
        let loaded: Library = load_library(&root_str).expect("load succeeds");
        assert_eq!(loaded.items.len(), 2);
        assert_eq!(loaded.tree.total_count, 2);

        let serialized: String = serde_json::to_string(&loaded).expect("serializes");
        for key in [
            "relPath",
            "folderRelPath",
            "previewPath",
            "sizeBytes",
            "scannedAtMs",
            "totalCount",
            "\"3mf\"",
        ] {
            assert!(serialized.contains(key), "missing {key}");
        }
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn load_library_rejects_missing_root() {
        assert!(load_library("/definitely/not/a/real/folder/sltpreview").is_err());
    }

    #[test]
    fn load_library_reports_projects_and_custom_previews_from_disk() {
        let root: PathBuf = unique_temp_dir("library-phase2");
        let kit: PathBuf = root.join("kit");
        write_file(&kit, "a.stl", &binary_stl(&cube_triangles(2.0)));
        write_file(&kit, "b.stl", &binary_stl(&cube_triangles(3.0)));
        write_file(&root, "loose.stl", &binary_stl(&cube_triangles(1.0)));
        let root_str: String = root_string(&root);
        quiet_refresh(&root);
        write_file(&kit, PROJECT_MARKER_NAME, br#"{"version":1}"#);
        solid_png(&kit.join("img"), "a.custom.png", 16, 16);

        let loaded: Library = load_library(&root_str).expect("load");
        let a: &ModelItem = find_item(&loaded, "kit/a.stl").expect("a");
        assert!(a.custom_preview);
        assert_eq!(
            a.preview_path.as_deref(),
            Some(
                kit.join("img")
                    .join("a.custom.png")
                    .to_string_lossy()
                    .as_ref()
            )
        );
        assert!(a.preview_rendered_ms.is_some());
        let b: &ModelItem = find_item(&loaded, "kit/b.stl").expect("b");
        assert!(!b.custom_preview);
        assert_eq!(
            b.preview_path.as_deref(),
            Some(kit.join("img").join("b.png").to_string_lossy().as_ref())
        );
        let node: &FolderNode = find_node(&loaded.tree, "kit").expect("kit");
        assert!(node.is_project);
        assert_eq!(node.preview_path, None);
        assert!(!node.custom_preview);
        assert!(!composite_path(&kit).exists());
        assert!(!loaded.tree.is_project);
        assert_eq!(loaded.tree.preview_path, None);

        quiet_refresh(&root);
        let refreshed: Library = load_library(&root_str).expect("load");
        let node: &FolderNode = find_node(&refreshed.tree, "kit").expect("kit");
        assert_eq!(
            node.preview_path.as_deref(),
            Some(composite_path(&kit).to_string_lossy().as_ref())
        );
        assert!(!node.custom_preview);
        let cache: CacheFile = cache::load_cache(&root).expect("cache");
        assert_eq!(
            cache
                .entries
                .get("kit/a.stl")
                .and_then(|e| e.custom_preview_rel.clone()),
            Some("kit/img/a.custom.png".to_string())
        );

        solid_png(&kit.join("img"), "_project.custom.png", 16, 16);
        let customised: Library = load_library(&root_str).expect("load");
        let node: &FolderNode = find_node(&customised.tree, "kit").expect("kit");
        assert!(node.custom_preview);
        assert_eq!(
            node.preview_path.as_deref(),
            Some(custom_path(&kit).to_string_lossy().as_ref())
        );
        let serialized: String = serde_json::to_string(&customised).expect("serializes");
        for key in ["isProject", "customPreview", "previewRenderedMs"] {
            assert!(serialized.contains(key), "missing {key}");
        }
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn refresh_moves_custom_preview_when_stem_clash_appears() {
        let root: PathBuf = unique_temp_dir("library-custom-migrate");
        write_file(&root, "cube.stl", &binary_stl(&cube_triangles(2.0)));
        let root_str: String = root_string(&root);
        quiet_refresh(&root);
        solid_png(&root.join("img"), "cube.custom.png", 16, 16);
        quiet_refresh(&root);
        minimal_3mf(&root, "cube.3mf", true, false);
        let refreshed: Library = quiet_refresh(&root);
        assert!(!root.join("img").join("cube.custom.png").exists());
        assert!(root.join("img").join("cube.stl.custom.png").is_file());
        assert!(find_item(&refreshed, "cube.stl").is_some_and(|item| item.custom_preview));
        assert!(find_item(&refreshed, "cube.3mf").is_some_and(|item| !item.custom_preview));
        assert!(load_library(&root_str).is_ok());
        let _ = std::fs::remove_dir_all(root);
    }
}
