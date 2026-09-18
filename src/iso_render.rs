use std::collections::HashMap;
use std::sync::Arc;

use image::{RgbaImage, imageops};

use crate::cell::LotPack;
use crate::foliage::FOLIAGE_MAP;
use crate::pack::{Sprite, TextureLibrary, blend_sprite};
use crate::{TEXTURE_PATH, ZResult};

const TILE_W: i32 = 128; // SQUARE_WIDTH
pub const HALF_TILE_W: i32 = 64;
const TILE_H: i32 = 64; // SQUARE_HEIGHT
pub const HALF_TILE_H: i32 = 32;
pub const Z_HEIGHT: i32 = 192; // LAYER_HEIGHT (vertical lift per z-level)

pub const N: i32 = 256;

/// Squares along one edge of a cell (== `N`). A cell therefore covers global
/// squares `[cx*CELL_SQUARES, cx*CELL_SQUARES + CELL_SQUARES)` on each axis.
pub const CELL_SQUARES: i32 = 256;

const MAX_SPRITE_W: i32 = 1024;
const MAX_SPRITE_H: i32 = 1024;

pub fn load_sprite(name: &str) -> Option<RgbaImage> {
    let path = TEXTURE_PATH.join(format!("{name}.png"));
    Some(image::open(path).ok()?.to_rgba8())
}

#[inline]
pub fn project(sx: i32, sy: i32, z: i32) -> (i32, i32) {
    let screen_x = (sx - sy) * HALF_TILE_W;
    let screen_y = (sx + sy) * HALF_TILE_H - z * Z_HEIGHT;
    (screen_x, screen_y)
}

/// Resolve a tile name to a sprite: a direct pack sprite, or (for foliage/trees)
/// a blend of its sub-sprites composited from the pack. Blends are cached.
fn resolve_sprite(lib: &TextureLibrary, name: &str) -> Option<Arc<Sprite>> {
    if let Some(s) = lib.get(name) {
        return Some(s);
    }
    if let Some(&sub_names) = FOLIAGE_MAP.get(name)
        && let Some(blended) = blend_sprite(lib, sub_names)
    {
        let arc = Arc::new(blended);
        lib.insert(name, Arc::clone(&arc));
        return Some(arc);
    }
    None
}

pub fn render_iso_viewport(pack: &LotPack, lib: &TextureLibrary, cam_x: i32, cam_y: i32, view_w: u32, view_h: u32, layer: i32) -> ZResult<RgbaImage> {
    let mut img = RgbaImage::new(view_w, view_h);
    let mut cache: HashMap<i32, Option<Arc<Sprite>>> = HashMap::new();

    let hw = TILE_W / 2;
    let hh = TILE_H / 2;
    let gx_lo = (cam_x - MAX_SPRITE_W / 2).div_euclid(hw);
    let gx_hi = (cam_x + view_w as i32 + MAX_SPRITE_W / 2).div_euclid(hw);
    let d_lo = (cam_y - hh).div_euclid(hh);
    let d_hi = (cam_y + view_h as i32 + MAX_SPRITE_H).div_euclid(hh);
    dbg!(gx_lo, gx_hi, d_lo, d_hi);
    // Painter's order: far -> near along the (sx + sy) diagonal.
    for diag in d_lo.max(0)..=d_hi.min(2 * (N - 1)) {
        let sx_from = ((diag + gx_lo + 1) / 2).max((diag - (N - 1)).max(0));
        let sx_to = ((diag + gx_hi) / 2).min(diag.min(N - 1));
        for sx in sx_from..=sx_to {
            let sy = diag - sx;

            let stack = pack.get_block(sx as usize, sy as usize, layer);
            if stack.is_empty() {
                continue;
            }

            let (world_x, world_y) = project(sx, sy, layer);
            let base_x = world_x - cam_x;
            let base_y = world_y + hh - cam_y;

            for &tid in stack {
                let sprite = cache
                    .entry(tid)
                    .or_insert_with(|| pack.header.tiles.get(tid as usize).and_then(|name| resolve_sprite(lib, name)));
                let Some(sprite) = sprite.as_ref() else {
                    continue;
                };
                let dx = base_x + sprite.ox;
                let dy = base_y + sprite.oy;
                imageops::overlay(&mut img, &sprite.im, dx as i64, dy as i64);
            }
        }
    }

    Ok(img)
}

