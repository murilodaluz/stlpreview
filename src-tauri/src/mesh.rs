use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, ErrorKind, Read};
use std::path::Path;

use quick_xml::XmlVersion;
use quick_xml::events::{BytesStart, Event};
use quick_xml::reader::Reader;
use zip::ZipArchive;

use crate::library::ModelKind;

pub type Vec3 = [f32; 3];
pub type Triangle = [Vec3; 3];

pub struct Mesh {
    pub triangles: Vec<Triangle>,
}

pub fn model_kind(path: &Path) -> Option<ModelKind> {
    let extension: String = path.extension()?.to_string_lossy().to_ascii_lowercase();
    match extension.as_str() {
        "stl" => Some(ModelKind::Stl),
        "3mf" => Some(ModelKind::ThreeMf),
        _ => None,
    }
}

pub fn load_mesh(path: &Path) -> Result<Mesh, String> {
    match model_kind(path) {
        Some(ModelKind::Stl) => load_stl(path),
        Some(ModelKind::ThreeMf) => load_3mf(path),
        None => Err(format!("Unsupported file type: {}", path.display())),
    }
}

const STL_HEADER_LEN: usize = 80;
const STL_PREAMBLE_LEN: usize = 84;
const STL_RECORD_LEN: usize = 50;
const STL_VERTEX_OFFSET: usize = 12;
const ASCII_STL_MAGIC: &[u8] = b"solid";

fn open_file(path: &Path) -> Result<File, String> {
    File::open(path).map_err(|e| format!("Cannot open {}: {e}", path.display()))
}

fn read_up_to(reader: &mut impl Read, buffer: &mut [u8]) -> Result<usize, String> {
    let mut filled: usize = 0;
    while filled < buffer.len() {
        match reader.read(&mut buffer[filled..]) {
            Ok(0) => break,
            Ok(read) => filled += read,
            Err(error) if error.kind() == ErrorKind::Interrupted => {}
            Err(error) => return Err(format!("Cannot read STL: {error}")),
        }
    }
    Ok(filled)
}

fn load_stl(path: &Path) -> Result<Mesh, String> {
    let file: File = open_file(path)?;
    let file_len: u64 = file
        .metadata()
        .map_err(|e| format!("Cannot stat {}: {e}", path.display()))?
        .len();
    let mut reader: BufReader<File> = BufReader::new(file);
    let mut preamble: [u8; STL_PREAMBLE_LEN] = [0; STL_PREAMBLE_LEN];
    let preamble_len: usize = read_up_to(&mut reader, &mut preamble)?;
    let has_full_preamble: bool = preamble_len == STL_PREAMBLE_LEN;
    let declared_count: u64 = if has_full_preamble {
        u64::from(u32::from_le_bytes([
            preamble[STL_HEADER_LEN],
            preamble[STL_HEADER_LEN + 1],
            preamble[STL_HEADER_LEN + 2],
            preamble[STL_HEADER_LEN + 3],
        ]))
    } else {
        0
    };
    let available_count: u64 =
        file_len.saturating_sub(STL_PREAMBLE_LEN as u64) / STL_RECORD_LEN as u64;
    let exact_binary_len: u64 = STL_PREAMBLE_LEN as u64 + STL_RECORD_LEN as u64 * declared_count;
    if has_full_preamble && file_len == exact_binary_len {
        return read_binary_stl(&mut reader, declared_count);
    }
    if preamble[..preamble_len]
        .trim_ascii_start()
        .starts_with(ASCII_STL_MAGIC)
    {
        let ascii: Mesh = read_ascii_stl(BufReader::new(open_file(path)?))?;
        if !ascii.triangles.is_empty() || !has_full_preamble {
            return Ok(ascii);
        }
        return read_binary_stl(&mut reader, available_count);
    }
    if !has_full_preamble {
        return Err(format!("{} is too small to be an STL file", path.display()));
    }
    if declared_count > available_count {
        return Err(format!(
            "Corrupt STL: header declares {declared_count} triangles but the file holds at most {available_count}"
        ));
    }
    let count: u64 = if declared_count > 0 {
        declared_count
    } else {
        available_count
    };
    read_binary_stl(&mut reader, count)
}

