//! Multi-cell "world" layer for whole-map isometric rendering.
//!
//! The per-cell renderer in [`crate::iso_render`] only understands one
//! [`LotPack`]. `World` stitches every cell of a map into one global isometric
//! plane: it discovers the cells on disk, computes the plane's pixel bounds, and
//! renders any fixed-size output tile by compositing the cells that overlap it —
//! back-to-front, per layer — so tall sprites overhang cell boundaries.
//!
//! Cells are loaded on demand through a bounded LRU cache, so rendering a large
//! map never needs every cell resident at once. Rendering is done per output
//! tile, which makes the whole thing embarrassingly parallel: tiles share only
//! the (synchronised) cell cache and the immutable texture library.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use image::RgbaImage;

use crate::ZResult;
use crate::cell::LotPack;
use crate::iso_render::{CELL_SQUARES, HALF_TILE_H, HALF_TILE_W, Z_HEIGHT, render_cell_layer_into};
use crate::pack::TextureLibrary;
use crate::top_render::read_lots_sync;

/// Pixel padding around the map bounds so tall sprites anchored just outside the
/// square grid still have room. Matches the sprite clamp in `iso_render`.
const SPRITE_MARGIN: i32 = 1024;

enum CellState {
    Present(Arc<LotPack>),
    Missing,
}

/// Minimal LRU cache of loaded cells. Cell working sets per tile are tiny (a few
/// cells) and the capacity is small, so an O(cap) eviction scan is cheap and
/// avoids pulling in an external LRU crate.
struct CellCache {
    cap: usize,
    tick: u64,
    map: HashMap<(u32, u32), (CellState, u64)>,
}

impl CellCache {
    fn new(cap: usize) -> Self {
        Self {
            cap: cap.max(1),
            tick: 0,
            map: HashMap::new(),
        }
    }

    fn get(&mut self, key: &(u32, u32)) -> Option<&CellState> {
        self.tick += 1;
        let tick = self.tick;
        let entry = self.map.get_mut(key)?;
        entry.1 = tick;
        Some(&entry.0)
    }

    fn put(&mut self, key: (u32, u32), value: CellState) {
        self.tick += 1;
        let tick = self.tick;
        self.map.insert(key, (value, tick));
        if self.map.len() > self.cap
            && let Some(lru) = self.map.iter().min_by_key(|(_, (_, t))| *t).map(|(k, _)| *k)
        {
            self.map.remove(&lru);
        }
    }
}

/// A whole map as one global isometric plane.
pub struct World {
    map_dir: PathBuf,
    present: HashSet<(u32, u32)>,
    min_cell: (u32, u32),
    max_cell: (u32, u32),
    /// z-layers to composite, ascending (drawn back-to-front by height).
    layers: Vec<i32>,
    /// Pixel `(0, 0)` of the plane maps to global iso coordinate `(origin_x, origin_y)`.
    origin_x: i32,
    origin_y: i32,
    width: u32,
    height: u32,
    cache: Mutex<CellCache>,
}

