use std::path::{Path, PathBuf};

use image::imageops::FilterType;
use image::{DynamicImage, RgbaImage};

use crate::cache::{self, CacheFile};
use crate::fsutil::{remove_if_exists, write_atomic};
use crate::imaging::{self, Cell, PREVIEW_SIZE};
use crate::library::{self, CompositeScope, Library};
use crate::project;
use crate::scan::{self, custom_rel_for, is_model_file, last_segment, parent_rel};

const CUSTOM_FILTER: FilterType = FilterType::Lanczos3;

pub fn convert_to_preview(source: &Path) -> Result<Vec<u8>, String> {
    let decoded: DynamicImage = imaging::decode_file(source)?;
    let mut canvas: RgbaImage = imaging::blank_canvas();
    let full: Cell = Cell {
        x: 0,
        y: 0,
        width: PREVIEW_SIZE,
        height: PREVIEW_SIZE,
    };
    imaging::place_fitted(&mut canvas, &decoded, &full, CUSTOM_FILTER);
    imaging::encode_png(&canvas)
}

fn apply_image(target: &Path, image_path: Option<&str>) -> Result<(), String> {
    match image_path {
        Some(source) => write_atomic(target, &convert_to_preview(Path::new(source))?),
        None => remove_if_exists(target),
    }
}

pub fn model_custom_rel(root: &Path, rel: &str) -> Result<String, String> {
    let model: PathBuf = scan::checked_absolute(root, rel)?;
    if rel.is_empty() || !is_model_file(&model) {
        return Err(format!("Model not found: {rel}"));
    }
    scan::folder_preview_rels(root, parent_rel(rel))
        .get(last_segment(rel))
        .map(|rendered_rel| custom_rel_for(rendered_rel))
        .ok_or_else(|| format!("Model not found: {rel}"))
}

fn set_model_custom(root: &Path, rel: &str, image_path: Option<&str>) -> Result<(), String> {
    let custom_rel: String = model_custom_rel(root, rel)?;
    apply_image(&scan::absolute(root, &custom_rel), image_path)?;
    let mut cache: CacheFile = cache::load_cache(root)?;
    let recorded: Option<String> = image_path.map(|_| custom_rel);
    let changed: bool = match cache.entries.get_mut(rel) {
        Some(entry) if entry.custom_preview_rel != recorded => {
            entry.custom_preview_rel = recorded;
            true
        }
        _ => false,
    };
    if changed {
        cache::save_cache(root, &cache)?;
    }
    Ok(())
}

fn set_project_custom(
    root: &Path,
    folder_rel: &str,
    image_path: Option<&str>,
) -> Result<(), String> {
    let folder: PathBuf = project::project_folder(root, folder_rel)?;
    if !project::is_project(&folder) {
        return Err(format!("{folder_rel} is not a project"));
    }
    apply_image(&project::custom_path(&folder), image_path)
}

