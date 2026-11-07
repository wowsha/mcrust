use std::collections::HashMap;

use anyhow::{Context, Result};
use image::{ImageFormat, Rgba, RgbaImage};

use crate::pack::ResourcePack;

/// Source textures are 16x16; higher resolution packs are downsampled.
const TILE: u32 = 16;
/// Repeated edge pixels. Keeps neighbouring tiles from bleeding at low mip
/// levels and gives future mipmapping some room.
const PAD: u32 = 2;
const CELL: u32 = TILE + PAD * 2;
const COLUMNS: u32 = 16;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UvRect {
    pub u0: f32,
    pub v0: f32,
    pub u1: f32,
    pub v1: f32,
}

impl UvRect {
    pub fn width(self) -> f32 {
        self.u1 - self.u0
    }

    pub fn height(self) -> f32 {
        self.v1 - self.v0
    }

    /// Maps a tile-local (0..1, 0..1) coordinate to atlas space.
    pub fn map(self, u: f32, v: f32) -> [f32; 2] {
        [self.u0 + u * self.width(), self.v0 + v * self.height()]
    }
}

/// A texture atlas built from real vanilla block textures.
#[derive(Debug, Clone)]
pub struct Atlas {
    image: RgbaImage,
    columns: u32,
    rows: u32,
    tiles: HashMap<String, UvRect>,
}

impl Atlas {
    pub fn build<'a>(
        pack: &ResourcePack,
        texture_names: impl IntoIterator<Item = &'a str>,
    ) -> Result<Self> {
        let mut names: Vec<&str> = texture_names
            .into_iter()
            .filter(|name| !name.is_empty())
            .collect();
        names.sort_unstable();
        names.dedup();

        let columns = COLUMNS.min(names.len().max(1) as u32).max(1);
        let rows = (names.len() as u32).div_ceil(columns).max(1);
        let width = columns * CELL;
        let height = rows * CELL;
        let mut image = RgbaImage::from_fn(width, height, |_, _| Rgba([255, 0, 255, 255]));
        let mut tiles = HashMap::with_capacity(names.len());

        for (index, name) in names.iter().enumerate() {
            let column = index as u32 % columns;
            let row = index as u32 / columns;
            let cell_x = column * CELL;
            let cell_y = row * CELL;

            let texture = match load_texture(pack, name) {
                Ok(texture) => texture,
                Err(error) => {
                    tracing::warn!(texture = %name, "using placeholder texture: {error:#}");
                    placeholder()
                }
            };

            let cell_image = padded_cell(&texture);
            image::imageops::replace(&mut image, &cell_image, cell_x as i64, cell_y as i64);
            tiles.insert(
                (*name).to_string(),
                UvRect {
                    u0: (cell_x + PAD) as f32 / width as f32,
                    v0: (cell_y + PAD) as f32 / height as f32,
                    u1: (cell_x + PAD + TILE) as f32 / width as f32,
                    v1: (cell_y + PAD + TILE) as f32 / height as f32,
                },
            );
        }

        Ok(Self {
            image,
            columns,
            rows,
            tiles,
        })
    }

    pub fn image(&self) -> &RgbaImage {
        &self.image
    }

    pub fn width(&self) -> u32 {
        self.image.width()
    }

    pub fn height(&self) -> u32 {
        self.image.height()
    }

    pub fn columns(&self) -> u32 {
        self.columns
    }

    pub fn rows(&self) -> u32 {
        self.rows
    }

    pub fn tile_count(&self) -> usize {
        self.tiles.len()
    }

    pub fn uv(&self, texture: &str) -> Option<UvRect> {
        self.tiles.get(texture).copied()
    }

    /// Like [`Self::uv`] but falls back to the first tile instead of failing.
    pub fn uv_or_first(&self, texture: &str) -> UvRect {
        self.uv(texture)
            .or_else(|| self.tiles.values().next().copied())
            .unwrap_or(UvRect {
                u0: 0.0,
                v0: 0.0,
                u1: 1.0,
                v1: 1.0,
            })
    }
}

