//! DeepZoom Image (DZI) layout — the tile pyramid format pzmap2dzi emits and
//! OpenSeadragon consumes.
//!
//! A DZI is a `<name>.dzi` descriptor plus a `<name>_files/<level>/<col>_<row>.png`
//! tile tree. The top level (`max_level`) is full resolution; each level below
//! halves both dimensions down to a 1×1 thumbnail. We render the base level from
//! a [`crate::world::World`] and build every coarser level by 2×2 box-downscaling
//! the level above it. Overlap is 0 (no duplicated tile borders).

use std::path::Path;

use image::{RgbaImage, imageops};

use crate::ZResult;

/// Layout parameters for one DZI pyramid.
#[derive(Debug, Clone, Copy)]
pub struct Dzi {
    pub width: u32,
    pub height: u32,
    pub tile_size: u32,
    pub overlap: u32,
    /// Level index whose dimensions equal the full image (the base level).
    pub max_level: u32,
}

impl Dzi {
    pub fn new(width: u32, height: u32, tile_size: u32) -> Self {
        let max_dim = width.max(height).max(1);
        let max_level = (max_dim as f64).log2().ceil() as u32;
        Self {
            width,
            height,
            tile_size,
            overlap: 0,
            max_level,
        }
    }

    /// Pixel dimensions of `level` (max_level == full size, each lower halves).
    pub fn level_dims(&self, level: u32) -> (u32, u32) {
        let shift = self.max_level - level;
        let scale = 2f64.powi(shift as i32);
        let w = ((self.width as f64) / scale).ceil() as u32;
        let h = ((self.height as f64) / scale).ceil() as u32;
        (w.max(1), h.max(1))
    }

    /// `(cols, rows)` of tiles at `level`.
    pub fn level_grid(&self, level: u32) -> (u32, u32) {
        let (w, h) = self.level_dims(level);
        (w.div_ceil(self.tile_size), h.div_ceil(self.tile_size))
    }

    /// The OpenSeadragon-compatible descriptor XML.
    pub fn descriptor_xml(&self) -> String {
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
             <Image xmlns=\"http://schemas.microsoft.com/deepzoom/2008\" Format=\"png\" Overlap=\"{}\" TileSize=\"{}\">\n\
             \x20 <Size Width=\"{}\" Height=\"{}\"/>\n\
             </Image>\n",
            self.overlap, self.tile_size, self.width, self.height
        )
    }
}

/// Build one tile of `level` by downscaling the (up to) 2×2 parent tiles that
/// cover it at `level + 1`. `files_dir` is the `<name>_files` directory whose
/// children are per-level tile folders. Returns `None` when every covering
/// parent tile is missing (the region is fully transparent).
pub fn downscale_tile(files_dir: &Path, dzi: &Dzi, level: u32, col: u32, row: u32) -> ZResult<Option<RgbaImage>> {
    let ts = dzi.tile_size;
    let (lw, lh) = dzi.level_dims(level);
    let tw = ts.min(lw - col * ts);
    let th = ts.min(lh - row * ts);
    if tw == 0 || th == 0 {
        return Ok(None);
    }

    // Parent pixel region this tile maps to: [2*col*ts, 2*col*ts + 2*tw).
    let parent = level + 1;
    let base_px = 2 * col * ts;
    let base_py = 2 * row * ts;

    // A 0-overlap tile spans at most two parent tiles per axis.
    let mut big = RgbaImage::new(2 * tw, 2 * th);
    let mut any = false;
    for pcol in (base_px / ts)..=((base_px + 2 * tw - 1) / ts) {
        for prow in (base_py / ts)..=((base_py + 2 * th - 1) / ts) {
            let tile_path = files_dir.join(format!("{parent}/{pcol}_{prow}.png"));
            let Ok(im) = image::open(&tile_path) else {
                continue;
            };
            let im = im.to_rgba8();
            let dx = (pcol * ts) as i64 - base_px as i64;
            let dy = (prow * ts) as i64 - base_py as i64;
            imageops::overlay(&mut big, &im, dx, dy);
            any = true;
        }
    }

    if !any {
        return Ok(None);
    }
    Ok(Some(imageops::resize(&big, tw, th, imageops::FilterType::Triangle)))
}