fn le_f32(bytes: &[u8], offset: usize) -> f32 {
    let mut word: [u8; 4] = [0; 4];
    if let Some(slice) = bytes.get(offset..offset + 4) {
        word.copy_from_slice(slice);
    }
    f32::from_le_bytes(word)
}

fn read_binary_stl(reader: &mut impl Read, count: u64) -> Result<Mesh, String> {
    let capacity: usize = usize::try_from(count).map_err(|_| "STL is too large".to_string())?;
    let mut triangles: Vec<Triangle> = Vec::with_capacity(capacity);
    let mut record: [u8; STL_RECORD_LEN] = [0; STL_RECORD_LEN];
    for _ in 0..count {
        match reader.read_exact(&mut record) {
            Ok(()) => {}
            Err(error) if error.kind() == ErrorKind::UnexpectedEof => break,
            Err(error) => return Err(format!("Cannot read STL: {error}")),
        }
        let vertex = |index: usize| -> Vec3 {
            let base: usize = STL_VERTEX_OFFSET + index * 12;
            [
                le_f32(&record, base),
                le_f32(&record, base + 4),
                le_f32(&record, base + 8),
            ]
        };
        triangles.push([vertex(0), vertex(1), vertex(2)]);
    }
    Ok(Mesh { triangles })
}

struct AsciiStlState {
    coords_needed: usize,
    coords: Vec3,
    vertices: Vec<Vec3>,
    triangles: Vec<Triangle>,
}

impl AsciiStlState {
    fn token(&mut self, token: &[u8]) -> Result<(), String> {
        if token.eq_ignore_ascii_case(b"vertex") {
            self.coords_needed = 3;
            return Ok(());
        }
        if token.eq_ignore_ascii_case(b"facet") || token.eq_ignore_ascii_case(b"endfacet") {
            self.coords_needed = 0;
            self.vertices.clear();
            return Ok(());
        }
        if self.coords_needed == 0 {
            return Ok(());
        }
        let value: f32 = std::str::from_utf8(token)
            .ok()
            .and_then(|text| text.parse::<f32>().ok())
            .ok_or_else(|| {
                format!(
                    "Invalid ASCII STL coordinate: {}",
                    String::from_utf8_lossy(token)
                )
            })?;
        self.coords[3 - self.coords_needed] = value;
        self.coords_needed -= 1;
        if self.coords_needed == 0 {
            self.vertices.push(self.coords);
            if self.vertices.len() == 3 {
                self.triangles
                    .push([self.vertices[0], self.vertices[1], self.vertices[2]]);
                self.vertices.clear();
            }
        }
        Ok(())
    }
}

fn read_ascii_stl(mut reader: impl BufRead) -> Result<Mesh, String> {
    let mut state = AsciiStlState {
        coords_needed: 0,
        coords: [0.0; 3],
        vertices: Vec::with_capacity(3),
        triangles: Vec::new(),
    };
    let mut line: Vec<u8> = Vec::new();
    loop {
        line.clear();
        let read: usize = reader
            .read_until(b'\n', &mut line)
            .map_err(|e| format!("Cannot read STL: {e}"))?;
        if read == 0 {
            break;
        }
        for token in line
            .split(u8::is_ascii_whitespace)
            .filter(|token| !token.is_empty())
        {
            state.token(token)?;
        }
    }
    Ok(Mesh {
        triangles: state.triangles,
    })
}

const RELS_PART: &str = "_rels/.rels";
const MODEL_REL_TYPE_SUFFIX: &str = "3dmodel";
const DEFAULT_MODEL_PART: &str = "3D/3dmodel.model";
const THUMBNAIL_PARTS: [&str; 2] = ["Metadata/thumbnail.png", "Metadata/plate_1.png"];
const SKIPPED_OBJECT_TYPES: [&str; 2] = ["other", "support"];
const MAX_COMPONENT_DEPTH: u32 = 32;

#[derive(Debug, Clone, Copy, PartialEq)]
struct Transform {
    m: [f32; 9],
    t: Vec3,
}

impl Transform {
    const IDENTITY: Transform = Transform {
        m: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
        t: [0.0, 0.0, 0.0],
    };

