use std::io::Cursor;
use std::path::Path;

use image::imageops::{self, FilterType};
use image::{DynamicImage, ImageFormat, ImageReader, RgbaImage};

pub const PREVIEW_SIZE: u32 = 512;

pub struct Cell {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

pub fn decode_file(path: &Path) -> Result<DynamicImage, String> {
    ImageReader::open(path)
        .map_err(|e| format!("Cannot open {}: {e}", path.display()))?
        .with_guessed_format()
        .map_err(|e| format!("Cannot read {}: {e}", path.display()))?
        .decode()
        .map_err(|e| format!("Cannot decode {}: {e}", path.display()))
}

pub fn blank_canvas() -> RgbaImage {
    RgbaImage::new(PREVIEW_SIZE, PREVIEW_SIZE)
}

pub fn place_fitted(
    canvas: &mut RgbaImage,
    source: &DynamicImage,
    cell: &Cell,
    filter: FilterType,
) {
    if cell.width == 0 || cell.height == 0 || source.width() == 0 || source.height() == 0 {
        return;
    }
    let fitted: RgbaImage = source.resize(cell.width, cell.height, filter).to_rgba8();
    let offset_x: u32 = cell.x + cell.width.saturating_sub(fitted.width()) / 2;
    let offset_y: u32 = cell.y + cell.height.saturating_sub(fitted.height()) / 2;
    imageops::replace(canvas, &fitted, i64::from(offset_x), i64::from(offset_y));
}

pub fn encode_png(image: &RgbaImage) -> Result<Vec<u8>, String> {
    let mut cursor: Cursor<Vec<u8>> = Cursor::new(Vec::new());
    image
        .write_to(&mut cursor, ImageFormat::Png)
        .map_err(|e| format!("Cannot encode PNG: {e}"))?;
    Ok(cursor.into_inner())
}
