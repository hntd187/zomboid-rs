use crate::ZResult;
use dashmap::DashMap;
use image::{RgbaImage, imageops};
use std::io::{Error, ErrorKind};
use std::path::Path;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct Sprite {
    pub im: RgbaImage,
    pub ox: i32,
    pub oy: i32,
}

/// Little-endian byte cursor over the pack file.
struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn u32(&mut self) -> ZResult<u32> {
        let data: [u8; 4] = self.data[self.pos..self.pos + 4].try_into()?;
        let v = u32::from_le_bytes(data);
        self.pos += 4;
        Ok(v)
    }

    fn i32(&mut self) -> ZResult<i32> {
        let v = i32::from_le_bytes(self.data[self.pos..self.pos + 4].try_into()?);
        self.pos += 4;
        Ok(v)
    }

    /// u32 length prefix followed by that many bytes.
    fn bytes_with_len(&mut self) -> ZResult<&'a [u8]> {
        let n = self.u32()? as usize;
        let b = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(b)
    }

    /// Read up to and including `pattern` (matches `util.read_until`).
    fn until(&mut self, pattern: &[u8]) -> ZResult<&'a [u8]> {
        let start = self.pos;
        let rel = self.data[start..]
            .windows(pattern.len())
            .position(|w| w == pattern)
            .ok_or_else(|| Error::new(ErrorKind::InvalidData, "pack: terminator not found"))?;
        let end = start + rel + pattern.len();
        self.pos = end;
        Ok(&self.data[start..end])
    }
}

/// One texture's metadata within a page.
struct TexMeta {
    name: String,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    ox: i32,
    oy: i32,
    ow: i32,
    oh: i32,
}

/// Name -> sprite. Load one or more `.pack` files into it.
#[derive(Default)]
pub struct TextureLibrary {
    sprites: DashMap<String, Arc<Sprite>>,
}

impl TextureLibrary {
    pub fn new() -> Self {
        Self::default()
    }

    /// Cheap: clones the `Arc`, not the sprite pixels.
    pub fn get(&self, name: &str) -> Option<Arc<Sprite>> {
        self.sprites.get(name).map(|r| r.value().clone())
    }

    pub fn insert(&self, name: &str, sprite: Arc<Sprite>) {
        self.sprites.insert(name.to_string(), sprite);
    }

    pub fn len(&self) -> usize {
        self.sprites.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sprites.is_empty()
    }

    /// Load every `*.pack` file in a directory (e.g. `media/texturepacks`).
    pub fn load_dir(&mut self, dir: &Path) -> ZResult<()> {
        for entry in std::fs::read_dir(dir)? {
            let path = entry?.path();
            if path.extension().and_then(|e| e.to_str()) == Some("pack") {
                self.load_pack(&path)?;
            }
        }
        println!("Loaded {} sprites", self.sprites.len());
        Ok(())
    }

    pub fn load_pack(&mut self, path: &Path) -> ZResult<()> {
        let data = std::fs::read(path)?;
        let mut r = Reader { data: &data, pos: 0 };
        let version = if data[0..4] != [b'P', b'Z', b'P', b'K'] {
            0
        } else {
            r.pos += 4;
            let v = r.u32()?;
            v
        };

        let page_num = r.u32()?;

        for i in 0..page_num {
            let _page_name = r.bytes_with_len()?;
            let count = r.u32()?;
            let _has_alpha = r.u32()?;

            let mut metas = Vec::with_capacity(count as usize);
            for _ in 0..count {
                let name = std::str::from_utf8(r.bytes_with_len()?)?.to_string();
                metas.push(TexMeta {
                    name,
                    x: r.i32()?,
                    y: r.i32()?,
                    w: r.i32()?,
                    h: r.i32()?,
                    ox: r.i32()?,
                    oy: r.i32()?,
                    ow: r.i32()?,
                    oh: r.i32()?,
                });
            }
            // println!("Pushed {count} texture metas");

            // Page PNG: v1 is length-prefixed; v0 runs until the 0xDEADBEEF magic.
            let png = match version {
                1 => r.bytes_with_len()?,
                0 => r.until(&[0xEF, 0xBE, 0xAD, 0xDE])?,
                v => return Err(Error::new(ErrorKind::InvalidData, format!("unsupported pack version {v}")).into()),
            };
            // println!("loading png with len: {:?}", png.len());
            let page = image::load_from_memory(png)?.to_rgba8();
            // println!("Image loaded.");
            // println!("Loading {} texture sprites", metas.len());
            for m in metas {
                // Crop the (trimmed) sprite rect out of the atlas page.
                let im = imageops::crop_imm(&page, m.x as u32, m.y as u32, m.w as u32, m.h as u32).to_image();
                // Offset relative to the square's bottom-center (pzmap2dzi math).
                let sprite = Sprite {
                    im,
                    ox: m.ox - (m.ow >> 1),
                    oy: m.oy - m.oh,
                };
                self.sprites.insert(m.name, Arc::new(sprite));
            }
        }
        Ok(())
    }
}

/// Composite a multi-cell sprite (e.g. a tree) from named sub-sprites in the
/// library, using each sub-sprite's own offset. Mirrors pzmap2dzi's
/// `blend_textures`: size a canvas to the combined affected area, draw each
/// cell at anchor + its offset, then anchor the result at bottom-center.
pub fn blend_sprite(lib: &TextureLibrary, names: &[&str]) -> Option<Sprite> {
    let subs: Vec<Arc<Sprite>> = names.iter().filter_map(|n| lib.get(n)).collect();
    if subs.is_empty() {
        return None;
    }

    // Extent from the bottom-center in each direction (get_affected_area).
    let (mut l, mut u, mut r, mut b) = (0i32, 0i32, 0i32, 0i32);
    for s in &subs {
        let (w, h) = (s.im.width() as i32, s.im.height() as i32);
        l = l.max((-s.ox).max(0));
        u = u.max((-s.oy).max(0));
        r = r.max((s.ox + w).max(0));
        b = b.max((s.oy + h).max(0));
    }

    let w = (2 * l.max(r)).max(1);
    let (ax, ay) = (w / 2, u); // bottom-center anchor inside the canvas
    let mut canvas = RgbaImage::new(w as u32, (u + b).max(1) as u32);
    for s in &subs {
        imageops::overlay(&mut canvas, &s.im, (ax + s.ox) as i64, (ay + s.oy) as i64);
    }
    Some(Sprite { im: canvas, ox: -ax, oy: -ay })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TEXTURE_PATH;
    use crate::foliage::FOLIAGE_MAP;
    use crate::iso_render::load_sprite;
    use imageproc::compose::overlay;
    use imageproc::drawing::Canvas;

    #[test]
    pub fn test_blending() {
        for (name, textures) in &FOLIAGE_MAP {
            let t = textures.into_iter().map(|s| load_sprite(*s).unwrap()).collect::<Vec<_>>();
            let blend_img = t.iter().fold(RgbaImage::new(t[0].width(), t[0].height()), |mut i, b| overlay(&mut i, b, 0, 0));
            let tex_name = format!("{name}.png");

            blend_img.save(TEXTURE_PATH.join(&tex_name)).unwrap();
            println!("blended {}", tex_name);
        }
    }
}