    fn parse(text: &str) -> Option<Transform> {
        let values: Vec<f32> = text
            .split_whitespace()
            .map(str::parse::<f32>)
            .collect::<Result<Vec<f32>, _>>()
            .ok()?;
        let fixed: [f32; 12] = values.try_into().ok()?;
        Some(Transform {
            m: [
                fixed[0], fixed[1], fixed[2], fixed[3], fixed[4], fixed[5], fixed[6], fixed[7],
                fixed[8],
            ],
            t: [fixed[9], fixed[10], fixed[11]],
        })
    }

    fn rotate(&self, p: Vec3) -> Vec3 {
        let m: &[f32; 9] = &self.m;
        [
            p[0] * m[0] + p[1] * m[3] + p[2] * m[6],
            p[0] * m[1] + p[1] * m[4] + p[2] * m[7],
            p[0] * m[2] + p[1] * m[5] + p[2] * m[8],
        ]
    }

    fn apply(&self, p: Vec3) -> Vec3 {
        let r: Vec3 = self.rotate(p);
        [r[0] + self.t[0], r[1] + self.t[1], r[2] + self.t[2]]
    }

    fn then(&self, outer: &Transform) -> Transform {
        let a: &[f32; 9] = &self.m;
        let b: &[f32; 9] = &outer.m;
        let mut m: [f32; 9] = [0.0; 9];
        for row in 0..3 {
            for col in 0..3 {
                m[row * 3 + col] = (0..3).map(|k| a[row * 3 + k] * b[k * 3 + col]).sum();
            }
        }
        Transform {
            m,
            t: outer.apply(self.t),
        }
    }
}

#[derive(Debug, Clone)]
struct ObjectRef {
    object_id: u32,
    path: Option<String>,
    transform: Transform,
}

#[derive(Default)]
struct Object {
    skipped: bool,
    vertices: Vec<Vec3>,
    triangles: Vec<[u32; 3]>,
    components: Vec<ObjectRef>,
}

#[derive(Default)]
struct ModelPart {
    objects: HashMap<u32, Object>,
    build: Vec<ObjectRef>,
}

impl ModelPart {
    fn referenced_parts(&self) -> impl Iterator<Item = &String> {
        self.objects
            .values()
            .flat_map(|object| object.components.iter())
            .chain(self.build.iter())
            .filter_map(|reference| reference.path.as_ref())
    }
}

type Archive = ZipArchive<File>;

fn open_archive(path: &Path) -> Result<Archive, String> {
    ZipArchive::new(open_file(path)?)
        .map_err(|e| format!("Invalid 3MF archive {}: {e}", path.display()))
}

fn normalize_part_name(name: &str) -> String {
    name.trim().trim_start_matches('/').to_string()
}

fn resolve_entry(archive: &Archive, name: &str) -> Option<String> {
    if archive.index_for_name(name).is_some() {
        return Some(name.to_string());
    }
    archive
        .file_names()
        .filter_map(Result::ok)
        .find(|candidate| candidate.eq_ignore_ascii_case(name))
        .map(|candidate| candidate.into_owned())
}

fn read_entry(archive: &mut Archive, name: &str) -> Result<Option<Vec<u8>>, String> {
    let Some(entry_name) = resolve_entry(archive, name) else {
        return Ok(None);
    };
    let mut entry = archive
        .by_name(&entry_name)
        .map_err(|e| format!("Cannot open {entry_name} in 3MF: {e}"))?;
    let mut bytes: Vec<u8> = Vec::new();
    entry
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Cannot read {entry_name} in 3MF: {e}"))?;
    Ok(Some(bytes))
}

fn has_local_name(element: &BytesStart, expected: &str) -> bool {
    element.local_name().as_ref() == expected
}

fn attribute_values<const N: usize>(element: &BytesStart, keys: [&str; N]) -> [Option<String>; N] {
    let mut values: [Option<String>; N] = std::array::from_fn(|_| None);
    for attribute in element
        .attributes()
        .with_checks(false)
        .filter_map(Result::ok)
    {
        let local_key = attribute.key.local_name();
        if let Some(slot) = keys.iter().position(|key| *key == local_key.as_ref()) {
            values[slot] = attribute
                .normalized_value(XmlVersion::Implicit1_0)
                .ok()
                .map(|value| value.into_owned());
        }
    }
    values
}

