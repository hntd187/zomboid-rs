use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

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

/// Widest / tallest sprite in the set (PZ jumbo trees are ~1024px). Used to pad
/// the visible-square cull so wide/tall sprites reaching into the viewport
/// aren't skipped. Lower these if your tile set has no jumbo sprites.
const MAX_SPRITE_W: i32 = 1024;
const MAX_SPRITE_H: i32 = 1024;

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

/// Resolve a tile name to a sprite: a direct pack sprite, or (for foliage/trees)
/// a blend of its sub-sprites composited from the pack. Blends are cached.
fn resolve_sprite(lib: &TextureLibrary, name: &str) -> Option<Arc<Sprite>> {
    if let Some(s) = lib.get(name) {
        return Some(s);
    }
    if let Some(&sub_names) = FOLIAGE_MAP.get(name) {
        if let Some(blended) = blend_sprite(lib, sub_names) {
            let arc = Arc::new(blended);
            lib.insert(name, arc.clone());
            return Some(arc);
        }
    }
    None
}

pub fn render_iso_viewport(pack: &LotPack, lib: &TextureLibrary, cam_x: i32, cam_y: i32, view_w: u32, view_h: u32) -> ZResult<(RgbaImage, BTreeMap<String, i32>)> {
    let mut img = RgbaImage::new(view_w, view_h);
    let mut missing: BTreeMap<String, i32> = BTreeMap::new();

    // Lazy per-render cache: tile id -> resolved sprite (blends included). Only
    // tiles actually drawn get resolved/blended, and each is resolved once.
    let mut cache: HashMap<i32, Option<Arc<Sprite>>> = HashMap::new();

    let min_layer = pack.header.min_layer;
    let hw = TILE_W / 2;
    let hh = TILE_H / 2;

    // Cull to squares whose sprites can touch the viewport: map the camera rect
    // back to (sx - sy) and (sx + sy) ranges, padded by the max sprite size
    // (sprites are centered horizontally and extend upward, so squares below the
    // viewport can still reach up into it).
    let gx_lo = (cam_x - MAX_SPRITE_W / 2).div_euclid(hw);
    let gx_hi = (cam_x + view_w as i32 + MAX_SPRITE_W / 2).div_euclid(hw);
    let d_lo = (cam_y - hh).div_euclid(hh);
    let d_hi = (cam_y + view_h as i32 + MAX_SPRITE_H).div_euclid(hh);

    // Painter's order: far -> near along the (sx + sy) diagonal.
    for diag in d_lo.max(0)..=d_hi.min(2 * (N - 1)) {
        // sx - sy = 2*sx - diag must lie in [gx_lo, gx_hi]; also 0 <= sx, sy < N.
        let sx_from = ((diag + gx_lo + 1) / 2).max((diag - (N - 1)).max(0));
        let sx_to = ((diag + gx_hi) / 2).min(diag.min(N - 1));
        for sx in sx_from..=sx_to {
            let sy = diag - sx;

            let stack = pack.get_block(sx as usize, sy as usize, min_layer);
            if stack.is_empty() {
                continue;
            }

            let (world_x, world_y) = project(sx, sy, min_layer);
            // Screen position of the square's bottom-center.
            let base_x = world_x - cam_x;
            let base_y = world_y + hh - cam_y;

            for &tid in stack {
                let sprite = cache
                    .entry(tid)
                    .or_insert_with(|| pack.header.tiles.get(tid as usize).and_then(|name| resolve_sprite(lib, name)));
                let Some(sprite) = sprite.as_ref() else {
                    if let Some(name) = pack.header.tiles.get(tid as usize) {
                        *missing.entry(name.to_string()).or_insert(0) += 1;
                    }
                    continue;
                };
                // bottom-center + the sprite's own offset.
                let dx = base_x + sprite.ox;
                let dy = base_y + sprite.oy;
                imageops::overlay(&mut img, &sprite.im, dx as i64, dy as i64);
            }
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
