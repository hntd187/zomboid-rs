use std::collections::{BTreeMap, BTreeSet};

use image::{RgbaImage, imageops};

use crate::cell::LotPack;
use crate::foliage::FOLIAGE_MAP;
use crate::pack::{Sprite, TextureLibrary, blend_sprite};
use crate::{TEXTURE_PATH, ZResult};

/// Iso constants, from pzmap2dzi's `pzdzi.py`.
const TILE_W: i32 = 128; // SQUARE_WIDTH
const TILE_H: i32 = 64; // SQUARE_HEIGHT
const Z_HEIGHT: i32 = 192; // LAYER_HEIGHT (vertical lift per z-level)

pub const N: i32 = 256;

pub fn load_sprite(name: &str) -> Option<RgbaImage> {
    let path = TEXTURE_PATH.join(format!("{name}.png"));
    Some(image::open(path).ok()?.to_rgba8())
}

#[inline]
pub fn project(sx: i32, sy: i32, z: i32) -> (i32, i32) {
    let screen_x = (sx - sy) * (TILE_W / 2);
    let screen_y = (sx + sy) * (TILE_H / 2) - z * Z_HEIGHT;
    (screen_x, screen_y)
}

pub fn render_iso_viewport(pack: &LotPack, lib: &TextureLibrary, cam_x: i32, cam_y: i32, view_w: u32, view_h: u32) -> ZResult<(RgbaImage, BTreeMap<String, i32>)> {
    let mut img = RgbaImage::new(view_w, view_h);
    let mut missing: BTreeMap<String, i32> = BTreeMap::new();

    // Resolve the pack's palette (tile id -> sprite) once, like PRECOMPUTED_COLORS.
    let sprite_of: Vec<Option<Sprite>> = pack.header.tiles.iter().map(|name| lib.get(name).map(|v| v.value().clone())).collect();

    let min_layer = pack.header.min_layer;
    let max_layer = pack.header.max_layer;

    // Painter's order: bottom layer first, then far->near along the (sx+sy)
    // diagonal so nearer/higher tiles overlay farther ones.
    // for layer in min_layer..max_layer {
    //     let z = layer - min_layer;
    for diag in 0..=(2 * (N - 1)) {
        let sx_start = (diag - (N - 1)).max(0);
        let sx_end = diag.min(N - 1);
        for sx in sx_start..=sx_end {
            let sy = diag - sx;

            let stack = pack.get_block(sx as usize, sy as usize, min_layer);
            if stack.is_empty() {
                continue;
            }

            let (world_x, world_y) = project(sx, sy, min_layer);
            // Screen position of the square's bottom-center.
            let base_x = world_x - cam_x;
            let base_y = world_y + TILE_H / 2 - cam_y;

            for &tid in stack {
                let tile_name = pack.header.tiles.get(tid as usize).unwrap();

                let mut sprite = sprite_of.get(tid as usize).cloned().flatten().or(lib.get(tile_name).map(|v| v.value().clone()));
                if sprite.is_none() {
                    *missing.entry(tile_name.to_string()).or_insert(0) += 1;
                    if let Some(s) = Some(tile_name).and_then(|name| load_sprite(name)) {
                        let dx = world_x - s.width() as i32 / 2 - cam_x;
                        let dy = world_y + TILE_H / 2 - s.height() as i32 - cam_y;
                        let ss = Sprite { im: s, ox: dx, oy: dy };
                        lib.insert(tile_name, ss.clone());
                        sprite = Some(ss)
                    } else {
                        println!("Did not locate sprite: {}", tile_name);
                        continue;
                    }
                }
                let sprite = sprite.unwrap();
                // bottom-center + the sprite's own offset (this is what puts
                // walls/fixtures in the right place instead of on the floor).
                let dx = base_x + sprite.ox;
                let dy = base_y + sprite.oy;

                imageops::overlay(&mut img, &sprite.im, dx as i64, dy as i64);
            }
            // }
        }
    }

    Ok((img, missing))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::top_render::read_lots;
    use std::path::{Path, PathBuf};

    #[tokio::test]
    async fn render_one() -> ZResult<()> {
        let path = PathBuf::from("D:\\SteamLibrary\\steamapps\\common\\ProjectZomboid\\media\\maps\\Muldraugh, KY\\");
        let pack = read_lots(path, 27, 32).await?;

        // Load the game's texture packs (walls, fixtures, everything).
        let mut lib = TextureLibrary::new();
        lib.load_dir(Path::new("D:\\SteamLibrary\\steamapps\\common\\ProjectZomboid\\media\\texturepacks"))?;
        eprintln!("loaded {} sprites", lib.len());

        let (cx, cy) = project(N / 2, N / 2, 0);
        let (img, missing) = render_iso_viewport(&pack, &lib, cx - 960, cy - 540, 1024, 1024)?;
        img.save("iso_tile.png")?;

        eprintln!("{} drawn tile names had no sprite:", missing.len());
        for (name, c) in missing.iter().take(40) {
            eprintln!("{name}: {c}");
        }
        Ok(())
    }
}