fn parse_numbers<T: std::str::FromStr + Copy + Default, const N: usize>(
    element: &BytesStart,
    keys: [&str; N],
) -> Option<[T; N]> {
    let mut parsed: [Option<T>; N] = [None; N];
    for attribute in element
        .attributes()
        .with_checks(false)
        .filter_map(Result::ok)
    {
        let local_key = attribute.key.local_name();
        if let Some(slot) = keys.iter().position(|key| *key == local_key.as_ref()) {
            parsed[slot] = attribute.value.trim().parse::<T>().ok();
        }
    }
    let mut result: [T; N] = [T::default(); N];
    for (slot, value) in result.iter_mut().zip(parsed) {
        *slot = value?;
    }
    Some(result)
}

fn object_ref(element: &BytesStart) -> Option<ObjectRef> {
    let [object_id, path, transform] = attribute_values(element, ["objectid", "path", "transform"]);
    Some(ObjectRef {
        object_id: object_id?.trim().parse::<u32>().ok()?,
        path: path
            .map(|value| normalize_part_name(&value))
            .filter(|value| !value.is_empty()),
        transform: transform
            .and_then(|value| Transform::parse(&value))
            .unwrap_or(Transform::IDENTITY),
    })
}

fn find_root_part(archive: &mut Archive) -> Result<String, String> {
    let Some(rels) = read_entry(archive, RELS_PART)? else {
        return Ok(DEFAULT_MODEL_PART.to_string());
    };
    let mut reader: Reader<&[u8]> = Reader::from_reader(rels.as_slice());
    loop {
        match reader.read_event() {
            Ok(Event::Start(element)) | Ok(Event::Empty(element)) => {
                if has_local_name(&element, "Relationship") {
                    let [kind, target] = attribute_values(&element, ["Type", "Target"]);
                    if let (Some(kind), Some(target)) = (kind, target)
                        && kind.trim_end().ends_with(MODEL_REL_TYPE_SUFFIX)
                    {
                        return Ok(normalize_part_name(&target));
                    }
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            Ok(_) => {}
        }
    }
    Ok(DEFAULT_MODEL_PART.to_string())
}

struct PartParser {
    part: ModelPart,
    current: Option<(u32, Object)>,
    in_build: bool,
}

impl PartParser {
    fn open(&mut self, element: &BytesStart, self_closing: bool) {
        let local_name = element.local_name();
        match local_name.as_ref() {
            "object" => {
                let [id, kind] = attribute_values(element, ["id", "type"]);
                let skipped: bool = kind
                    .map(|value| {
                        SKIPPED_OBJECT_TYPES
                            .iter()
                            .any(|t| value.trim().eq_ignore_ascii_case(t))
                    })
                    .unwrap_or(false);
                let parsed_id: Option<u32> = id.and_then(|value| value.trim().parse::<u32>().ok());
                let object = Object {
                    skipped,
                    ..Object::default()
                };
                match (parsed_id, self_closing) {
                    (Some(object_id), true) => {
                        self.part.objects.insert(object_id, object);
                    }
                    (Some(object_id), false) => self.current = Some((object_id, object)),
                    (None, _) => self.current = None,
                }
            }
            "vertex" => {
                if let Some((_, object)) = self.current.as_mut()
                    && let Some(vertex) = parse_numbers::<f32, 3>(element, ["x", "y", "z"])
                {
                    object.vertices.push(vertex);
                }
            }
            "triangle" => {
                if let Some((_, object)) = self.current.as_mut()
                    && let Some(indices) = parse_numbers::<u32, 3>(element, ["v1", "v2", "v3"])
                {
                    object.triangles.push(indices);
                }
            }
            "component" => {
                if let (Some((_, object)), Some(reference)) =
                    (self.current.as_mut(), object_ref(element))
                {
                    object.components.push(reference);
                }
            }
            "build" => self.in_build = !self_closing,
            "item" => {
                if self.in_build
                    && let Some(reference) = object_ref(element)
                {
                    self.part.build.push(reference);
                }
            }
            _ => {}
        }
    }

    fn close(&mut self, local_name: &str) {
        match local_name {
            "object" => {
                if let Some((object_id, object)) = self.current.take() {
                    self.part.objects.insert(object_id, object);
                }
            }
            "build" => self.in_build = false,
            _ => {}
        }
    }
}

fn parse_model_part(archive: &mut Archive, name: &str) -> Result<Option<ModelPart>, String> {
    let Some(entry_name) = resolve_entry(archive, name) else {
        return Ok(None);
    };
    let entry = archive
        .by_name(&entry_name)
        .map_err(|e| format!("Cannot open {entry_name} in 3MF: {e}"))?;
    let mut reader = Reader::from_reader(BufReader::new(entry));
    let mut buffer: Vec<u8> = Vec::new();
    let mut parser = PartParser {
        part: ModelPart::default(),
        current: None,
        in_build: false,
    };
    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Start(element)) => parser.open(&element, false),
            Ok(Event::Empty(element)) => parser.open(&element, true),
            Ok(Event::End(element)) => parser.close(element.local_name().as_ref()),
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(error) => return Err(format!("Invalid XML in {entry_name}: {error}")),
        }
        buffer.clear();
    }
    Ok(Some(parser.part))
}

