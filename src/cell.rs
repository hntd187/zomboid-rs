use crate::{LotHeader, ZResult, BLOCK_SIZE_IN_SQUARES, CELL_SIZE_IN_BLOCKS};
use ndarray::prelude::*;
use ndarray::{Ix, OwnedRepr};
use std::io::SeekFrom;
use std::path::PathBuf;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncSeekExt, BufReader};
use tracing::instrument;

const BLOCK_SIZE: usize = 8;
const SQRT_BLOCK_SIZE: usize = 8 * 8;
pub type Blocks = Vec<
    // Block
    Vec<
        // layer
        Vec<
            // row
            Vec<Vec<i32>>,
        >,
    >,
>;

#[derive(Debug)]
pub struct LotPack {
    pub header: LotHeader,
    pub blocks: Blocks,
    pub alt_blocks: ArrayBase<OwnedRepr<heapless::Vec<i32, 16>>, Dim<[Ix; 4]>>,
}

impl LotPack {
    #[instrument(skip(self), ret, fields(idx, idx_mod, yidx, yidx_mod, block))]
    pub fn get_block(&self, bx: usize, by: usize, layer: i32) -> Option<&[i32]> {
        let idx = bx / BLOCK_SIZE_IN_SQUARES;
        let idx_mod = bx % BLOCK_SIZE_IN_SQUARES;
        let yidx = by / BLOCK_SIZE_IN_SQUARES;
        let yidx_mod = by % BLOCK_SIZE_IN_SQUARES;
        let b = idx * CELL_SIZE_IN_BLOCKS + yidx;
        let r = self
            .blocks
            .get(b)
            .and_then(|l| l.get((layer + self.header.min_layer.abs()) as usize))
            .and_then(|r| r.get(idx_mod))
            .and_then(|s| s.get(yidx_mod))?;
        Some(r)
    }

    #[instrument(level = "trace", skip(self), ret)]
    pub fn unsafe_get_block(&self, bx: usize, by: usize, layer: i32) -> Option<&[i32]> {
        let idx = bx / BLOCK_SIZE_IN_SQUARES;
        let idx_mod = bx % BLOCK_SIZE_IN_SQUARES;
        let yidx = by / BLOCK_SIZE_IN_SQUARES;
        let yidx_mod = by % BLOCK_SIZE_IN_SQUARES;
        let b = idx * CELL_SIZE_IN_BLOCKS + yidx;
        let block = &self.blocks[b];
        let layer = &block[(layer + self.header.min_layer.abs()) as usize];
        let row = &layer[idx_mod];
        Some(&row[yidx_mod])
    }
}

// #[instrument]
pub async fn load_lotpack(path: &PathBuf, lot_header: LotHeader) -> ZResult<LotPack> {
    let file = File::open(path).await?;
    let mut reader = BufReader::new(file);
    let mut version_buf = [0; 4];
    reader.read_exact(&mut version_buf).await?;
    assert_eq!(version_buf, [b'L', b'O', b'T', b'P']);
    let _version = reader.read_u32_le().await?;
    let block_num = reader.read_u32_le().await?;

    let mut blocks: Blocks = vec![];
    let starting_pos = reader.stream_position().await?;

    for i in 0..block_num {
        let mut skip = 0;
        let mut block = vec![vec![]; (lot_header.max_layer - lot_header.min_layer) as usize];
        reader
            .seek(SeekFrom::Start(starting_pos + i as u64 * 8))
            .await?;
        let new_pos = reader.read_u32_le().await? as u64;
        reader.seek(SeekFrom::Start(new_pos)).await?;
        for z in 0..(lot_header.max_layer - lot_header.min_layer) as usize {
            if skip >= SQRT_BLOCK_SIZE {
                skip -= SQRT_BLOCK_SIZE;
                continue;
            }
            let mut layer = vec![vec![]; BLOCK_SIZE];
            for x in 0..BLOCK_SIZE {
                if skip >= BLOCK_SIZE {
                    skip -= BLOCK_SIZE;
                    continue;
                }
                let mut row = vec![vec![]; BLOCK_SIZE];
                for y in 0..BLOCK_SIZE {
                    if skip > 0 {
                        skip -= 1;
                        continue;
                    }
                    let count = reader.read_i32_le().await?;
                    if count == -1 {
                        skip = reader.read_i32_le().await? as usize;
                        if skip > 0 {
                            skip -= 1;
                            continue;
                        }
                    }
                    if count <= 1 {
                        continue;
                    }
                    let _room = reader.read_i32_le().await?;
                    for _ in 0..count - 1 {
                        row[y].push(reader.read_i32_le().await?);
                    }
                }
                if row.iter().any(|r| !r.is_empty()) {
                    layer[x] = row;
                }
            }
            if layer.iter().any(|r| !r.is_empty()) {
                block[z] = layer;
            }
        }
        blocks.push(block);
    }

    Ok(LotPack {
        header: lot_header,
        blocks,
        alt_blocks: Array::from_elem((1024, 30, 8, 8), heapless::Vec::new()),
    })
}

