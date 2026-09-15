use crate::cell::{LotPack, LotPackReader};
use crate::header::LotHeaderReader;
use crate::textures::PRECOMPUTED_COLORS;
use crate::{ImageRef, TEXTURE_PATH, ZError, ZResult};

use dashmap::DashMap;
use fnv::FnvBuildHasher;
use image::Rgba;
use std::path::PathBuf;
use std::sync::LazyLock;
use std::sync::atomic::AtomicU32;
use wide::f32x4;

pub static TEXTURE_CACHE: LazyLock<DashMap<String, f32x4, FnvBuildHasher>> = LazyLock::new(|| DashMap::with_capacity_and_hasher(2048, FnvBuildHasher::default()));
pub static ACCESS_COUNTS: LazyLock<DashMap<String, u64, FnvBuildHasher>> = LazyLock::new(|| DashMap::with_capacity_and_hasher(2048, FnvBuildHasher::default()));

pub static PRECOMPUTE_HIT: AtomicU32 = AtomicU32::new(0);
pub static CACHE_HIT: AtomicU32 = AtomicU32::new(0);
pub static PRECOMPUTE_MISS: AtomicU32 = AtomicU32::new(0);

#[inline]
pub fn sum_vecs(pixels: &[f32]) -> f32x4 {
    let mut out = f32x4::ZERO;
    for c in pixels.chunks_exact(4) {
        out += f32x4::from([c[0], c[1], c[2], 1.0]) * c[3]
    }
    out
}

#[inline]
fn read_tile(tile: &str) -> Option<f32x4> {
    let tile_path = TEXTURE_PATH.join(format!("{}.png", tile));
    let img = image::open(tile_path).ok()?.into_rgba32f();
    Some(sum_vecs(&img))
}

#[inline]
fn sum_colors(pallet: &[f32x4], pack: &LotPack, bx: usize, by: usize, layer: i32) -> Option<Rgba<u8>> {
    let tiles_ids = pack.get_block(bx, by, layer)?;
    let mut final_color = f32x4::ZERO;

    for &tile_id in tiles_ids {
        final_color += pallet[tile_id as usize];
    }

    let final_color = final_color.to_array();
    Some(Rgba([
        ((final_color[0] / final_color[3]) * 255.0) as u8,
        ((final_color[1] / final_color[3]) * 255.0) as u8,
        ((final_color[2] / final_color[3]) * 255.0) as u8,
        255,
    ]))
}

pub async fn read_lots(path_buf: PathBuf, x: usize, y: usize) -> ZResult<LotPack> {
    let header = path_buf.join(format!("{}_{}.lotheader", x, y));
    let pack = path_buf.join(format!("world_{}_{}.lotpack", x, y));
    let mut lot_reader = LotHeaderReader::new();
    let mut pak_reader = LotPackReader::new();
    let lot_header = lot_reader.load_lotheader(&header).await?;
    pak_reader.load_lotpack(&pack, lot_header).await
}

pub fn render_top(img: ImageRef, pack: LotPack, x_offset: usize, y_offset: usize, layer: i32) -> ZResult<()> {
    let pallet: Vec<_> = pack
        .header
        .tiles
        .iter()
        .map(|tile| PRECOMPUTED_COLORS.get(tile.as_str()).copied().or_else(|| read_tile(&tile)).unwrap_or(f32x4::ZERO))
        .collect();

    for bx in 0..256 {
        for by in 0..256 {
            if let Some(color) = sum_colors(&pallet, &pack, bx, by, layer) {
                unsafe {
                    let mut handout = img.request_handout((bx + x_offset) as u32, (by + y_offset) as u32);
                    handout.unsafe_put_pixel(color);
                }
            }
        }
    }
    Ok::<_, ZError>(())
}

#[cfg(test)]
mod tests {
    use std::error::Error;
    use std::path::PathBuf;
    use tokio::time::Instant;

    #[tokio::test]
    pub async fn test_read() -> Result<(), Box<dyn Error>> {
        let path = PathBuf::from("D:\\SteamLibrary\\steamapps\\common\\ProjectZomboid\\media\\maps\\Muldraugh, KY\\");
        let start = Instant::now();

        Ok(())
    }
}