impl World {
    /// Discover every `world_{x}_{y}.lotpack` under `map_dir` and compute the
    /// global iso pixel bounds for the given `layers` (ascending z-levels).
    pub fn scan(map_dir: &Path, layers: Vec<i32>, cache_cells: usize) -> ZResult<Self> {
        assert!(!layers.is_empty(), "at least one layer must be rendered");

        let mut present = HashSet::new();
        for entry in std::fs::read_dir(map_dir)? {
            let path = entry?.path();
            if let Some(name) = path.file_name().and_then(|n| n.to_str())
                && let Some((x, y)) = parse_cell_name(name)
            {
                present.insert((x, y));
            }
        }
        assert!(!present.is_empty(), "no world_{{x}}_{{y}}.lotpack cells found in {}", map_dir.display());

        let min_cx = present.iter().map(|c| c.0).min().unwrap();
        let max_cx = present.iter().map(|c| c.0).max().unwrap();
        let min_cy = present.iter().map(|c| c.1).min().unwrap();
        let max_cy = present.iter().map(|c| c.1).max().unwrap();

        // Global square box (inclusive).
        let gx_min = min_cx as i32 * CELL_SQUARES;
        let gx_max = (max_cx as i32 + 1) * CELL_SQUARES - 1;
        let gy_min = min_cy as i32 * CELL_SQUARES;
        let gy_max = (max_cy as i32 + 1) * CELL_SQUARES - 1;

        let z_lo = *layers.first().unwrap();
        let z_hi = *layers.last().unwrap();

        // screen_x = (gx - gy) * HALF_TILE_W
        let sx_min = (gx_min - gy_max) * HALF_TILE_W;
        let sx_max = (gx_max - gy_min) * HALF_TILE_W;
        // screen_y = (gx + gy) * HALF_TILE_H - z * Z_HEIGHT
        // Highest z lifts content up (smaller y); lowest z pushes it down.
        let sy_min = (gx_min + gy_min) * HALF_TILE_H - z_hi * Z_HEIGHT;
        let sy_max = (gx_max + gy_max) * HALF_TILE_H - z_lo * Z_HEIGHT;

        let origin_x = sx_min - SPRITE_MARGIN;
        let origin_y = sy_min - SPRITE_MARGIN;
        let width = (sx_max - sx_min + 2 * SPRITE_MARGIN) as u32;
        let height = (sy_max - sy_min + 2 * SPRITE_MARGIN) as u32;

        Ok(Self {
            map_dir: map_dir.to_path_buf(),
            present,
            min_cell: (min_cx, min_cy),
            max_cell: (max_cx, max_cy),
            layers,
            origin_x,
            origin_y,
            width,
            height,
            cache: Mutex::new(CellCache::new(cache_cells)),
        })
    }

    pub fn width(&self) -> u32 {
        self.width
    }
    pub fn height(&self) -> u32 {
        self.height
    }
    pub fn cell_count(&self) -> usize {
        self.present.len()
    }
    pub fn cell_bounds(&self) -> ((u32, u32), (u32, u32)) {
        (self.min_cell, self.max_cell)
    }

    /// Fetch a cell, loading and caching it on first use. Returns `None` for
    /// cells that are absent on disk or fail to load.
    fn get_cell(&self, cx: u32, cy: u32) -> Option<Arc<LotPack>> {
        {
            let mut cache = self.cache.lock().unwrap();
            if let Some(state) = cache.get(&(cx, cy)) {
                return match state {
                    CellState::Present(p) => Some(Arc::clone(p)),
                    CellState::Missing => None,
                };
            }
        }
        // Load outside the lock so concurrent tiles don't serialise on file IO.
        let loaded = read_lots_sync(&self.map_dir, cx as usize, cy as usize).ok().map(Arc::new);
        let mut cache = self.cache.lock().unwrap();
        let ret = loaded.clone();
        cache.put((cx, cy), loaded.map_or(CellState::Missing, CellState::Present));
        ret
    }

