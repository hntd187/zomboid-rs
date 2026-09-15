//! Presentation-layer abstraction.
//!
//! The idea: decouple *computing* cell colors (your existing `sum_colors` /
//! `render_top` logic) from *presenting* them. The CPU produces a `CellColors`
//! buffer; a `Backend` turns that into pixels on a PNG, a native window, or a
//! web canvas. Native and web share the exact same `WgpuBackend`.
//!
//! NOTE: sketch only. Not wired into `lib.rs`; adapt to your crate.

use crate::ZResult;

/// A finished, backend-agnostic canvas of one averaged color per map square.
///
/// This is what `render_top` should produce instead of writing straight into
/// an `image::ImageBuffer`. Row-major, 4 bytes/pixel (RGBA8), `width * height`
/// pixels. For the stitched case this is the *whole* canvas (e.g. 256*24 sq.)
/// and each pack fills its own sub-rect.
pub struct CellColors {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<[u8; 4]>,
}

impl CellColors {
    pub fn new(width: u32, height: u32) -> Self {
        Self { width, height, pixels: vec![[0, 0, 0, 0]; (width * height) as usize] }
    }

    #[inline]
    pub fn put(&mut self, x: u32, y: u32, rgba: [u8; 4]) {
        self.pixels[(y * self.width + x) as usize] = rgba;
    }

    /// Flat byte view for texture upload / PNG encoding.
    pub fn as_bytes(&self) -> &[u8] {
        // [u8; 4] is layout-compatible with 4 contiguous u8s.
        bytemuck::cast_slice(&self.pixels)
    }
}

/// A vector overlay (room outlines, isometric shapes). Left as a stub so you
/// can see where Lyon/Vello tessellation would slot in later.
pub struct Polygon {
    pub points: Vec<[f32; 2]>,
    pub color: [u8; 4],
}

/// Anything that can present a finished canvas.
///
/// `present` is called once per frame (window) or once total (PNG). Keep the
/// heavy color computation *outside* — a backend just uploads/draws.
pub trait Backend {
    fn present(&mut self, colors: &CellColors, overlays: &[Polygon]) -> ZResult<()>;
}

/// Your current output path, behind the trait. Encodes a PNG via the `image`
/// crate — no GPU, no window. Keeps the CPU path alive for headless/batch use.
pub struct ImageBackend {
    pub path: std::path::PathBuf,
}

impl Backend for ImageBackend {
    fn present(&mut self, colors: &CellColors, _overlays: &[Polygon]) -> ZResult<()> {
        let buf = image::RgbaImage::from_raw(colors.width, colors.height, colors.as_bytes().to_vec())
            .expect("buffer size matches width*height*4");
        buf.save(&self.path)?;
        Ok(())
    }
}
