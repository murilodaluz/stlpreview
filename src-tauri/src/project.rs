use std::path::{Path, PathBuf};

use image::imageops::FilterType;
use image::{DynamicImage, RgbaImage};
use serde::Serialize;

use crate::fsutil::{modified_ms, remove_if_exists, write_atomic};
use crate::imaging::{self, Cell, PREVIEW_SIZE};
use crate::library::{self, CompositeScope, Library};
use crate::scan::{self, PREVIEW_DIR_NAME};

pub const PROJECT_MARKER_NAME: &str = ".sltproject.json";
const PROJECT_MARKER_VERSION: u32 = 1;
const COMPOSITE_FILE_NAME: &str = "_project.png";
const CUSTOM_FILE_NAME: &str = "_project.custom.png";
const GUTTER: u32 = 6;
const MAX_TILES: usize = 9;
const TILE_FILTER: FilterType = FilterType::Triangle;

#[derive(Serialize)]
struct ProjectMarker {
    version: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Grid {
    pub columns: u32,
    pub rows: u32,
}

pub struct ProjectPreview {
    pub path: Option<PathBuf>,
    pub modified_ms: Option<u64>,
    pub custom: bool,
}

pub fn marker_path(folder: &Path) -> PathBuf {
    folder.join(PROJECT_MARKER_NAME)
}

pub fn composite_path(folder: &Path) -> PathBuf {
    folder.join(PREVIEW_DIR_NAME).join(COMPOSITE_FILE_NAME)
}

pub fn custom_path(folder: &Path) -> PathBuf {
    folder.join(PREVIEW_DIR_NAME).join(CUSTOM_FILE_NAME)
}

pub fn is_project(folder: &Path) -> bool {
    marker_path(folder).is_file()
}

pub fn project_preview(folder: &Path) -> ProjectPreview {
    let custom: PathBuf = custom_path(folder);
    if let Some(modified) = modified_ms(&custom) {
        return ProjectPreview {
            path: Some(custom),
            modified_ms: Some(modified),
            custom: true,
        };
    }
    let composite: PathBuf = composite_path(folder);
    match modified_ms(&composite) {
        Some(modified) => ProjectPreview {
            path: Some(composite),
            modified_ms: Some(modified),
            custom: false,
        },
        None => ProjectPreview {
            path: None,
            modified_ms: None,
            custom: false,
        },
    }
}

pub fn grid_for(count: usize) -> Option<Grid> {
    match count {
        0 => None,
        1 => Some(Grid {
            columns: 1,
            rows: 1,
        }),
        2 => Some(Grid {
            columns: 2,
            rows: 1,
        }),
        3..=4 => Some(Grid {
            columns: 2,
            rows: 2,
        }),
        _ => Some(Grid {
            columns: 3,
            rows: 3,
        }),
    }
}

fn tile_cells(grid: Grid) -> Vec<Cell> {
    let tile: u32 = PREVIEW_SIZE.saturating_sub(GUTTER * (grid.columns + 1)) / grid.columns;
    let span = |count: u32| -> u32 { count * tile + (count + 1) * GUTTER };
    let offset_x: u32 = PREVIEW_SIZE.saturating_sub(span(grid.columns)) / 2;
    let offset_y: u32 = PREVIEW_SIZE.saturating_sub(span(grid.rows)) / 2;
    (0..grid.rows)
        .flat_map(|row| (0..grid.columns).map(move |column| (row, column)))
        .map(|(row, column)| Cell {
            x: offset_x + GUTTER + column * (tile + GUTTER),
            y: offset_y + GUTTER + row * (tile + GUTTER),
            width: tile,
            height: tile,
        })
        .collect()
}

pub fn compose(tiles: &[DynamicImage]) -> Option<RgbaImage> {
    let used: &[DynamicImage] = &tiles[..tiles.len().min(MAX_TILES)];
    let grid: Grid = grid_for(used.len())?;
    let mut canvas: RgbaImage = imaging::blank_canvas();
    for (tile, cell) in used.iter().zip(tile_cells(grid)) {
        imaging::place_fitted(&mut canvas, tile, &cell, TILE_FILTER);
    }
    Some(canvas)
}

pub fn write_composite(folder: &Path, previews: &[PathBuf]) -> Result<(), String> {
    let tiles: Vec<DynamicImage> = previews
        .iter()
        .filter_map(|preview| imaging::decode_file(preview).ok())
        .take(MAX_TILES)
        .collect();
    let target: PathBuf = composite_path(folder);
    match compose(&tiles) {
        Some(canvas) => write_atomic(&target, &imaging::encode_png(&canvas)?),
        None => remove_if_exists(&target),
    }
}

pub fn project_folder(root: &Path, folder_rel: &str) -> Result<PathBuf, String> {
    if folder_rel.is_empty() {
        return Err("The root folder cannot be a project".to_string());
    }
    let excluded: bool = folder_rel
        .split('/')
        .any(|segment| segment.starts_with('.') || segment == PREVIEW_DIR_NAME);
    let folder: PathBuf = scan::checked_absolute(root, folder_rel)?;
    if excluded || !folder.is_dir() {
        return Err(format!("Folder not found: {folder_rel}"));
    }
    Ok(folder)
}

fn write_marker(folder: &Path) -> Result<(), String> {
    let marker: ProjectMarker = ProjectMarker {
        version: PROJECT_MARKER_VERSION,
    };
    let bytes: Vec<u8> =
        serde_json::to_vec(&marker).map_err(|e| format!("Cannot serialize project marker: {e}"))?;
    write_atomic(&marker_path(folder), &bytes)
}

fn remove_project_files(folder: &Path) -> Result<(), String> {
    remove_if_exists(&marker_path(folder))?;
    remove_if_exists(&composite_path(folder))?;
    remove_if_exists(&custom_path(folder))
}

pub fn set_project(root: &str, folder_rel: &str, is_project: bool) -> Result<Library, String> {
    let root_path: PathBuf = library::validate_root(root)?;
    let folder: PathBuf = project_folder(&root_path, folder_rel)?;
    if is_project {
        write_marker(&folder)?;
    } else {
        remove_project_files(&folder)?;
    }
    library::load_with_composites(
        root,
        &root_path,
        CompositeScope::Around(vec![folder_rel.to_string()]),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{
        binary_stl, cube_triangles, decode_png, find_node, quiet_refresh, root_string, solid_jpeg,
        solid_png, unique_temp_dir, write_file,
    };

    fn alpha_at(rgba: &[u8], x: usize, y: usize) -> u8 {
        rgba[(y * PREVIEW_SIZE as usize + x) * 4 + 3]
    }

    fn composite_of(count: usize) -> Vec<u8> {
        let dir: PathBuf = unique_temp_dir("composite");
        let previews: Vec<PathBuf> = (0..count)
            .map(|index| solid_png(&dir.join("img"), &format!("part{index}.png"), 512, 512))
            .collect();
        assert_eq!(write_composite(&dir, &previews), Ok(()));
        let bytes: Vec<u8> = std::fs::read(composite_path(&dir)).unwrap_or_default();
        let (width, height, rgba) = decode_png(&bytes);
        assert_eq!((width, height), (512, 512));
        let _ = std::fs::remove_dir_all(dir);
        rgba
    }

    #[test]
    fn grid_layout_by_part_count() {
        let shape = |count: usize| grid_for(count).map(|grid| (grid.columns, grid.rows));
        assert_eq!(shape(0), None);
        assert_eq!(shape(1), Some((1, 1)));
        assert_eq!(shape(2), Some((2, 1)));
        assert_eq!(shape(3), Some((2, 2)));
        assert_eq!(shape(4), Some((2, 2)));
        assert_eq!(shape(5), Some((3, 3)));
        assert_eq!(shape(9), Some((3, 3)));
        assert_eq!(shape(10), Some((3, 3)));
    }

    #[test]
    fn single_part_fills_the_canvas_inside_the_gutter() {
        let rgba: Vec<u8> = composite_of(1);
        assert_eq!(alpha_at(&rgba, 256, 256), 255);
        assert_eq!(alpha_at(&rgba, 10, 10), 255);
        assert_eq!(alpha_at(&rgba, 2, 2), 0);
        assert_eq!(alpha_at(&rgba, 509, 509), 0);
    }

    #[test]
    fn two_parts_sit_side_by_side_vertically_centred() {
        let rgba: Vec<u8> = composite_of(2);
        assert_eq!(alpha_at(&rgba, 128, 256), 255);
        assert_eq!(alpha_at(&rgba, 384, 256), 255);
        assert_eq!(alpha_at(&rgba, 256, 256), 0);
        assert_eq!(alpha_at(&rgba, 128, 40), 0);
        assert_eq!(alpha_at(&rgba, 384, 470), 0);
    }

    #[test]
    fn four_parts_fill_a_two_by_two_grid() {
        let rgba: Vec<u8> = composite_of(4);
        for (x, y) in [(128, 128), (384, 128), (128, 384), (384, 384)] {
            assert_eq!(alpha_at(&rgba, x, y), 255, "tile at {x},{y}");
        }
        assert_eq!(alpha_at(&rgba, 256, 128), 0);
        assert_eq!(alpha_at(&rgba, 128, 256), 0);
    }

    #[test]
    fn ten_parts_use_the_first_nine_in_a_three_by_three_grid() {
        let rgba: Vec<u8> = composite_of(10);
        for y in [85, 256, 426] {
            for x in [85, 256, 426] {
                assert_eq!(alpha_at(&rgba, x, y), 255, "tile at {x},{y}");
            }
        }
        assert_eq!(alpha_at(&rgba, 171, 85), 0);
        assert_eq!(alpha_at(&rgba, 85, 171), 0);
    }

    #[test]
    fn three_parts_leave_the_fourth_cell_empty() {
        let rgba: Vec<u8> = composite_of(3);
        assert_eq!(alpha_at(&rgba, 128, 384), 255);
        assert_eq!(alpha_at(&rgba, 384, 384), 0);
    }

    #[test]
    fn unusable_previews_are_skipped_and_none_left_removes_composite() {
        let dir: PathBuf = unique_temp_dir("composite-empty");
        let broken: PathBuf = crate::testutil::write_file(&dir.join("img"), "broken.png", b"nope");
        let missing: PathBuf = dir.join("img").join("missing.png");
        assert_eq!(
            write_composite(&dir, &[solid_png(&dir.join("img"), "ok.png", 512, 512)]),
            Ok(())
        );
        assert!(composite_path(&dir).is_file());
        assert_eq!(write_composite(&dir, &[broken, missing]), Ok(()));
        assert!(!composite_path(&dir).exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    fn kit_root(label: &str) -> PathBuf {
        let root: PathBuf = unique_temp_dir(label);
        let kit: PathBuf = root.join("kit");
        write_file(&kit, "a.stl", &binary_stl(&cube_triangles(2.0)));
        write_file(&kit, "b.stl", &binary_stl(&cube_triangles(3.0)));
        write_file(&kit.join("sub"), "c.stl", &binary_stl(&cube_triangles(4.0)));
        write_file(&root, "loose.stl", &binary_stl(&cube_triangles(1.0)));
        root
    }

    #[test]
    fn marking_and_unmarking_a_project_manages_marker_and_composite() {
        let root: PathBuf = kit_root("project-mark");
        let kit: PathBuf = root.join("kit");
        let root_str: String = root_string(&root);
        quiet_refresh(&root);

        let marked: Library = set_project(&root_str, "kit", true).expect("mark");
        assert_eq!(
            std::fs::read_to_string(marker_path(&kit)).unwrap_or_default(),
            r#"{"version":1}"#
        );
        assert!(composite_path(&kit).is_file());
        let node = find_node(&marked.tree, "kit").expect("kit node");
        assert!(node.is_project);
        assert!(!node.custom_preview);
        assert_eq!(
            node.preview_path.as_deref(),
            Some(composite_path(&kit).to_string_lossy().as_ref())
        );
        assert!(node.preview_rendered_ms.is_some());
        assert_eq!(node.total_count, 3);
        let sub = find_node(&marked.tree, "kit/sub").expect("sub node");
        assert!(!sub.is_project);
        assert_eq!(sub.preview_path, None);
        assert!(!marked.tree.is_project);
        let bytes: Vec<u8> = std::fs::read(composite_path(&kit)).unwrap_or_default();
        let (width, height, rgba) = decode_png(&bytes);
        assert_eq!((width, height), (512, 512));
        assert!(rgba.chunks_exact(4).any(|pixel| pixel[3] > 0));
        assert_eq!(marked.items.len(), 4);

        std::fs::write(custom_path(&kit), b"placeholder").expect("write custom");
        let unmarked: Library = set_project(&root_str, "kit", false).expect("unmark");
        assert!(!marker_path(&kit).exists());
        assert!(!composite_path(&kit).exists());
        assert!(!custom_path(&kit).exists());
        let node = find_node(&unmarked.tree, "kit").expect("kit node");
        assert!(!node.is_project);
        assert_eq!(node.preview_path, None);
        assert_eq!(node.preview_rendered_ms, None);
        assert!(kit.join("img").join("a.png").is_file());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn root_and_missing_folders_cannot_be_projects() {
        let root: PathBuf = kit_root("project-root");
        let root_str: String = root_string(&root);
        assert!(set_project(&root_str, "", true).is_err());
        assert!(set_project(&root_str, "nope", true).is_err());
        assert!(set_project(&root_str, "../outside", true).is_err());
        assert!(set_project(&root_str, "kit/img", true).is_err());
        assert!(!marker_path(&root).exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn refresh_rebuilds_composites_and_ignores_reserved_files() {
        let root: PathBuf = kit_root("project-refresh");
        let kit: PathBuf = root.join("kit");
        write_file(
            &kit,
            "_project_extra.stl",
            &binary_stl(&cube_triangles(1.0)),
        );
        write_file(&kit.join("img"), "_project.png", b"stale");
        write_file(&kit, PROJECT_MARKER_NAME, br#"{"version":1}"#);
        let library: Library = quiet_refresh(&root);
        let names: Vec<&str> = library
            .items
            .iter()
            .map(|item| item.name.as_str())
            .collect();
        assert_eq!(names, vec!["a.stl", "b.stl", "c.stl", "loose.stl"]);
        let bytes: Vec<u8> = std::fs::read(composite_path(&kit)).unwrap_or_default();
        let (width, height, _) = decode_png(&bytes);
        assert_eq!((width, height), (512, 512));
        assert!(find_node(&library.tree, "kit").is_some_and(|node| node.is_project));

        let _ = std::fs::remove_file(composite_path(&kit));
        quiet_refresh(&root);
        assert!(composite_path(&kit).is_file());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn composite_prefers_custom_part_previews() {
        let root: PathBuf = unique_temp_dir("project-custom-part");
        let images: PathBuf = unique_temp_dir("project-custom-part-images");
        let kit: PathBuf = root.join("kit");
        write_file(&kit, "a.stl", &binary_stl(&cube_triangles(2.0)));
        let root_str: String = root_string(&root);
        quiet_refresh(&root);
        set_project(&root_str, "kit", true).expect("mark");
        let center_of_composite = || -> [u8; 4] {
            let bytes: Vec<u8> = std::fs::read(composite_path(&kit)).unwrap_or_default();
            let (_, _, rgba) = decode_png(&bytes);
            let index: usize = (256 * 512 + 256) * 4;
            [
                rgba[index],
                rgba[index + 1],
                rgba[index + 2],
                rgba[index + 3],
            ]
        };
        let rendered: [u8; 4] = center_of_composite();
        assert!(rendered[0] > rendered[2]);
        let jpeg: PathBuf = solid_jpeg(&images, "blue.jpg", 64, 64);
        crate::custom_preview::set_custom_preview(
            &root_str,
            "kit/a.stl",
            false,
            Some(&jpeg.to_string_lossy()),
        )
        .expect("custom");
        let custom: [u8; 4] = center_of_composite();
        assert!(custom[2] > custom[0], "expected blue, got {custom:?}");
        assert_eq!(custom[3], 255);
        let _ = std::fs::remove_dir_all(root);
        let _ = std::fs::remove_dir_all(images);
    }
}
