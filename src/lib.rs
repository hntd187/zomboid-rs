// #![allow(unused)]

use crate::handout::ImageCell;
use bytes::Bytes;
use image::{ImageBuffer, Rgba};
use std::convert::Infallible;
use std::path::PathBuf;
use std::sync::{Arc, LazyLock};
use thiserror::Error;
use tracing::dispatcher::SetGlobalDefaultError;

static TEXTURE_PATH: LazyLock<PathBuf> = LazyLock::new(|| PathBuf::from("D:\\pzmap\\texture\\default"));
const CELL_SIZE_IN_BLOCKS: usize = 32;
const BLOCK_SIZE_IN_SQUARES: usize = 8;

pub mod cell;
pub mod foliage;
pub mod handout;
pub mod header;
pub mod iso_render;
pub mod pack;
pub mod render_backend;
pub mod rooms;
pub mod textures;
pub mod top_render;

pub type ImageRef = Arc<ImageCell<Rgba<u8>, ImageBuffer<Rgba<u8>, Vec<u8>>>>;

#[derive(Error, Debug)]
pub enum ZError {
    #[error("Io Error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Utf8 decoding error: {0}")]
    Utf8(#[from] std::str::Utf8Error),
    #[error("Cannot convert slice: {0}")]
    Slice(#[from] std::array::TryFromSliceError),
    #[error("Image error: {0}")]
    Image(#[from] image::ImageError),
    #[error("Infallible")]
    Infallible(#[from] Infallible),
    #[error("Global Default Error: {0}")]
    GlobalDefault(#[from] SetGlobalDefaultError),
}

pub type ZResult<T> = Result<T, ZError>;

#[derive(Default, Debug, Clone)]
pub struct LotHeader {
    version: u32,
    width: u32,
    height: u32,
    max_layer: i32,
    min_layer: i32,
    tiles: Vec<String>,
    rooms: Vec<Room>,
    buildings: Vec<Building>,
    zpop: Vec<Bytes>,
}

#[derive(Default, Debug, Clone)]
pub struct Building {
    id: u32,
    rooms: Vec<u32>,
}

#[derive(Default, Debug, Clone)]
pub struct Room {
    id: usize,
    name: String,
    layer: i32,
    area: i32,
    rects: Vec<Rect>,
    objects: Vec<Obj>,
    xmin: i32,
    ymin: i32,
    xmax: i32,
    ymax: i32,
}

#[derive(Default, Debug, Copy, Clone)]
pub struct Obj(i32, i32, i32);

#[derive(Default, Debug, Copy, Clone)]
pub struct Rect(i32, i32, i32, i32);

#[cfg(test)]
mod tests {
    use crate::header::LotHeaderReader;
    use std::collections::HashMap;
    use std::error::Error;
    use std::path::PathBuf;
    use tokio::time::Instant;

    #[tokio::test]
    pub async fn test_read() -> Result<(), Box<dyn Error>> {
        let path = PathBuf::from("D:\\SteamLibrary\\steamapps\\common\\ProjectZomboid\\media\\maps\\Muldraugh, KY\\49_6.lotheader");
        let start = Instant::now();
        let mut lot_reader = LotHeaderReader::new();
        let header = lot_reader.load_lotheader(&path).await?;
        println!("Time taken: {:?}", start.elapsed());
        println!("header: {}", header.version);
        println!("dimension: {}x{}", header.width, header.height);
        println!("layer: [{}, {})", header.min_layer, header.max_layer);
        println!("tiles: {}", header.tiles.len());
        println!("rooms: {}", header.rooms.len());
        println!("buildings: {}", header.buildings.len());
        println!();
        let mut room_counts = HashMap::new();
        for r in header.rooms {
            *room_counts.entry(r.name).or_insert(0) += 1;
        }
        for (k, v) in room_counts {
            println!("{}: {}", k, v)
        }
        Ok(())
    }
}
