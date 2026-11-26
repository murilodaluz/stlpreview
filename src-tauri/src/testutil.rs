use std::fs;
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use zip::ZipWriter;
use zip::write::SimpleFileOptions;

use crate::library::{FolderNode, Library, ModelItem, ScanProgress, refresh_library};
use crate::mesh::Triangle;
use crate::render::{Image, OUTPUT_SIZE, encode_png};

pub const CUBE_TRIANGLES: usize = 12;

static DIR_SERIAL: AtomicU64 = AtomicU64::new(0);

pub fn unique_temp_dir(label: &str) -> PathBuf {
    let nanos: u128 = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or(0);
    let serial: u64 = DIR_SERIAL.fetch_add(1, Ordering::Relaxed);
    let dir: PathBuf = std::env::temp_dir().join(format!(
        "sltpreview-test-{label}-{}-{nanos}-{serial}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

pub fn write_file(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    fs::create_dir_all(dir).expect("create dir");
    let path: PathBuf = dir.join(name);
    fs::write(&path, bytes).expect("write file");
    path
}

pub fn cube_triangles(size: f32) -> Vec<Triangle> {
    let s: f32 = size;
    let corner = |index: usize| -> [f32; 3] {
        [
            if index & 1 == 0 { 0.0 } else { s },
            if index & 2 == 0 { 0.0 } else { s },
            if index & 4 == 0 { 0.0 } else { s },
        ]
    };
    let faces: [[usize; 4]; 6] = [
        [0, 2, 3, 1],
        [4, 5, 7, 6],
        [0, 1, 5, 4],
        [2, 6, 7, 3],
        [0, 4, 6, 2],
        [1, 3, 7, 5],
    ];
    faces
        .iter()
        .flat_map(|[a, b, c, d]| {
            [
                [corner(*a), corner(*b), corner(*c)],
                [corner(*a), corner(*c), corner(*d)],
            ]
        })
        .collect()
}

pub fn binary_stl(triangles: &[Triangle]) -> Vec<u8> {
    let mut bytes: Vec<u8> = vec![0; 80];
    let count: u32 = u32::try_from(triangles.len()).expect("count fits");
    bytes.extend_from_slice(&count.to_le_bytes());
    for triangle in triangles {
        bytes.extend_from_slice(&[0; 12]);
        for value in triangle.iter().flatten() {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes.extend_from_slice(&[0; 2]);
    }
    bytes
}

pub fn ascii_stl(triangles: &[Triangle]) -> String {
    let mut text: String = String::from("solid test\n");
    for triangle in triangles {
        text.push_str("  facet normal 0 0 0\n    outer loop\n");
        for vertex in triangle {
            text.push_str(&format!(
                "      vertex {} {} {}\n",
                vertex[0], vertex[1], vertex[2]
            ));
        }
        text.push_str("    endloop\n  endfacet\n");
    }
    text.push_str("endsolid test\n");
    text
}

pub fn decode_png(bytes: &[u8]) -> (u32, u32, Vec<u8>) {
    let decoder = png::Decoder::new(Cursor::new(bytes));
    let mut reader = decoder.read_info().expect("png header");
    let mut buffer: Vec<u8> = vec![0; reader.output_buffer_size().expect("buffer size")];
    let info = reader.next_frame(&mut buffer).expect("png frame");
    buffer.truncate(info.buffer_size());
    (info.width, info.height, buffer)
}

const RELS_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Target="/3D/3dmodel.model" Id="rel0" Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel"/>
</Relationships>"#;

const TETRA_OBJECT: &str = r#"<object id="1" type="model"><mesh><vertices>
<vertex x="0" y="0" z="0"/><vertex x="1" y="0" z="0"/><vertex x="0" y="1" z="0"/><vertex x="0" y="0" z="1"/>
</vertices><triangles>
<triangle v1="0" v2="2" v3="1"/><triangle v1="0" v2="1" v3="3"/><triangle v1="1" v2="2" v3="3"/><triangle v1="0" v2="3" v3="2"/>
</triangles></mesh></object>"#;

const SUPPORT_OBJECT: &str = r#"<object id="3" type="support"><mesh><vertices>
<vertex x="0" y="0" z="0"/><vertex x="500" y="0" z="0"/><vertex x="0" y="500" z="0"/>
</vertices><triangles><triangle v1="0" v2="1" v3="2"/></triangles></mesh></object>"#;

fn cube_part_xml() -> String {
    let triangles: Vec<Triangle> = cube_triangles(5.0);
    let mut vertices: String = String::new();
    let mut indices: String = String::new();
    for (index, triangle) in triangles.iter().enumerate() {
        for vertex in triangle {
            vertices.push_str(&format!(
                r#"<vertex x="{}" y="{}" z="{}"/>"#,
                vertex[0], vertex[1], vertex[2]
            ));
        }
        let base: usize = index * 3;
        indices.push_str(&format!(
            r#"<triangle v1="{}" v2="{}" v3="{}"/>"#,
            base,
            base + 1,
            base + 2
        ));
    }
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<model unit="millimeter" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02">
<resources><object id="5" type="model"><mesh><vertices>{vertices}</vertices><triangles>{indices}</triangles></mesh></object></resources>
<build/>
</model>"#
    )
}

fn assembly_model_xml() -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<model unit="millimeter" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02" xmlns:p="http://schemas.microsoft.com/3dmanufacturing/production/2015/06">
<resources>
{TETRA_OBJECT}
<object id="2" type="model"><components>
<component objectid="1" transform="1 0 0 0 1 0 0 0 1 0 0 0"/>
<component objectid="5" p:path="/3D/Objects/part.model" transform="1 0 0 0 1 0 0 0 1 0 0 20"/>
</components></object>
{SUPPORT_OBJECT}
</resources>
<build>
<item objectid="2" transform="1 0 0 0 1 0 0 0 1 100 0 0"/>
<item objectid="3"/>
</build>
</model>"#
    )
}

const EMPTY_MODEL_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<model unit="millimeter" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02">
<resources><object id="1" type="model"><mesh><vertices/><triangles/></mesh></object></resources>
<build><item objectid="1"/></build>
</model>"#;

fn thumbnail_png() -> Vec<u8> {
    let image = Image {
        width: OUTPUT_SIZE,
        height: OUTPUT_SIZE,
        rgba: vec![200; OUTPUT_SIZE * OUTPUT_SIZE * 4],
    };
    encode_png(&image).expect("thumbnail encodes")
}

pub fn minimal_3mf(dir: &Path, name: &str, with_geometry: bool, with_thumbnail: bool) -> PathBuf {
    fs::create_dir_all(dir).expect("create dir");
    let path: PathBuf = dir.join(name);
    let file: fs::File = fs::File::create(&path).expect("create 3mf");
    let mut zip: ZipWriter<fs::File> = ZipWriter::new(file);
    let options: SimpleFileOptions = SimpleFileOptions::default();
    let mut add = |entry: &str, bytes: &[u8]| {
        zip.start_file(entry, options).expect("start entry");
        zip.write_all(bytes).expect("write entry");
    };
    add("_rels/.rels", RELS_XML.as_bytes());
    if with_geometry {
        add("3D/3dmodel.model", assembly_model_xml().as_bytes());
        add("3D/Objects/part.model", cube_part_xml().as_bytes());
    } else {
        add("3D/3dmodel.model", EMPTY_MODEL_XML.as_bytes());
    }
    if with_thumbnail {
        add("Metadata/thumbnail.png", &thumbnail_png());
    }
    zip.finish().expect("finish zip");
    path
}

pub fn solid_png(dir: &Path, name: &str, width: u32, height: u32) -> PathBuf {
    let image: image::RgbaImage =
        image::RgbaImage::from_pixel(width, height, image::Rgba([215, 73, 37, 255]));
    let mut cursor: Cursor<Vec<u8>> = Cursor::new(Vec::new());
    image
        .write_to(&mut cursor, image::ImageFormat::Png)
        .expect("png encodes");
    write_file(dir, name, &cursor.into_inner())
}

pub fn solid_jpeg(dir: &Path, name: &str, width: u32, height: u32) -> PathBuf {
    let image: image::RgbImage =
        image::RgbImage::from_pixel(width, height, image::Rgb([30, 120, 200]));
    let mut cursor: Cursor<Vec<u8>> = Cursor::new(Vec::new());
    image
        .write_to(&mut cursor, image::ImageFormat::Jpeg)
        .expect("jpeg encodes");
    write_file(dir, name, &cursor.into_inner())
}

pub fn find_node<'a>(node: &'a FolderNode, rel: &str) -> Option<&'a FolderNode> {
    if node.rel_path == rel {
        return Some(node);
    }
    node.children.iter().find_map(|child| find_node(child, rel))
}

pub fn find_item<'a>(library: &'a Library, rel: &str) -> Option<&'a ModelItem> {
    library.items.iter().find(|item| item.rel_path == rel)
}

pub fn quiet_refresh(root: &Path) -> Library {
    let report = |_: ScanProgress| {};
    refresh_library(&root.to_string_lossy(), &report).expect("refresh succeeds")
}

pub fn root_string(root: &Path) -> String {
    root.to_string_lossy().into_owned()
}