fn emit_object(
    parts: &HashMap<String, ModelPart>,
    part_name: &str,
    object_id: u32,
    transform: &Transform,
    depth: u32,
    out: &mut Vec<Triangle>,
) {
    if depth > MAX_COMPONENT_DEPTH {
        return;
    }
    let Some(object) = parts
        .get(part_name)
        .and_then(|part| part.objects.get(&object_id))
    else {
        return;
    };
    if object.skipped {
        return;
    }
    out.reserve(object.triangles.len());
    for indices in &object.triangles {
        let corners: [Option<&Vec3>; 3] = indices.map(|index| object.vertices.get(index as usize));
        if let [Some(a), Some(b), Some(c)] = corners {
            out.push([
                transform.apply(*a),
                transform.apply(*b),
                transform.apply(*c),
            ]);
        }
    }
    for component in &object.components {
        let child_part: &str = component.path.as_deref().unwrap_or(part_name);
        emit_object(
            parts,
            child_part,
            component.object_id,
            &component.transform.then(transform),
            depth + 1,
            out,
        );
    }
}

fn load_3mf(path: &Path) -> Result<Mesh, String> {
    let mut archive: Archive = open_archive(path)?;
    let root_part: String = find_root_part(&mut archive)?;
    let root: ModelPart = parse_model_part(&mut archive, &root_part)?
        .ok_or_else(|| format!("3MF has no model part {root_part}"))?;
    let mut parts: HashMap<String, ModelPart> = HashMap::new();
    let mut pending: Vec<String> = root.referenced_parts().cloned().collect();
    parts.insert(root_part.clone(), root);
    while let Some(name) = pending.pop() {
        if parts.contains_key(&name) {
            continue;
        }
        let part: ModelPart = parse_model_part(&mut archive, &name)?.unwrap_or_default();
        pending.extend(
            part.referenced_parts()
                .filter(|next| !parts.contains_key(*next))
                .cloned(),
        );
        parts.insert(name, part);
    }
    let items: Vec<ObjectRef> = match parts.get(&root_part) {
        Some(root) if !root.build.is_empty() => root.build.clone(),
        Some(root) => {
            let mut ids: Vec<u32> = root.objects.keys().copied().collect();
            ids.sort_unstable();
            ids.into_iter()
                .map(|object_id| ObjectRef {
                    object_id,
                    path: None,
                    transform: Transform::IDENTITY,
                })
                .collect()
        }
        None => Vec::new(),
    };
    let mut triangles: Vec<Triangle> = Vec::new();
    for item in &items {
        let part_name: &str = item.path.as_deref().unwrap_or(&root_part);
        emit_object(
            &parts,
            part_name,
            item.object_id,
            &item.transform,
            0,
            &mut triangles,
        );
    }
    Ok(Mesh { triangles })
}