pub fn set_custom_preview(
    root: &str,
    rel: &str,
    is_project: bool,
    image_path: Option<&str>,
) -> Result<Library, String> {
    let root_path: PathBuf = library::validate_root(root)?;
    let folder_rel: String = if is_project {
        set_project_custom(&root_path, rel, image_path)?;
        rel.to_string()
    } else {
        set_model_custom(&root_path, rel, image_path)?;
        parent_rel(rel).to_string()
    };
    library::load_with_composites(root, &root_path, CompositeScope::Around(vec![folder_rel]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{
        binary_stl, cube_triangles, decode_png, find_item, find_node, quiet_refresh, root_string,
        solid_jpeg, solid_png, unique_temp_dir, write_file,
    };

    fn alpha_at(rgba: &[u8], x: usize, y: usize) -> u8 {
        rgba[(y * PREVIEW_SIZE as usize + x) * 4 + 3]
    }

    fn converted(source: &Path) -> Vec<u8> {
        let bytes: Vec<u8> = convert_to_preview(source).expect("converts");
        let (width, height, rgba) = decode_png(&bytes);
        assert_eq!((width, height), (512, 512));
        rgba
    }

    #[test]
    fn jpeg_is_fitted_with_transparent_letterbox() {
        let dir: PathBuf = unique_temp_dir("custom-jpeg");
        let rgba: Vec<u8> = converted(&solid_jpeg(&dir, "photo.jpg", 300, 200));
        assert_eq!(alpha_at(&rgba, 256, 256), 255);
        assert_eq!(alpha_at(&rgba, 5, 256), 255);
        assert_eq!(alpha_at(&rgba, 256, 10), 0);
        assert_eq!(alpha_at(&rgba, 256, 501), 0);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn tall_png_is_fitted_with_transparent_pillarbox() {
        let dir: PathBuf = unique_temp_dir("custom-png");
        let rgba: Vec<u8> = converted(&solid_png(&dir, "tall.png", 100, 400));
        assert_eq!(alpha_at(&rgba, 256, 256), 255);
        assert_eq!(alpha_at(&rgba, 256, 3), 255);
        assert_eq!(alpha_at(&rgba, 10, 256), 0);
        assert_eq!(alpha_at(&rgba, 500, 256), 0);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn unreadable_image_is_an_error() {
        let dir: PathBuf = unique_temp_dir("custom-bad");
        let bogus: PathBuf = write_file(&dir, "bogus.png", b"not an image");
        assert!(convert_to_preview(&bogus).is_err());
        assert!(convert_to_preview(&dir.join("missing.jpg")).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn model_custom_preview_is_set_and_reset() {
        let root: PathBuf = unique_temp_dir("custom-model");
        let images: PathBuf = unique_temp_dir("custom-model-images");
        write_file(
            &root.join("toys"),
            "cube.stl",
            &binary_stl(&cube_triangles(2.0)),
        );
        quiet_refresh(&root);
        let jpeg: PathBuf = solid_jpeg(&images, "photo.jpeg", 64, 32);
        let custom_file: PathBuf = root.join("toys").join("img").join("cube.custom.png");

        let library: Library = set_custom_preview(
            &root_string(&root),
            "toys/cube.stl",
            false,
            Some(&jpeg.to_string_lossy()),
        )
        .expect("custom preview set");
        let item = find_item(&library, "toys/cube.stl").expect("item");
        assert!(item.custom_preview);
        assert_eq!(
            item.preview_path.as_deref(),
            Some(custom_file.to_string_lossy().as_ref())
        );
        assert!(custom_file.is_file());
        let cache: CacheFile = cache::load_cache(&root).expect("cache");
        assert_eq!(
            cache
                .entries
                .get("toys/cube.stl")
                .and_then(|entry| entry.custom_preview_rel.clone()),
            Some("toys/img/cube.custom.png".to_string())
        );

        let refreshed: Library = quiet_refresh(&root);
        assert!(custom_file.is_file());
        assert!(find_item(&refreshed, "toys/cube.stl").is_some_and(|found| found.custom_preview));

        let reset: Library = set_custom_preview(&root_string(&root), "toys/cube.stl", false, None)
            .expect("custom preview reset");
        assert!(!custom_file.exists());
        let item = find_item(&reset, "toys/cube.stl").expect("item");
        assert!(!item.custom_preview);
        assert!(
            item.preview_path
                .as_deref()
                .is_some_and(|path| path.ends_with("toys/img/cube.png"))
        );
        let cache: CacheFile = cache::load_cache(&root).expect("cache");
        assert_eq!(
            cache
                .entries
                .get("toys/cube.stl")
                .map(|entry| entry.custom_preview_rel.clone()),
            Some(None)
        );
        assert!(set_custom_preview(&root_string(&root), "toys/nope.stl", false, None).is_err());
        let _ = std::fs::remove_dir_all(root);
        let _ = std::fs::remove_dir_all(images);
    }

    #[test]
    fn custom_preview_uses_stem_clash_naming() {
        let root: PathBuf = unique_temp_dir("custom-clash");
        let images: PathBuf = unique_temp_dir("custom-clash-images");
        write_file(&root, "cube.stl", &binary_stl(&cube_triangles(2.0)));
        crate::testutil::minimal_3mf(&root, "cube.3mf", true, false);
        quiet_refresh(&root);
        let png: PathBuf = solid_png(&images, "a.png", 10, 10);
        let library: Library = set_custom_preview(
            &root_string(&root),
            "cube.3mf",
            false,
            Some(&png.to_string_lossy()),
        )
        .expect("set");
        assert!(root.join("img").join("cube.3mf.custom.png").is_file());
        assert!(find_item(&library, "cube.3mf").is_some_and(|item| item.custom_preview));
        assert!(find_item(&library, "cube.stl").is_some_and(|item| !item.custom_preview));
        let _ = std::fs::remove_dir_all(root);
        let _ = std::fs::remove_dir_all(images);
    }

    #[test]
    fn project_custom_preview_overrides_composite() {
        let root: PathBuf = unique_temp_dir("custom-project");
        let images: PathBuf = unique_temp_dir("custom-project-images");
        write_file(
            &root.join("kit"),
            "a.stl",
            &binary_stl(&cube_triangles(2.0)),
        );
        quiet_refresh(&root);
        let png: PathBuf = solid_png(&images, "cover.png", 200, 100);
        let root_str: String = root_string(&root);
        assert!(set_custom_preview(&root_str, "kit", true, Some(&png.to_string_lossy())).is_err());
        project::set_project(&root_str, "kit", true).expect("mark");
        let library: Library =
            set_custom_preview(&root_str, "kit", true, Some(&png.to_string_lossy())).expect("set");
        let node = find_node(&library.tree, "kit").expect("node");
        assert!(node.custom_preview);
        assert!(
            node.preview_path
                .as_deref()
                .is_some_and(|path| path.ends_with("kit/img/_project.custom.png"))
        );
        assert!(node.preview_rendered_ms.is_some());
        let reset: Library = set_custom_preview(&root_str, "kit", true, None).expect("reset");
        let node = find_node(&reset.tree, "kit").expect("node");
        assert!(!node.custom_preview);
        assert!(
            node.preview_path
                .as_deref()
                .is_some_and(|path| path.ends_with("kit/img/_project.png"))
        );
        assert!(
            !root
                .join("kit")
                .join("img")
                .join("_project.custom.png")
                .exists()
        );
        let _ = std::fs::remove_dir_all(root);
        let _ = std::fs::remove_dir_all(images);
    }
}
