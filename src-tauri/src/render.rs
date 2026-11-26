use std::path::Path;

use crate::fsutil::write_atomic;
use crate::library::ModelKind;
use crate::mesh::{self, Mesh, Triangle, Vec3};

pub const OUTPUT_SIZE: usize = 512;
const SUPERSAMPLE: usize = 2;
const RENDER_SIZE: usize = OUTPUT_SIZE * SUPERSAMPLE;
const MARGIN: f32 = 0.08;
const AZIMUTH_DEG: f32 = 45.0;
const ELEVATION_DEG: f32 = 30.0;
const DARK: Vec3 = [215.0, 73.0, 37.0];
const LIGHT: Vec3 = [254.0, 189.0, 161.0];
const AMBIENT: f32 = 0.25;
const DIFFUSE: f32 = 0.75;
const LIGHT_TOWARD_CAMERA: f32 = 1.0;
const LIGHT_TOWARD_UP: f32 = 0.6;
const LIGHT_TOWARD_LEFT: f32 = 0.45;
const MIN_PIXEL_AREA: f32 = 1e-9;

pub struct Image {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
}

fn dot(a: Vec3, b: Vec3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn scale(v: Vec3, factor: f32) -> Vec3 {
    [v[0] * factor, v[1] * factor, v[2] * factor]
}

fn add(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn normalize(v: Vec3) -> Option<Vec3> {
    let length: f32 = dot(v, v).sqrt();
    (length.is_finite() && length > 0.0).then(|| scale(v, 1.0 / length))
}

fn is_finite_triangle(triangle: &Triangle) -> bool {
    triangle.iter().flatten().all(|value| value.is_finite())
}

struct Camera {
    right: Vec3,
    up: Vec3,
    back: Vec3,
    light: Vec3,
}

impl Camera {
    fn three_quarter() -> Camera {
        let azimuth: f32 = AZIMUTH_DEG.to_radians();
        let elevation: f32 = ELEVATION_DEG.to_radians();
        let back: Vec3 = [
            elevation.cos() * azimuth.sin(),
            -elevation.cos() * azimuth.cos(),
            elevation.sin(),
        ];
        let world_up: Vec3 = [0.0, 0.0, 1.0];
        let right: Vec3 = normalize(cross(world_up, back)).unwrap_or([1.0, 0.0, 0.0]);
        let up: Vec3 = cross(back, right);
        let light_direction: Vec3 = add(
            add(scale(back, LIGHT_TOWARD_CAMERA), scale(up, LIGHT_TOWARD_UP)),
            scale(right, -LIGHT_TOWARD_LEFT),
        );
        Camera {
            right,
            up,
            back,
            light: normalize(light_direction).unwrap_or(back),
        }
    }

    fn view(&self, p: Vec3) -> Vec3 {
        [dot(p, self.right), dot(p, self.up), dot(p, self.back)]
    }
}

struct Framing {
    center_x: f32,
    center_y: f32,
    pixels_per_unit: f32,
}

impl Framing {
    fn fit(camera: &Camera, triangles: &[Triangle]) -> Result<Framing, String> {
        let mut min: [f32; 2] = [f32::INFINITY; 2];
        let mut max: [f32; 2] = [f32::NEG_INFINITY; 2];
        for triangle in triangles
            .iter()
            .filter(|triangle| is_finite_triangle(triangle))
        {
            for vertex in triangle {
                let projected: Vec3 = camera.view(*vertex);
                for axis in 0..2 {
                    min[axis] = min[axis].min(projected[axis]);
                    max[axis] = max[axis].max(projected[axis]);
                }
            }
        }
        let extent: f32 = (max[0] - min[0]).max(max[1] - min[1]);
        if !extent.is_finite() || extent <= 0.0 {
            return Err("Model has no visible extent".to_string());
        }
        let usable_pixels: f32 = RENDER_SIZE as f32 * (1.0 - 2.0 * MARGIN);
        Ok(Framing {
            center_x: (min[0] + max[0]) * 0.5,
            center_y: (min[1] + max[1]) * 0.5,
            pixels_per_unit: usable_pixels / extent,
        })
    }

    fn to_screen(&self, view: Vec3) -> Vec3 {
        let half: f32 = RENDER_SIZE as f32 * 0.5;
        [
            half + (view[0] - self.center_x) * self.pixels_per_unit,
            half - (view[1] - self.center_y) * self.pixels_per_unit,
            view[2],
        ]
    }
}

struct Raster {
    color: Vec<[u8; 3]>,
    depth: Vec<f32>,
}

fn edge(a: Vec3, b: Vec3, px: f32, py: f32) -> f32 {
    (b[0] - a[0]) * (py - a[1]) - (b[1] - a[1]) * (px - a[0])
}

impl Raster {
    fn new() -> Raster {
        Raster {
            color: vec![[0; 3]; RENDER_SIZE * RENDER_SIZE],
            depth: vec![f32::NEG_INFINITY; RENDER_SIZE * RENDER_SIZE],
        }
    }

    fn fill(&mut self, p0: Vec3, p1: Vec3, p2: Vec3, color: [u8; 3]) {
        if ![p0, p1, p2].iter().flatten().all(|value| value.is_finite()) {
            return;
        }
        let area: f32 = edge(p0, p1, p2[0], p2[1]);
        if area.abs() < MIN_PIXEL_AREA {
            return;
        }
        let last: f32 = (RENDER_SIZE - 1) as f32;
        let x_start: f32 = (p0[0].min(p1[0]).min(p2[0]) - 0.5).ceil().max(0.0);
        let x_end: f32 = (p0[0].max(p1[0]).max(p2[0]) - 0.5).floor().min(last);
        let y_start: f32 = (p0[1].min(p1[1]).min(p2[1]) - 0.5).ceil().max(0.0);
        let y_end: f32 = (p0[1].max(p1[1]).max(p2[1]) - 0.5).floor().min(last);
        if x_start > x_end || y_start > y_end {
            return;
        }
        let inverse_area: f32 = 1.0 / area;
        for y in (y_start as usize)..=(y_end as usize) {
            let py: f32 = y as f32 + 0.5;
            let row: usize = y * RENDER_SIZE;
            for x in (x_start as usize)..=(x_end as usize) {
                let px: f32 = x as f32 + 0.5;
                let w0: f32 = edge(p1, p2, px, py) * inverse_area;
                let w1: f32 = edge(p2, p0, px, py) * inverse_area;
                let w2: f32 = edge(p0, p1, px, py) * inverse_area;
                if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                    continue;
                }
                let z: f32 = w0 * p0[2] + w1 * p1[2] + w2 * p2[2];
                let index: usize = row + x;
                if z > self.depth[index] {
                    self.depth[index] = z;
                    self.color[index] = color;
                }
            }
        }
    }

    fn downsample(&self) -> Image {
        let mut rgba: Vec<u8> = vec![0; OUTPUT_SIZE * OUTPUT_SIZE * 4];
        for out_y in 0..OUTPUT_SIZE {
            for out_x in 0..OUTPUT_SIZE {
                let mut sum: [u32; 3] = [0; 3];
                let mut covered: u32 = 0;
                for sy in 0..SUPERSAMPLE {
                    for sx in 0..SUPERSAMPLE {
                        let index: usize =
                            (out_y * SUPERSAMPLE + sy) * RENDER_SIZE + out_x * SUPERSAMPLE + sx;
                        if self.depth[index] > f32::NEG_INFINITY {
                            covered += 1;
                            for (total, value) in sum.iter_mut().zip(self.color[index]) {
                                *total += u32::from(value);
                            }
                        }
                    }
                }
                if covered == 0 {
                    continue;
                }
                let samples: u32 = (SUPERSAMPLE * SUPERSAMPLE) as u32;
                let base: usize = (out_y * OUTPUT_SIZE + out_x) * 4;
                for channel in 0..3 {
                    rgba[base + channel] = ((sum[channel] + covered / 2) / covered) as u8;
                }
                rgba[base + 3] = ((255 * covered + samples / 2) / samples) as u8;
            }
        }
        Image {
            width: OUTPUT_SIZE,
            height: OUTPUT_SIZE,
            rgba,
        }
    }
}

fn shade(camera: &Camera, triangle: &Triangle) -> Option<[u8; 3]> {
    let normal: Vec3 = normalize(cross(
        sub(triangle[1], triangle[0]),
        sub(triangle[2], triangle[0]),
    ))?;
    let facing: Vec3 = if dot(normal, camera.back) < 0.0 {
        scale(normal, -1.0)
    } else {
        normal
    };
    let intensity: f32 = dot(facing, camera.light).max(0.0);
    let mix: f32 = (AMBIENT + DIFFUSE * intensity).clamp(0.0, 1.0);
    Some(std::array::from_fn(|channel| {
        (DARK[channel] + (LIGHT[channel] - DARK[channel]) * mix).round() as u8
    }))
}

pub fn render_mesh(mesh: &Mesh) -> Result<Image, String> {
    let camera: Camera = Camera::three_quarter();
    let framing: Framing = Framing::fit(&camera, &mesh.triangles)?;
    let mut raster: Raster = Raster::new();
    for triangle in mesh
        .triangles
        .iter()
        .filter(|triangle| is_finite_triangle(triangle))
    {
        let Some(color) = shade(&camera, triangle) else {
            continue;
        };
        let [a, b, c]: [Vec3; 3] = triangle.map(|vertex| framing.to_screen(camera.view(vertex)));
        raster.fill(a, b, c, color);
    }
    Ok(raster.downsample())
}

pub fn encode_png(image: &Image) -> Result<Vec<u8>, String> {
    let width: u32 = u32::try_from(image.width).map_err(|_| "Image too wide".to_string())?;
    let height: u32 = u32::try_from(image.height).map_err(|_| "Image too tall".to_string())?;
    let mut bytes: Vec<u8> = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder
        .write_header()
        .map_err(|e| format!("Cannot encode PNG: {e}"))?;
    writer
        .write_image_data(&image.rgba)
        .map_err(|e| format!("Cannot encode PNG: {e}"))?;
    writer
        .finish()
        .map_err(|e| format!("Cannot encode PNG: {e}"))?;
    Ok(bytes)
}

pub fn render_model(model: &Path, preview: &Path) -> Result<u64, String> {
    let mesh: Mesh = mesh::load_mesh(model)?;
    let triangles: u64 = mesh.triangles.len() as u64;
    if triangles == 0 {
        if mesh::model_kind(model) == Some(ModelKind::ThreeMf)
            && let Some(thumbnail) = mesh::read_3mf_thumbnail(model)?
        {
            write_atomic(preview, &thumbnail)?;
            return Ok(0);
        }
        return Err("Model contains no triangles".to_string());
    }
    let image: Image = render_mesh(&mesh)?;
    drop(mesh);
    let png_bytes: Vec<u8> = encode_png(&image)?;
    write_atomic(preview, &png_bytes)?;
    Ok(triangles)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{
        CUBE_TRIANGLES, binary_stl, cube_triangles, decode_png, minimal_3mf, unique_temp_dir,
        write_file,
    };
    use std::path::PathBuf;

    #[test]
    fn renders_cube_to_transparent_512_png() {
        let mesh = Mesh {
            triangles: cube_triangles(20.0),
        };
        let image: Image = render_mesh(&mesh).expect("renders");
        let png_bytes: Vec<u8> = encode_png(&image).expect("encodes");
        let (width, height, rgba) = decode_png(&png_bytes);
        assert_eq!((width, height), (512, 512));
        let opaque: usize = rgba.chunks_exact(4).filter(|pixel| pixel[3] > 0).count();
        assert!(opaque > 10_000, "only {opaque} visible pixels");
        assert_eq!(rgba[3], 0);
        let last: usize = rgba.len() - 1;
        assert_eq!(rgba[last], 0);
        let center: usize = (256 * 512 + 256) * 4;
        assert_eq!(rgba[center + 3], 255);
    }

    #[test]
    fn render_model_writes_png_into_img_folder() {
        let dir: PathBuf = unique_temp_dir("render-model");
        let model: PathBuf = write_file(&dir, "cube.stl", &binary_stl(&cube_triangles(3.0)));
        let preview: PathBuf = dir.join("img").join("cube.png");
        assert_eq!(render_model(&model, &preview), Ok(CUBE_TRIANGLES as u64));
        let bytes: Vec<u8> = std::fs::read(&preview).expect("preview exists");
        let (width, height, rgba) = decode_png(&bytes);
        assert_eq!((width, height), (512, 512));
        assert!(rgba.chunks_exact(4).any(|pixel| pixel[3] > 0));
        let leftovers: usize = std::fs::read_dir(dir.join("img"))
            .expect("img dir")
            .filter_map(Result::ok)
            .count();
        assert_eq!(leftovers, 1);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn render_model_falls_back_to_3mf_thumbnail() {
        let dir: PathBuf = unique_temp_dir("render-thumb");
        let model: PathBuf = minimal_3mf(&dir, "plate.3mf", false, true);
        let preview: PathBuf = dir.join("img").join("plate.png");
        assert_eq!(render_model(&model, &preview), Ok(0));
        let bytes: Vec<u8> = std::fs::read(&preview).expect("preview exists");
        let (width, _, _) = decode_png(&bytes);
        assert!(width > 0);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn empty_stl_is_an_error() {
        let dir: PathBuf = unique_temp_dir("render-empty");
        let model: PathBuf = write_file(&dir, "empty.stl", b"solid empty\nendsolid empty\n");
        assert!(render_model(&model, &dir.join("img").join("empty.png")).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }
}