pub fn read_3mf_thumbnail(path: &Path) -> Result<Option<Vec<u8>>, String> {
    let mut archive: Archive = open_archive(path)?;
    for candidate in THUMBNAIL_PARTS {
        if let Some(bytes) = read_entry(&mut archive, candidate)? {
            return Ok(Some(bytes));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{
        CUBE_TRIANGLES, ascii_stl, binary_stl, cube_triangles, minimal_3mf, unique_temp_dir,
        write_file,
    };
    use std::path::PathBuf;

    #[test]
    fn loads_binary_stl() {
        let dir: PathBuf = unique_temp_dir("mesh-binary");
        let path: PathBuf = write_file(&dir, "cube.stl", &binary_stl(&cube_triangles(10.0)));
        let mesh: Mesh = load_mesh(&path).expect("binary stl loads");
        assert_eq!(mesh.triangles.len(), CUBE_TRIANGLES);
        assert_eq!(mesh.triangles[0], cube_triangles(10.0)[0]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn loads_binary_stl_whose_header_starts_with_solid() {
        let dir: PathBuf = unique_temp_dir("mesh-binary-solid");
        let mut bytes: Vec<u8> = binary_stl(&cube_triangles(1.0));
        bytes[..5].copy_from_slice(b"solid");
        let path: PathBuf = write_file(&dir, "tricky.STL", &bytes);
        let mesh: Mesh = load_mesh(&path).expect("binary stl loads");
        assert_eq!(mesh.triangles.len(), CUBE_TRIANGLES);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn rejects_truncated_binary_stl() {
        let dir: PathBuf = unique_temp_dir("mesh-truncated");
        let bytes: Vec<u8> = (0..=255u8).collect();
        let path: PathBuf = write_file(&dir, "broken.stl", &bytes);
        assert!(load_mesh(&path).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn loads_ascii_stl() {
        let dir: PathBuf = unique_temp_dir("mesh-ascii");
        let path: PathBuf =
            write_file(&dir, "cube.stl", ascii_stl(&cube_triangles(5.0)).as_bytes());
        let mesh: Mesh = load_mesh(&path).expect("ascii stl loads");
        assert_eq!(mesh.triangles.len(), CUBE_TRIANGLES);
        assert_eq!(mesh.triangles[3], cube_triangles(5.0)[3]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn loads_3mf_with_components_paths_and_transforms() {
        let dir: PathBuf = unique_temp_dir("mesh-3mf");
        let path: PathBuf = minimal_3mf(&dir, "assembly.3mf", true, false);
        let mesh: Mesh = load_mesh(&path).expect("3mf loads");
        assert_eq!(mesh.triangles.len(), 4 + CUBE_TRIANGLES);
        let min_x: f32 = mesh
            .triangles
            .iter()
            .flatten()
            .map(|vertex| vertex[0])
            .fold(f32::INFINITY, f32::min);
        let max_z: f32 = mesh
            .triangles
            .iter()
            .flatten()
            .map(|vertex| vertex[2])
            .fold(f32::NEG_INFINITY, f32::max);
        assert!((min_x - 100.0).abs() < 1e-4, "min x was {min_x}");
        assert!((max_z - 25.0).abs() < 1e-4, "max z was {max_z}");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn reads_3mf_thumbnail_when_mesh_is_empty() {
        let dir: PathBuf = unique_temp_dir("mesh-3mf-thumb");
        let path: PathBuf = minimal_3mf(&dir, "empty.3mf", false, true);
        let mesh: Mesh = load_mesh(&path).expect("3mf loads");
        assert!(mesh.triangles.is_empty());
        let thumbnail: Option<Vec<u8>> = read_3mf_thumbnail(&path).expect("thumbnail readable");
        assert!(thumbnail.is_some_and(|bytes| !bytes.is_empty()));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn transform_composition_matches_sequential_application() {
        let inner: Transform = Transform::parse("0 1 0 -1 0 0 0 0 1 1 2 3").expect("inner");
        let outer: Transform = Transform::parse("2 0 0 0 2 0 0 0 2 10 0 0").expect("outer");
        let point: Vec3 = [1.0, 2.0, 3.0];
        let sequential: Vec3 = outer.apply(inner.apply(point));
        let composed: Vec3 = inner.then(&outer).apply(point);
        for axis in 0..3 {
            assert!((sequential[axis] - composed[axis]).abs() < 1e-5);
        }
        assert_eq!(Transform::parse("1 2 3"), None);
    }
}
