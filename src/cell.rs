use crate::{BLOCK_SIZE_IN_SQUARES, CELL_SIZE_IN_BLOCKS, LotHeader, ZResult};
use ndarray::prelude::*;
use ndarray::{Ix, OwnedRepr};
use std::path::PathBuf;

const BLOCK_SIZE: usize = 8;
const SQRT_BLOCK_SIZE: usize = 8 * 8;
pub type Blocks = ArrayBase<OwnedRepr<heapless::Vec<i32, 16>>, Dim<[Ix; 4]>>;

#[derive(Debug)]
pub struct LotPack {
    pub header: LotHeader,
    pub blocks: Blocks,
}

impl LotPack {
    /// `[min_layer, max_layer)` — the z-levels this cell actually stores. Layers
    /// outside this range must not be passed to `get_block`.
    pub fn layer_range(&self) -> (i32, i32) {
        (self.header.min_layer, self.header.max_layer)
    }

    /// True if `layer` is a valid z-level for this cell.
    pub fn has_layer(&self, layer: i32) -> bool {
        layer >= self.header.min_layer && layer < self.header.max_layer
    }

    pub fn get_block(&self, bx: usize, by: usize, layer: i32) -> &[i32] {
        let idx = bx / BLOCK_SIZE_IN_SQUARES;
        let idx_mod = bx % BLOCK_SIZE_IN_SQUARES;
        let yidx = by / BLOCK_SIZE_IN_SQUARES;
        let yidx_mod = by % BLOCK_SIZE_IN_SQUARES;
        let b = idx * CELL_SIZE_IN_BLOCKS + yidx;
        unsafe { self.blocks.uget([b, (layer + self.header.min_layer.abs()) as usize, idx_mod, yidx_mod]) }.as_slice()
    }
}

#[derive(Debug, Copy, Clone)]
pub struct LotPackReader {
    offset: usize,
}

impl LotPackReader {
    pub fn new() -> Self {
        Self { offset: 4 } // We read the first 4 bytes directly in offsets, so we start here
    }

    fn advance_offset(&mut self, offset: usize) {
        self.offset += offset;
    }

    fn read_u32(&mut self, data: &[u8]) -> ZResult<u32> {
        let result = u32::from_le_bytes(data[self.offset..self.offset + 4].try_into()?);
        self.advance_offset(4);
        Ok(result)
    }

    fn read_i32(&mut self, data: &[u8]) -> ZResult<i32> {
        let result = i32::from_le_bytes(data[self.offset..self.offset + 4].try_into()?);
        self.advance_offset(4);
        Ok(result)
    }
    /// Read a lotpack from disk using async IO (requires a tokio runtime).
    pub async fn load_lotpack(&mut self, path: &PathBuf, lot_header: LotHeader) -> ZResult<LotPack> {
        let file_bytes = tokio::fs::read(path).await?;
        self.parse(&file_bytes, lot_header)
    }

    /// Read a lotpack from disk using blocking IO. Safe to call from a plain
    /// worker thread (rayon / `spawn_blocking`) with no tokio runtime present.
    pub fn load_lotpack_sync(&mut self, path: &PathBuf, lot_header: LotHeader) -> ZResult<LotPack> {
        let file_bytes = std::fs::read(path)?;
        self.parse(&file_bytes, lot_header)
    }

    fn parse(&mut self, file_bytes: &[u8], lot_header: LotHeader) -> ZResult<LotPack> {
        assert_eq!(file_bytes[0..4], [b'L', b'O', b'T', b'P']);
        let _version = self.read_u32(file_bytes)?;
        let block_num = self.read_u32(file_bytes)?;
        let resolved_layer_count = (lot_header.max_layer - lot_header.min_layer) as usize;

        let mut blocks = Array::from_elem((1024, resolved_layer_count, 8, 8), heapless::Vec::new());

        for i in 0..block_num {
            let mut skip = 0;
            self.offset = (12 + i * 8) as usize;
            self.offset = self.read_u32(file_bytes)? as usize;

            for z in 0..resolved_layer_count {
                if skip >= SQRT_BLOCK_SIZE {
                    skip -= SQRT_BLOCK_SIZE;
                    continue;
                }
                for x in 0..BLOCK_SIZE {
                    if skip >= BLOCK_SIZE {
                        skip -= BLOCK_SIZE;
                        continue;
                    }
                    for y in 0..BLOCK_SIZE {
                        if skip > 0 {
                            skip -= 1;
                            continue;
                        }
                        let count = self.read_i32(file_bytes)?;
                        if count == -1 {
                            skip = self.read_i32(file_bytes)? as usize;
                            if skip > 0 {
                                skip -= 1;
                                continue;
                            }
                        }
                        if count <= 1 {
                            continue;
                        }
                        let _room = self.read_i32(file_bytes)?;
                        for _ in 0..count - 1 {
                            blocks[[i as usize, z, x, y]]
                                .push(self.read_i32(file_bytes)?)
                                .expect("Block out of range, this shouldn't happen");
                        }
                    }
                }
            }
        }
        self.offset = 0;
        Ok(LotPack { header: lot_header, blocks })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::header::LotHeaderReader;
    use tokio::time::Instant;

    #[tokio::test]
    pub async fn test_read() -> ZResult<()> {
        let path = PathBuf::from("D:\\SteamLibrary\\steamapps\\common\\ProjectZomboid\\media\\maps\\Muldraugh, KY\\world_21_48.lotpack");
        let header = PathBuf::from("D:\\SteamLibrary\\steamapps\\common\\ProjectZomboid\\media\\maps\\Muldraugh, KY\\21_48.lotheader");
        let mut header_reader = LotHeaderReader::new();
        let header = header_reader.load_lotheader(&header).await?;
        let mut reader = LotPackReader::new();
        let _start = Instant::now();
        let new_pack = reader.load_lotpack(&path, header.clone()).await?;
        println!("Elapsed time: {:?}", _start.elapsed());

        Ok(())
    }
}
