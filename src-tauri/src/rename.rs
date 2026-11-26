use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::cache::{self, CacheEntry, CacheFile};
use crate::fsutil::{remove_if_exists, temp_sibling};
use crate::library::{self, CompositeScope, Library, ModelKind};
use crate::mesh::model_kind;
use crate::scan::{
    self, absolute, custom_rel_for, is_model_file, is_reserved_name, last_segment, parent_rel,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PreviewKind {
    Rendered,
    Custom,
}

struct PreviewMove {
    kind: PreviewKind,
    from_rel: String,
    to_rel: String,
}

struct PlannedRename {
    before_rel: String,
    after_rel: String,
    rendered: PreviewMove,
    custom: PreviewMove,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum MoveOutcome {
    Moved,
    Missing,
    Failed(String),
}

enum MoveStage {
    Finished(MoveOutcome),
    Staged(PathBuf),
}

fn join_rel(folder_rel: &str, name: &str) -> String {
    if folder_rel.is_empty() {
        name.to_string()
    } else {
        format!("{folder_rel}/{name}")
    }
}

fn extension_of(name: &str) -> &str {
    name.rsplit_once('.')
        .map(|(_, extension)| extension)
        .unwrap_or("")
}

pub fn resolve_new_name(old_name: &str, new_name: &str) -> Result<String, String> {
    if new_name.trim().is_empty() {
        return Err("The new name cannot be empty".to_string());
    }
    if new_name.contains(['/', '\\', '\0']) || new_name == "." || new_name == ".." {
        return Err(format!("\"{new_name}\" is not a valid file name"));
    }
    if is_reserved_name(new_name) {
        return Err(format!(
            "\"{new_name}\" cannot be used: names starting with \".\" or \"_project\" are reserved"
        ));
    }
    let old_kind: ModelKind = model_kind(Path::new(old_name))
        .ok_or_else(|| format!("{old_name} is not an STL or 3MF file"))?;
    match model_kind(Path::new(new_name)) {
        Some(new_kind) if new_kind != old_kind => Err(format!(
            "Changing the file type is not supported: {old_name} must keep its .{} extension",
            extension_of(old_name).to_ascii_lowercase()
        )),
        Some(_) => Ok(new_name.to_string()),
        None => Ok(format!("{new_name}.{}", extension_of(old_name))),
    }
}

fn existing_rel(root: &Path, recorded: Option<&str>, fallback: &str) -> String {
    recorded
        .filter(|rel| absolute(root, rel).is_file())
        .unwrap_or(fallback)
        .to_string()
}

fn plan_renames(
    root: &Path,
    folder_rel: &str,
    old_name: &str,
    new_name: &str,
    before: &BTreeMap<String, String>,
    after: &BTreeMap<String, String>,
    cache: &CacheFile,
) -> Vec<PlannedRename> {
    after
        .iter()
        .filter_map(|(name, after_rendered)| {
            let before_name: &str = if name == new_name { old_name } else { name };
            let before_rendered: &String = before.get(before_name)?;
            if name != new_name && before_rendered == after_rendered {
                return None;
            }
            let before_rel: String = join_rel(folder_rel, before_name);
            let entry: Option<&CacheEntry> = cache.entries.get(&before_rel);
            let rendered_from: String = existing_rel(
                root,
                entry.and_then(|found| found.preview_rel.as_deref()),
                before_rendered,
            );
            let custom_from: String = existing_rel(
                root,
                entry.and_then(|found| found.custom_preview_rel.as_deref()),
                &custom_rel_for(before_rendered),
            );
            Some(PlannedRename {
                before_rel,
                after_rel: join_rel(folder_rel, name),
                rendered: PreviewMove {
                    kind: PreviewKind::Rendered,
                    from_rel: rendered_from,
                    to_rel: after_rendered.clone(),
                },
                custom: PreviewMove {
                    kind: PreviewKind::Custom,
                    from_rel: custom_from,
                    to_rel: custom_rel_for(after_rendered),
                },
            })
        })
        .collect()
}

fn stage_move(root: &Path, preview: &PreviewMove) -> MoveStage {
    if preview.from_rel == preview.to_rel {
        return MoveStage::Finished(MoveOutcome::Moved);
    }
    let from: PathBuf = absolute(root, &preview.from_rel);
    if !from.is_file() {
        return MoveStage::Finished(MoveOutcome::Missing);
    }
    let temp: PathBuf = temp_sibling(&absolute(root, &preview.to_rel));
    match fs::rename(&from, &temp) {
        Ok(()) => MoveStage::Staged(temp),
        Err(error) => MoveStage::Finished(MoveOutcome::Failed(format!(
            "Cannot move {}: {error}",
            preview.from_rel
        ))),
    }
}

fn remove_orphan_custom(root: &Path, preview: &PreviewMove, moves: &[&PreviewMove]) -> MoveOutcome {
    let claimed: bool = moves.iter().any(|other| other.from_rel == preview.to_rel);
    if preview.kind != PreviewKind::Custom || claimed {
        return MoveOutcome::Missing;
    }
    match remove_if_exists(&absolute(root, &preview.to_rel)) {
        Ok(()) => MoveOutcome::Missing,
        Err(error) => MoveOutcome::Failed(error),
    }
}

fn finish_move(root: &Path, preview: &PreviewMove, temp: &Path) -> MoveOutcome {
    let to: PathBuf = absolute(root, &preview.to_rel);
    match fs::rename(temp, &to) {
        Ok(()) => MoveOutcome::Moved,
        Err(error) => {
            let restored: bool = fs::rename(temp, absolute(root, &preview.from_rel)).is_ok();
            let left_at: String = if restored {
                preview.from_rel.clone()
            } else {
                temp.display().to_string()
            };
            MoveOutcome::Failed(format!(
                "Cannot move {} to {}: {error} (left at {left_at})",
                preview.from_rel, preview.to_rel
            ))
        }
    }
}

fn apply_moves(root: &Path, moves: &[&PreviewMove]) -> Vec<MoveOutcome> {
    let stages: Vec<MoveStage> = moves
        .iter()
        .map(|preview| stage_move(root, preview))
        .collect();
    let cleared: Vec<MoveStage> = moves
        .iter()
        .zip(stages)
        .map(|(preview, stage)| match stage {
            MoveStage::Finished(MoveOutcome::Missing) => {
                MoveStage::Finished(remove_orphan_custom(root, preview, moves))
            }
            other => other,
        })
        .collect();
    moves
        .iter()
        .zip(cleared)
        .map(|(preview, stage)| match stage {
            MoveStage::Finished(outcome) => outcome,
            MoveStage::Staged(temp) => finish_move(root, preview, &temp),
        })
        .collect()
}

fn rewritten_entry(
    root: &Path,
    entry: CacheEntry,
    plan: &PlannedRename,
    rendered: &MoveOutcome,
) -> CacheEntry {
    let preview_rel: Option<String> = match (entry.preview_rel.is_some(), rendered) {
        (true, MoveOutcome::Moved) => Some(plan.rendered.to_rel.clone()),
        (true, MoveOutcome::Failed(_)) => Some(plan.rendered.from_rel.clone()),
        _ => entry.preview_rel.clone(),
    };
    let custom_preview_rel: Option<String> = [&plan.custom.to_rel, &plan.custom.from_rel]
        .into_iter()
        .find(|rel| absolute(root, rel).is_file())
        .cloned();
    CacheEntry {
        preview_rel,
        custom_preview_rel,
        ..entry
    }
}

fn rewrite_cache(
    root: &Path,
    cache: &mut CacheFile,
    plans: &[PlannedRename],
    outcomes: &[MoveOutcome],
) {
    let rewritten: Vec<(String, CacheEntry)> = plans
        .iter()
        .zip(outcomes.chunks_exact(2))
        .filter_map(|(plan, pair)| {
            let entry: CacheEntry = cache.entries.remove(&plan.before_rel)?;
            Some((
                plan.after_rel.clone(),
                rewritten_entry(root, entry, plan, &pair[0]),
            ))
        })
        .collect();
    cache.entries.extend(rewritten);
}

pub fn rename_model(root: &str, rel: &str, new_name: &str) -> Result<Library, String> {
    let root_path: PathBuf = library::validate_root(root)?;
    let old_path: PathBuf = scan::checked_absolute(&root_path, rel)?;
    if rel.is_empty() || !is_model_file(&old_path) {
        return Err(format!("Model not found: {rel}"));
    }
    let folder_rel: String = parent_rel(rel).to_string();
    let old_name: String = last_segment(rel).to_string();
    let target_name: String = resolve_new_name(&old_name, new_name)?;
    if target_name == old_name {
        return library::load_with_composites(root, &root_path, CompositeScope::Skip);
    }
    let new_path: PathBuf = old_path.with_file_name(&target_name);
    if new_path.symlink_metadata().is_ok() {
        return Err(format!("A file named {target_name} already exists"));
    }
    let mut cache: CacheFile = cache::load_cache(&root_path)?;
    let before: BTreeMap<String, String> = scan::folder_preview_rels(&root_path, &folder_rel);
    fs::rename(&old_path, &new_path)
        .map_err(|e| format!("Cannot rename {old_name} to {target_name}: {e}"))?;
    let after: BTreeMap<String, String> = scan::folder_preview_rels(&root_path, &folder_rel);
    let plans: Vec<PlannedRename> = plan_renames(
        &root_path,
        &folder_rel,
        &old_name,
        &target_name,
        &before,
        &after,
        &cache,
    );
    let moves: Vec<&PreviewMove> = plans
        .iter()
        .flat_map(|plan| [&plan.rendered, &plan.custom])
        .collect();
    let outcomes: Vec<MoveOutcome> = apply_moves(&root_path, &moves);
    rewrite_cache(&root_path, &mut cache, &plans, &outcomes);
    cache::save_cache(&root_path, &cache)?;
    let library: Library =
        library::load_with_composites(root, &root_path, CompositeScope::Around(vec![folder_rel]))?;
    let failures: Vec<String> = outcomes
        .into_iter()
        .filter_map(|outcome| match outcome {
            MoveOutcome::Failed(message) => Some(message),
            MoveOutcome::Moved | MoveOutcome::Missing => None,
        })
        .collect();
    if failures.is_empty() {
        Ok(library)
    } else {
        Err(format!(
            "Renamed {old_name} to {target_name}, but some previews could not be moved: {}",
            failures.join("; ")
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::custom_preview::set_custom_preview;
    use crate::library::{ModelItem, ScanProgress, refresh_library};
    use crate::project::{composite_path, set_project};
    use crate::testutil::{
        binary_stl, cube_triangles, find_item, minimal_3mf, quiet_refresh, root_string, solid_png,
        unique_temp_dir, write_file,
    };
    use std::sync::Mutex;

    #[test]
    fn new_names_are_validated_and_completed() {
        assert_eq!(
            resolve_new_name("cube.stl", "box"),
            Ok("box.stl".to_string())
        );
        assert_eq!(
            resolve_new_name("cube.STL", "box"),
            Ok("box.STL".to_string())
        );
        assert_eq!(
            resolve_new_name("cube.stl", "Box.STL"),
            Ok("Box.STL".to_string())
        );
        assert_eq!(
            resolve_new_name("cube.3mf", "v1.2"),
            Ok("v1.2.3mf".to_string())
        );
        for bad in [
            "",
            "   ",
            "a/b",
            "a\\b",
            ".",
            "..",
            ".hidden",
            "_project",
            "_projectx.stl",
        ] {
            assert!(
                resolve_new_name("cube.stl", bad).is_err(),
                "accepted {bad:?}"
            );
        }
        assert!(resolve_new_name("cube.stl", "cube.3mf").is_err());
        assert!(resolve_new_name("cube.3mf", "cube.STL").is_err());
    }

    fn item_of<'a>(library: &'a Library, rel: &str) -> &'a ModelItem {
        find_item(library, rel).expect("item present")
    }

    #[test]
    fn rename_moves_previews_and_preserves_cache_entry() {
        let root: PathBuf = unique_temp_dir("rename-happy");
        let images: PathBuf = unique_temp_dir("rename-happy-images");
        let toys: PathBuf = root.join("toys");
        let img: PathBuf = toys.join("img");
        write_file(&toys, "cube.stl", &binary_stl(&cube_triangles(2.0)));
        write_file(&toys, "ball.stl", &binary_stl(&cube_triangles(3.0)));
        let root_str: String = root_string(&root);
        quiet_refresh(&root);
        set_project(&root_str, "toys", true).expect("mark");
        let png: PathBuf = solid_png(&images, "cover.png", 32, 32);
        set_custom_preview(
            &root_str,
            "toys/cube.stl",
            false,
            Some(&png.to_string_lossy()),
        )
        .expect("custom");
        let before: CacheEntry = cache::load_cache(&root)
            .expect("cache")
            .entries
            .get("toys/cube.stl")
            .cloned()
            .expect("entry");
        let ball_before: Library = library::load_library(&root_str).expect("load");
        let ball_rendered_ms: Option<u64> =
            item_of(&ball_before, "toys/ball.stl").preview_rendered_ms;
        assert!(ball_rendered_ms.is_some());

        let renamed: Library = rename_model(&root_str, "toys/cube.stl", "box").expect("rename");
        assert!(!toys.join("cube.stl").exists());
        assert!(toys.join("box.stl").is_file());
        assert!(!img.join("cube.png").exists());
        assert!(!img.join("cube.custom.png").exists());
        assert!(img.join("box.png").is_file());
        assert!(img.join("box.custom.png").is_file());
        assert!(composite_path(&toys).is_file());
        let item: &ModelItem = item_of(&renamed, "toys/box.stl");
        assert!(item.custom_preview);
        assert_eq!(item.name, "box.stl");
        assert!(find_item(&renamed, "toys/cube.stl").is_none());

        let cache_after: CacheFile = cache::load_cache(&root).expect("cache");
        assert!(!cache_after.entries.contains_key("toys/cube.stl"));
        let after: CacheEntry = cache_after
            .entries
            .get("toys/box.stl")
            .cloned()
            .expect("renamed entry");
        assert_eq!(
            after,
            CacheEntry {
                preview_rel: Some("toys/img/box.png".to_string()),
                custom_preview_rel: Some("toys/img/box.custom.png".to_string()),
                ..before
            }
        );

        let ball_renamed: Library =
            rename_model(&root_str, "toys/ball.stl", "sphere.stl").expect("rename");
        assert_eq!(
            item_of(&ball_renamed, "toys/sphere.stl").preview_rendered_ms,
            ball_rendered_ms
        );
        let events: Mutex<Vec<ScanProgress>> = Mutex::new(Vec::new());
        let report = |progress: ScanProgress| {
            if let Ok(mut guard) = events.lock() {
                guard.push(progress);
            }
        };
        let refreshed: Library = refresh_library(&root_str, &report).expect("refresh");
        assert_eq!(
            events
                .into_inner()
                .expect("events")
                .last()
                .map(|event| event.total),
            Some(0)
        );
        assert_eq!(
            item_of(&refreshed, "toys/sphere.stl").preview_rendered_ms,
            ball_rendered_ms
        );
        assert!(item_of(&refreshed, "toys/box.stl").custom_preview);
        let _ = std::fs::remove_dir_all(root);
        let _ = std::fs::remove_dir_all(images);
    }

    #[test]
    fn rename_fixes_sibling_stem_clash_both_ways() {
        let root: PathBuf = unique_temp_dir("rename-clash");
        let images: PathBuf = unique_temp_dir("rename-clash-images");
        let img: PathBuf = root.join("img");
        write_file(&root, "cube.stl", &binary_stl(&cube_triangles(2.0)));
        minimal_3mf(&root, "cube.3mf", true, false);
        let root_str: String = root_string(&root);
        quiet_refresh(&root);
        let png: PathBuf = solid_png(&images, "cover.png", 32, 32);
        set_custom_preview(&root_str, "cube.stl", false, Some(&png.to_string_lossy()))
            .expect("custom");
        assert!(img.join("cube.stl.png").is_file());
        assert!(img.join("cube.stl.custom.png").is_file());
        assert!(img.join("cube.3mf.png").is_file());

        let split: Library = rename_model(&root_str, "cube.3mf", "ball").expect("rename");
        assert!(img.join("ball.png").is_file());
        assert!(img.join("cube.png").is_file());
        assert!(img.join("cube.custom.png").is_file());
        assert!(!img.join("cube.stl.png").exists());
        assert!(!img.join("cube.stl.custom.png").exists());
        assert!(!img.join("cube.3mf.png").exists());
        assert!(item_of(&split, "cube.stl").custom_preview);
        assert!(!item_of(&split, "ball.3mf").custom_preview);
        let cache_split: CacheFile = cache::load_cache(&root).expect("cache");
        assert_eq!(
            cache_split
                .entries
                .get("cube.stl")
                .and_then(|e| e.preview_rel.clone()),
            Some("img/cube.png".to_string())
        );

        let joined: Library = rename_model(&root_str, "ball.3mf", "cube.3mf").expect("rename back");
        assert!(img.join("cube.stl.png").is_file());
        assert!(img.join("cube.stl.custom.png").is_file());
        assert!(img.join("cube.3mf.png").is_file());
        assert!(!img.join("cube.png").exists());
        assert!(!img.join("ball.png").exists());
        assert!(item_of(&joined, "cube.stl").custom_preview);
        assert!(
            item_of(&joined, "cube.3mf")
                .preview_path
                .as_deref()
                .is_some_and(|path| path.ends_with("img/cube.3mf.png"))
        );
        let cache_joined: CacheFile = cache::load_cache(&root).expect("cache");
        assert_eq!(
            cache_joined
                .entries
                .get("cube.stl")
                .and_then(|e| e.preview_rel.clone()),
            Some("img/cube.stl.png".to_string())
        );
        let events_total: Mutex<Option<u32>> = Mutex::new(None);
        let counting = |progress: ScanProgress| {
            if let Ok(mut guard) = events_total.lock() {
                *guard = Some(progress.total);
            }
        };
        refresh_library(&root_str, &counting).expect("refresh");
        assert_eq!(events_total.into_inner().expect("total"), Some(0));
        let _ = std::fs::remove_dir_all(root);
        let _ = std::fs::remove_dir_all(images);
    }

    #[test]
    fn rename_rejections_leave_everything_untouched() {
        let root: PathBuf = unique_temp_dir("rename-reject");
        write_file(&root, "cube.stl", &binary_stl(&cube_triangles(2.0)));
        write_file(&root, "taken.stl", &binary_stl(&cube_triangles(3.0)));
        let root_str: String = root_string(&root);
        quiet_refresh(&root);
        let cache_before: CacheFile = cache::load_cache(&root).expect("cache");
        for bad in [
            "",
            "sub/cube",
            "..",
            "cube.3mf",
            "taken",
            "taken.stl",
            ".cube",
            "_project",
        ] {
            assert!(
                rename_model(&root_str, "cube.stl", bad).is_err(),
                "accepted {bad:?}"
            );
        }
        assert!(rename_model(&root_str, "missing.stl", "x").is_err());
        assert!(rename_model(&root_str, "../cube.stl", "x").is_err());
        assert!(root.join("cube.stl").is_file());
        assert!(root.join("img").join("cube.png").is_file());
        assert_eq!(cache::load_cache(&root), Ok(cache_before));
        let same: Library = rename_model(&root_str, "cube.stl", "cube").expect("no-op rename");
        assert!(find_item(&same, "cube.stl").is_some());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn rename_drops_orphan_custom_preview_at_destination() {
        let root: PathBuf = unique_temp_dir("rename-orphan");
        write_file(&root, "cube.stl", &binary_stl(&cube_triangles(2.0)));
        let root_str: String = root_string(&root);
        quiet_refresh(&root);
        solid_png(&root.join("img"), "box.custom.png", 8, 8);
        let renamed: Library = rename_model(&root_str, "cube.stl", "box").expect("rename");
        assert!(!root.join("img").join("box.custom.png").exists());
        assert!(!item_of(&renamed, "box.stl").custom_preview);
        let _ = std::fs::remove_dir_all(root);
    }
}