    /// Cells whose sprites can touch the tile at pixel `(cam_x, cam_y)` of size
    /// `view_w`×`view_h`, ordered back-to-front (ascending `cx + cy`).
    fn overlapping_cells(&self, cam_x: i32, cam_y: i32, view_w: u32, view_h: u32) -> Vec<(u32, u32)> {
        let z_lo = *self.layers.first().unwrap();
        let z_hi = *self.layers.last().unwrap();

        // Pixel rect to cover, padded for sprite overhang.
        let px_lo = cam_x - SPRITE_MARGIN;
        let px_hi = cam_x + view_w as i32 + SPRITE_MARGIN;
        let py_lo = cam_y - SPRITE_MARGIN;
        let py_hi = cam_y + view_h as i32 + SPRITE_MARGIN;

        // Invert the projection. u = gx - gy = screen_x / HALF_TILE_W,
        // v = gx + gy = (screen_y + z * Z_HEIGHT) / HALF_TILE_H.
        let u_lo = px_lo.div_euclid(HALF_TILE_W);
        let u_hi = px_hi.div_euclid(HALF_TILE_W);
        let v_lo = (py_lo + z_lo * Z_HEIGHT).div_euclid(HALF_TILE_H);
        let v_hi = (py_hi + z_hi * Z_HEIGHT).div_euclid(HALF_TILE_H);

        // gx = (u + v) / 2, gy = (v - u) / 2 over the corner extremes.
        let gx_lo = (u_lo + v_lo).div_euclid(2);
        let gx_hi = (u_hi + v_hi).div_euclid(2);
        let gy_lo = (v_lo - u_hi).div_euclid(2);
        let gy_hi = (v_hi - u_lo).div_euclid(2);

        let cx_lo = gx_lo.div_euclid(CELL_SQUARES).max(self.min_cell.0 as i32);
        let cx_hi = gx_hi.div_euclid(CELL_SQUARES).min(self.max_cell.0 as i32);
        let cy_lo = gy_lo.div_euclid(CELL_SQUARES).max(self.min_cell.1 as i32);
        let cy_hi = gy_hi.div_euclid(CELL_SQUARES).min(self.max_cell.1 as i32);

        let mut cells = Vec::new();
        for cx in cx_lo..=cx_hi {
            for cy in cy_lo..=cy_hi {
                let c = (cx as u32, cy as u32);
                if self.present.contains(&c) {
                    cells.push(c);
                }
            }
        }
        // Back-to-front: farther cells (smaller cx + cy) first.
        cells.sort_by_key(|&(cx, cy)| (cx + cy, cx));
        cells
    }

    /// Render the base-resolution tile at grid `(col, row)` of edge `tile_size`.
    /// Edge tiles are clipped to the true plane size. Returns `None` when the
    /// tile is fully transparent (no cell content), so callers can skip writing
    /// empty tiles — OpenSeadragon treats missing tiles as transparent.
    pub fn render_tile(&self, lib: &TextureLibrary, col: u32, row: u32, tile_size: u32) -> Option<RgbaImage> {
        let px = col * tile_size;
        let py = row * tile_size;
        let tw = tile_size.min(self.width - px);
        let th = tile_size.min(self.height - py);
        if tw == 0 || th == 0 {
            return None;
        }

        let cam_x = self.origin_x + px as i32;
        let cam_y = self.origin_y + py as i32;

        let mut img = RgbaImage::new(tw, th);
        let cells = self.overlapping_cells(cam_x, cam_y, tw, th);
        if cells.is_empty() {
            return None;
        }

        let mut drew = false;
        for &layer in &self.layers {
            for &(cx, cy) in &cells {
                let Some(pack) = self.get_cell(cx, cy) else {
                    continue;
                };
                if !pack.has_layer(layer) {
                    continue;
                }
                let mut cache = std::collections::HashMap::new();
                render_cell_layer_into(&mut img, &pack, cx, cy, lib, cam_x, cam_y, tw, th, layer, &mut cache);
                drew = true;
            }
        }

        if drew && img.pixels().any(|p| p.0[3] != 0) { Some(img) } else { None }
    }
}

/// Parse `world_{x}_{y}.lotpack` into `(x, y)`.
fn parse_cell_name(name: &str) -> Option<(u32, u32)> {
    let stem = name.strip_prefix("world_")?.strip_suffix(".lotpack")?;
    let (x, y) = stem.split_once('_')?;
    Some((x.parse().ok()?, y.parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::parse_cell_name;

    #[test]
    fn parses_cell_names() {
        assert_eq!(parse_cell_name("world_27_32.lotpack"), Some((27, 32)));
        assert_eq!(parse_cell_name("27_32.lotheader"), None);
        assert_eq!(parse_cell_name("world_x_y.lotpack"), None);
    }
}
