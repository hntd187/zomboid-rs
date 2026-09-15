//! Isometric renderer: blits real tile sprites at iso-projected positions,
//! back-to-front, into a viewport-sized image. Sprites and their per-tile
//! offsets come from a `TextureLibrary` (parsed from the game's `.pack` files),
//! so walls/fixtures land correctly — not just floors.
//!
//! Uses what the parser produces (per-square, per-layer stacks of tile IDs)
//! plus the sprite atlas + offsets from the packs. This is the CPU reference
//! implementation; it maps directly onto a wgpu version later (camera transform
//! + one instanced textured quad per drawn tile, sampling a sprite atlas).
//!
//! SKETCH: not verified by `cargo check` in this environment (no network to
//! resolve deps). Wire in with `pub mod iso_render;` and `pub mod pack;` in
//! lib.rs. No new deps — uses the `image` crate you already depend on.

use std::collections::BTreeSet;

use image::{imageops, RgbaImage};

use crate::cell::LotPack;
use crate::pack::TextureLibrary;
use crate::ZResult;

/// Iso constants, from pzmap2dzi's `pzdzi.py`.
const TILE_W: i32 = 128; // SQUARE_WIDTH
const TILE_H: i32 = 64; // SQUARE_HEIGHT
const Z_HEIGHT: i32 = 192; // LAYER_HEIGHT (vertical lift per z-level)
/// Squares per cell edge (CELL_SIZE_IN_BLOCKS * BLOCK_SIZE_IN_SQUARES = 32*8).
const N: i32 = 256;

/// World square (sx, sy, z) -> screen pixel of the floor-diamond *center*.
#[inline]
pub fn project(sx: i32, sy: i32, z: i32) -> (i32, i32) {
    let screen_x = (sx - sy) * (TILE_W / 2);
    let screen_y = (sx + sy) * (TILE_H / 2) - z * Z_HEIGHT;
    (screen_x, screen_y)
}

/// Render one pack into a `view_w` x `view_h` image. `cam_x`/`cam_y` are the
/// screen-space coordinates (from `project`) that map to the top-left of the
/// viewport — i.e. your camera/pan position. Sprites outside the viewport are
/// clipped for free by `imageops::overlay`.
///
/// Returns the image plus the sorted set of drawn tile names whose sprite was
/// missing from the library — your signal for what still isn't loading.
pub fn render_iso_viewport(
    pack: &LotPack,
    lib: &TextureLibrary,
    cam_x: i32,
    cam_y: i32,
    view_w: u32,
    view_h: u32,
) -> ZResult<(RgbaImage, Vec<String>)> {
    let mut img = RgbaImage::new(view_w, view_h);
    let mut missing: BTreeSet<String> = BTreeSet::new();

    // Resolve the pack's palette (tile id -> sprite) once, like PRECOMPUTED_COLORS.
    let sprite_of: Vec<Option<&crate::pack::Sprite>> =
        pack.header.tiles.iter().map(|name| lib.get(name)).collect();

    let min_layer = pack.header.min_layer;
    let max_layer = pack.header.max_layer;

    // Painter's order: bottom layer first, then far->near along the (sx+sy)
    // diagonal so nearer/higher tiles overlay farther ones.
    for layer in min_layer..max_layer {
        let z = layer - min_layer;
        for diag in 0..=(2 * (N - 1)) {
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
                // Screen position of the square's bottom-center.
                let base_x = world_x - cam_x;
                let base_y = world_y + TILE_H / 2 - cam_y;

                for &tid in stack {
                    let sprite = sprite_of.get(tid as usize).copied().flatten();
                    let Some(sprite) = sprite else {
                        if let Some(name) = pack.header.tiles.get(tid as usize) {
                            missing.insert(name.clone());
                        }
                        continue;
                    };
                    // bottom-center + the sprite's own offset (this is what puts
                    // walls/fixtures in the right place instead of on the floor).
                    let dx = base_x + sprite.ox;
                    let dy = base_y + sprite.oy;
                    imageops::overlay(&mut img, &sprite.im, dx as i64, dy as i64);
                }
            }
        }
    }

    Ok((img, missing.into_iter().collect()))
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
        lib.load_dir(Path::new(
            "D:\\SteamLibrary\\steamapps\\common\\ProjectZomboid\\media\\texturepacks",
        ))?;
        eprintln!("loaded {} sprites", lib.len());

        let (cx, cy) = project(N / 2, N / 2, 0);
        let (img, missing) = render_iso_viewport(&pack, &lib, cx - 960, cy - 540, 1920, 1080)?;
        img.save("iso_tile.png")?;

        eprintln!("{} drawn tile names had no sprite:", missing.len());
        for name in missing.iter().take(40) {
            eprintln!("  {name}");
        }
        Ok(())
    }
}