fn load_texture(pack: &ResourcePack, name: &str) -> Result<RgbaImage> {
    let path = pack.texture_path(name);
    let bytes = pack
        .read(&path)?
        .with_context(|| format!("texture {name} is missing from the resource pack ({path})"))?;
    let image = image::load_from_memory_with_format(&bytes, ImageFormat::Png)
        .with_context(|| format!("decoding {path}"))?
        .to_rgba8();

    // Animated textures are vertical strips; use the first frame.
    let image = if image.height() > image.width() {
        image::imageops::crop_imm(&image, 0, 0, image.width(), image.width()).to_image()
    } else {
        image
    };

    if image.width() != TILE || image.height() != TILE {
        Ok(image::imageops::resize(
            &image,
            TILE,
            TILE,
            image::imageops::FilterType::Nearest,
        ))
    } else {
        Ok(image)
    }
}

fn padded_cell(texture: &RgbaImage) -> RgbaImage {
    RgbaImage::from_fn(CELL, CELL, |x, y| {
        let source_x = (x as i32 - PAD as i32).clamp(0, TILE as i32 - 1) as u32;
        let source_y = (y as i32 - PAD as i32).clamp(0, TILE as i32 - 1) as u32;
        *texture.get_pixel(source_x, source_y)
    })
}

fn placeholder() -> RgbaImage {
    RgbaImage::from_fn(TILE, TILE, |x, y| {
        if (x / 8 + y / 8) % 2 == 0 {
            Rgba([255, 0, 255, 255])
        } else {
            Rgba([0, 0, 0, 255])
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write_texture(pack_root: &std::path::Path, name: &str, color: [u8; 4]) {
        let dir = pack_root.join("assets/minecraft/textures/block");
        fs::create_dir_all(&dir).unwrap();
        let image = RgbaImage::from_pixel(TILE, TILE, Rgba(color));
        image.save(dir.join(format!("{name}.png"))).unwrap();
    }

    #[test]
    fn builds_atlas_with_padding() {
        let dir = tempfile::tempdir().unwrap();
        write_texture(dir.path(), "stone", [128, 128, 128, 255]);
        write_texture(dir.path(), "dirt", [134, 96, 67, 255]);
        let pack = ResourcePack::open(dir.path()).unwrap();
        let atlas = Atlas::build(&pack, ["stone", "dirt"]).unwrap();

        assert_eq!(atlas.tile_count(), 2);
        assert_eq!(atlas.width(), 2 * CELL);
        // Cells include a repeated-edge border so mip levels don't bleed.
        assert!(atlas.width() > 2 * TILE);

        let stone = atlas.uv("stone").unwrap();
        let (px, py) = (
            ((stone.u0 + stone.u1) * 0.5 * atlas.width() as f32) as u32,
            ((stone.v0 + stone.v1) * 0.5 * atlas.height() as f32) as u32,
        );
        assert_eq!(atlas.image().get_pixel(px, py), &Rgba([128, 128, 128, 255]));
        assert!(atlas.uv("missing").is_none());
    }

    #[test]
    fn crops_animated_textures_to_first_frame() {
        let dir = tempfile::tempdir().unwrap();
        let texture_dir = dir.path().join("assets/minecraft/textures/block");
        fs::create_dir_all(&texture_dir).unwrap();
        let mut strip = RgbaImage::new(TILE, TILE * 4);
        for frame in 0..4u8 {
            for y in 0..TILE {
                for x in 0..TILE {
                    strip.put_pixel(x, y + frame as u32 * TILE, Rgba([frame, 20, 30, 255]));
                }
            }
        }
        strip.save(texture_dir.join("water_still.png")).unwrap();
        let pack = ResourcePack::open(dir.path()).unwrap();
        let atlas = Atlas::build(&pack, ["water_still"]).unwrap();
        let uv = atlas.uv("water_still").unwrap();
        let x = (uv.u0 * atlas.width() as f32) as u32;
        let y = (uv.v0 * atlas.height() as f32) as u32;
        assert_eq!(atlas.image().get_pixel(x, y), &Rgba([0, 20, 30, 255]));
    }
}