/// Composite one cell's `layer` into `img`, a `view_w`×`view_h` output tile
/// whose top-left pixel maps to global iso coordinate `(cam_x, cam_y)`.
///
/// Unlike [`render_iso_viewport`], squares are addressed in *global* space:
/// cell `(cell_x, cell_y)` owns squares `[cell_x*CELL_SQUARES, +CELL_SQUARES)`.
/// Because `project` is linear, the same painter's walk works for any cell — the
/// caller draws every overlapping cell back-to-front (ascending `cell_x+cell_y`)
/// and per layer so tall sprites overhang cell boundaries correctly.
///
/// `cache` is a per-cell tile-id → sprite memo; callers should clear/replace it
/// between cells since tile ids are indices into that cell's own tile table.
#[allow(clippy::too_many_arguments)]
pub fn render_cell_layer_into(
    img: &mut RgbaImage,
    pack: &LotPack,
    cell_x: u32,
    cell_y: u32,
    lib: &TextureLibrary,
    cam_x: i32,
    cam_y: i32,
    view_w: u32,
    view_h: u32,
    layer: i32,
    cache: &mut HashMap<i32, Option<Arc<Sprite>>>,
) {
    if !pack.has_layer(layer) {
        return;
    }

    let hw = HALF_TILE_W;
    let hh = HALF_TILE_H;
    let z = layer;

    // Visible window in (sx-sy) [screen-x] and (sx+sy) [diagonal] space, padded
    // by a full sprite so overhang from off-tile anchors is still drawn. The
    // z-lift shifts which diagonal band is on screen for this layer.
    let u_lo = (cam_x - MAX_SPRITE_W).div_euclid(hw);
    let u_hi = (cam_x + view_w as i32 + MAX_SPRITE_W).div_euclid(hw);
    let v_lo = (cam_y - MAX_SPRITE_H + z * Z_HEIGHT).div_euclid(hh);
    let v_hi = (cam_y + view_h as i32 + MAX_SPRITE_H + z * Z_HEIGHT).div_euclid(hh);

    let gx0 = cell_x as i32 * CELL_SQUARES;
    let gy0 = cell_y as i32 * CELL_SQUARES;
    let gx1 = gx0 + CELL_SQUARES - 1;
    let gy1 = gy0 + CELL_SQUARES - 1;

    let diag_lo = (gx0 + gy0).max(v_lo);
    let diag_hi = (gx1 + gy1).min(v_hi);

    for diag in diag_lo..=diag_hi {
        // gx bounded by: the cell box, the diagonal (gy in-box), and screen-x.
        let gx_from = gx0.max(diag - gy1).max((diag + u_lo).div_euclid(2));
        let gx_to = gx1.min(diag - gy0).min((diag + u_hi).div_euclid(2));
        for gx in gx_from..=gx_to {
            let gy = diag - gx;
            let lx = (gx - gx0) as usize;
            let ly = (gy - gy0) as usize;

            let stack = pack.get_block(lx, ly, layer);
            if stack.is_empty() {
                continue;
            }

            let (world_x, world_y) = project(gx, gy, layer);
            let base_x = world_x - cam_x;
            let base_y = world_y + hh - cam_y;

            for &tid in stack {
                let sprite = cache
                    .entry(tid)
                    .or_insert_with(|| pack.header.tiles.get(tid as usize).and_then(|name| resolve_sprite(lib, name)));
                let Some(sprite) = sprite.as_ref() else {
                    continue;
                };
                let dx = base_x + sprite.ox;
                let dy = base_y + sprite.oy;
                imageops::overlay(img, &sprite.im, dx as i64, dy as i64);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::top_render::read_lots;
    use std::path::{Path, PathBuf};

    #[tokio::test]
    async fn render_one() -> ZResult<()> {
        let path = PathBuf::from("D:\\SteamLibrary\\steamapps\\common\\ProjectZomboid\\media\\maps\\Muldraugh, KY\\");
        let texture_path = Path::new("D:\\SteamLibrary\\steamapps\\common\\ProjectZomboid\\media\\texturepacks");
        let pack = read_lots(path, 27, 32).await?;

        let mut lib = TextureLibrary::new();
        lib.load_dir(texture_path)?;
        println!("loaded {} sprites", lib.len());

        let (cx, cy) = project(N / 2, N / 2, 0);
        let mut img = render_iso_viewport(&pack, &lib, cx - 960, cy - 540, 1024, 1024, 0)?;
        let img2 = render_iso_viewport(&pack, &lib, cx - 960, cy - 540, 1024, 1024, 1)?;
        imageops::overlay(&mut img, &img2, 0, 0);
        img.save("iso_tile.png")?;

        Ok(())
    }
}
