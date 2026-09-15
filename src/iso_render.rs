//! Isometric renderer: blits the real tile sprites (from `TEXTURE_PATH`) at
//! iso-projected positions, back-to-front, into a viewport-sized image.
//!
//! Uses ONLY what the parser already produces (per-square, per-layer stacks of
//! tile IDs) plus the sprite PNGs you already have on disk. This is the CPU
//! reference implementation; it maps directly onto a wgpu version later
//! (camera transform in the vertex shader + one instanced textured quad per
//! drawn tile, sampling a sprite atlas).
//!
//! SKETCH: not verified by `cargo check` in this environment (no network to
//! resolve deps here). Wire in with `pub mod iso_render;` in lib.rs. No new
//! deps — uses the `image` crate you already depend on.

use std::collections::HashMap;

use image::{imageops, RgbaImage};

use crate::cell::LotPack;
use crate::{TEXTURE_PATH, ZResult};

/// Iso diamond footprint, from the 128x64 diamond in `rooms.rs`.
const TILE_W: i32 = 128;
const TILE_H: i32 = 64;
/// Vertical screen offset per z-level. Approximate — PZ's true value depends on
/// the tilesets; tune to taste (roughly one storey of sprite height).
const Z_HEIGHT: i32 = 96;
/// Squares per cell edge (CELL_SIZE_IN_BLOCKS * BLOCK_SIZE_IN_SQUARES = 32*8).
const N: i32 = 256;

/// World square (sx, sy, z) -> screen pixel of the floor-diamond *center*.
#[inline]
pub fn project(sx: i32, sy: i32, z: i32) -> (i32, i32) {
    let screen_x = (sx - sy) * (TILE_W / 2);
    let screen_y = (sx + sy) * (TILE_H / 2) - z * Z_HEIGHT;
    (screen_x, screen_y)
}

/// Load a tile sprite as RGBA, or None if the file is missing. (Like
/// `read_tile`, but keeps the pixels instead of averaging them to a color.)
fn load_sprite(name: &str) -> Option<RgbaImage> {
    let path = TEXTURE_PATH.join(format!("{name}.png"));
    Some(image::open(path).ok()?.to_rgba8())
}

/// Render one pack into a `view_w` x `view_h` image. `cam_x`/`cam_y` are the
/// screen-space coordinates (from `project`) that map to the top-left of the
/// viewport — i.e. your camera/pan position. Sprites outside the viewport are
/// clipped for free by `imageops::overlay`.
pub fn render_iso_viewport(
    pack: &LotPack,
    cam_x: i32,
    cam_y: i32,
    view_w: u32,
    view_h: u32,
) -> ZResult<RgbaImage> {
    let mut img = RgbaImage::new(view_w, view_h);

    // Cache decoded sprites; a cell references each unique sprite thousands of
    // times. `None` caches misses so we don't re-hit the disk for them.
    let mut sprites: HashMap<i32, Option<RgbaImage>> = HashMap::new();

    let min_layer = pack.header.min_layer;
    let max_layer = pack.header.max_layer;

    // Painter's order: bottom layer first, then far->near along the (sx+sy)
    // diagonal so nearer/higher tiles overlay farther ones.
    for layer in min_layer..max_layer {
        let z = layer - min_layer;
        for diag in 0..=(2 * (N - 1)) {
            // sx from max(0, diag-(N-1)) .. min(N-1, diag)
            let sx_start = (diag - (N - 1)).max(0);
            let sx_end = diag.min(N - 1);
            for sx in sx_start..=sx_end {
                let sy = diag - sx;

                let Some(stack) = pack.get_block(sx as usize, sy as usize, layer) else {
                    continue;
                };
                if stack.is_empty() {
                    continue;
                }

                let (world_x, world_y) = project(sx, sy, z);
                for &tid in stack {
                    let sprite = sprites.entry(tid).or_insert_with(|| {
                        pack.header
                            .tiles
                            .get(tid as usize)
                            .and_then(|name| load_sprite(name))
                    });
                    let Some(sprite) = sprite else { continue };

                    // Anchor: center the sprite horizontally on the diamond and
                    // sit its bottom edge on the diamond's bottom vertex. Real
                    // PZ tilesets carry per-tile offsets (in the .tiles defs) —
                    // wire those in here for pixel-accurate placement.
                    let dx = world_x - sprite.width() as i32 / 2 - cam_x;
                    let dy = world_y + TILE_H / 2 - sprite.height() as i32 - cam_y;

                    // src-over alpha blend, clipped to the viewport bounds.
                    imageops::overlay(&mut img, sprite, dx as i64, dy as i64);
                }
            }
        }
    }

    Ok(img)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::top_render::read_lots;
    use std::path::PathBuf;

    #[tokio::test]
    async fn render_one() -> ZResult<()> {
        let path = PathBuf::from("D:\\SteamLibrary\\steamapps\\common\\ProjectZomboid\\media\\maps\\Muldraugh, KY\\");
        let pack = read_lots(path, 27, 32).await?;
        // Camera at the cell's visual center-ish; 1920x1080 window.
        let (cx, cy) = project(N / 2, N / 2, 0);
        let img = render_iso_viewport(&pack, cx - 960, cy - 540, 1920, 1080)?;
        img.save("iso_tile.png")?;
        Ok(())
    }
}