pub async fn alt_load_lotpack(path: &PathBuf, lot_header: LotHeader) -> ZResult<LotPack> {
    let file = File::open(path).await?;
    let mut reader = BufReader::new(file);
    let mut version_buf = [0; 4];
    reader.read_exact(&mut version_buf).await?;
    assert_eq!(version_buf, [b'L', b'O', b'T', b'P']);
    let _version = reader.read_u32_le().await?;
    let block_num = reader.read_u32_le().await?;
    let resolved_layer_count = (lot_header.max_layer - lot_header.min_layer) as usize;
    let mut blocks = Array::<heapless::Vec<i32, 16>, _>::from_elem((1024, resolved_layer_count, 8, 8), heapless::Vec::new());
    let starting_pos = reader.stream_position().await?;

    for i in 0..block_num {
        let mut skip = 0;
        reader
            .seek(SeekFrom::Start(starting_pos + i as u64 * 8))
            .await?;
        let new_pos = reader.read_u32_le().await? as u64;
        reader.seek(SeekFrom::Start(new_pos)).await?;
        for z in 0..resolved_layer_count {
            if skip >= SQRT_BLOCK_SIZE {
                // println!("greater than SQRT: {}", skip);
                skip -= SQRT_BLOCK_SIZE;
                continue;
            }

            for x in 0..BLOCK_SIZE {
                if skip >= BLOCK_SIZE {
                    // println!("x greater than block: {}", skip);
                    skip -= BLOCK_SIZE;
                    continue;
                }

                for y in 0..BLOCK_SIZE {
                    if skip > 0 {
                        // println!("y greater than block: {} {} {} {}", z, x, y, skip);
                        skip -= 1;
                        continue;
                    }
                    let count = reader.read_i32_le().await?;
                    if count == -1 {
                        skip = reader.read_i32_le().await? as usize;
                        if skip > 0 {
                            // println!("greater than reader: {}", skip);
                            skip -= 1;
                            continue;
                        }
                    }
                    if count <= 1 {
                        continue;
                    }
                    let _room = reader.read_i32_le().await?;
                    for _ in 0..count - 1 {
                        blocks[[i as usize, z, x, y]].push(reader.read_i32_le().await?).expect("What?");
                    }
                }
            }
        }
    }

    Ok(LotPack {
        header: lot_header,
        blocks: vec![],
        alt_blocks: blocks,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::header::load_lotheader;
    use tokio::time::Instant;
    use tracing::{span, Level};

    #[tokio::test]
    pub async fn test_read() -> ZResult<()> {
        let s = tracing_subscriber::fmt().finish();
        tracing::subscriber::set_global_default(s)
            .map_err(|_err| eprintln!("Unable to set global default subscriber"))
            .unwrap();

        let path = PathBuf::from("D:\\SteamLibrary\\steamapps\\common\\ProjectZomboid\\media\\maps\\Muldraugh, KY\\world_21_48.lotpack");
        let header = PathBuf::from("D:\\SteamLibrary\\steamapps\\common\\ProjectZomboid\\media\\maps\\Muldraugh, KY\\21_48.lotheader");

        let _start = Instant::now();
        let header = load_lotheader(&header).await?;
        let pack = alt_load_lotpack(&path, header).await?;

        let s = span!(Level::INFO, "pack_reading").entered();

        // for (i, b) in pack.blocks.iter().enumerate() {
        //     for (i2, b2) in b.iter().enumerate() {
        //         if !b2.is_empty() {
        //             println!("i={} b={} i2={} b2={}", i, b.len(), i2, b2.len());
        //             for (i3, b3) in b2.iter().enumerate() {
        //                 println!("{:?}", b3);
        //             }
        //         }
        //     }
        // }

        for b in pack.alt_blocks.outer_iter() {
            if !b[[0, 0, 0]].is_empty() {
                println!("{:?}", b);
            }
        }
        println!("{}", pack.alt_blocks.is_standard_layout());
        let _s = s.exit();

        Ok(())
    }
}
