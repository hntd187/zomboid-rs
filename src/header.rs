use crate::{Building, LotHeader, Obj, Rect, Room, ZResult};
use bytes::Bytes;
use std::path::PathBuf;

#[derive(Debug, Copy, Clone)]
pub struct LotHeaderReader {
    offset: usize,
}

impl LotHeaderReader {
    pub fn new() -> Self {
        Self { offset: 4 } // We read the first 12 bytes directly in offsets, so we start here
    }

    fn advance_offset(&mut self, offset: usize) {
        self.offset += offset;
    }

    fn maybe_string(&mut self, data: &mut [u8]) -> Option<String> {
        let idx = memchr::memchr(b'\n', &data[self.offset..])?;
        let tile = std::str::from_utf8(&data[self.offset..self.offset + idx]).ok()?;
        self.advance_offset(idx + 1);
        Some(tile.trim().to_string())
    }

    fn read_u32(&mut self, data: &mut [u8]) -> ZResult<u32> {
        let result = u32::from_le_bytes(data[self.offset..self.offset + 4].try_into()?);
        self.advance_offset(4);
        Ok(result)
    }

    fn read_i32(&mut self, data: &mut [u8]) -> ZResult<i32> {
        let result = i32::from_le_bytes(data[self.offset..self.offset + 4].try_into()?);
        self.advance_offset(4);
        Ok(result)
    }

    #[inline]
    fn read_tiles(&mut self, data: &mut [u8], num_tiles: usize) -> ZResult<Vec<String>> {
        let mut output = Vec::with_capacity(num_tiles);
        for _ in 0..num_tiles {
            if let Some(tile_name) = self.maybe_string(data) {
                output.push(tile_name);
            } else {
                break;
            }
        }
        Ok(output)
    }
    fn read_rooms(&mut self, data: &mut [u8], room_number: usize) -> ZResult<Vec<Room>> {
        let mut rooms: Vec<Room> = vec![Room::default(); room_number];
        for id in 0..room_number {
            if let Some(name) = self.maybe_string(data) {
                let layer = self.read_i32(data)?;
                let num_rects = self.read_u32(data)?;
                let mut rects = vec![Rect::default(); num_rects as usize];

                for r in 0..num_rects as usize {
                    let x = self.read_i32(data)?;
                    let y = self.read_i32(data)?;
                    let w = self.read_i32(data)?;
                    let h = self.read_i32(data)?;
                    rects[r].0 = x;
                    rects[r].1 = y;
                    rects[r].2 = w;
                    rects[r].3 = h;
                    rooms[id].area += w * h;
                    rooms[id].xmin = rooms[id].xmin.min(x);
                    rooms[id].ymin = rooms[id].ymin.min(y);
                    rooms[id].xmax = rooms[id].xmax.max(x + w);
                    rooms[id].ymax = rooms[id].ymax.max(y + h);
                }

                let obj_num = self.read_i32(data)?;
                let mut objects = vec![Obj::default(); obj_num as usize];
                for i in 0..obj_num as usize {
                    objects[i].0 = self.read_i32(data)?;
                    objects[i].1 = self.read_i32(data)?;
                    objects[i].2 = self.read_i32(data)?;
                }
                rooms[id].id = id;
                rooms[id].name = name;
                rooms[id].layer = layer;
                rooms[id].rects = rects;
                rooms[id].objects = objects;
            }
        }
        Ok(rooms)
    }

    /// Read a lotheader from disk using async IO (requires a tokio runtime).
    pub async fn load_lotheader(&mut self, file_path: &PathBuf) -> ZResult<LotHeader> {
        let mut file_bytes = tokio::fs::read(file_path).await?;
        self.parse(&mut file_bytes)
    }

    /// Read a lotheader from disk using blocking IO. Safe to call from a plain
    /// worker thread (rayon / `spawn_blocking`) with no tokio runtime present.
    pub fn load_lotheader_sync(&mut self, file_path: &PathBuf) -> ZResult<LotHeader> {
        let mut file_bytes = std::fs::read(file_path)?;
        self.parse(&mut file_bytes)
    }

    fn parse(&mut self, file_bytes: &mut [u8]) -> ZResult<LotHeader> {
        let version = if file_bytes[0..4] == [b'L', b'O', b'T', b'H'] {
            self.read_u32(&mut *file_bytes)?
        } else {
            u32::from_le_bytes(file_bytes[0..4].try_into()?)
        };

        let num_tiles = self.read_u32(&mut *file_bytes)?;
        let tiles = self.read_tiles(&mut *file_bytes, num_tiles as usize)?;

        let width = self.read_u32(&mut *file_bytes)?;
        let height = self.read_u32(&mut *file_bytes)?;
        let min_layer = self.read_i32(&mut *file_bytes)?;
        let max_layer = self.read_i32(&mut *file_bytes)? + 1;

        let room_number = self.read_u32(&mut *file_bytes)?;
        let rooms = self.read_rooms(&mut *file_bytes, room_number as usize)?;
        let building_num = self.read_u32(&mut *file_bytes)?;
        let mut buildings = vec![Building::default(); building_num as usize];
        for id in 0..building_num as usize {
            let room_num = self.read_u32(&mut *file_bytes)?;
            let mut rooms = vec![0u32; room_num as usize];
            for rn in 0..room_num as usize {
                rooms[rn] = self.read_u32(&mut *file_bytes)?;
            }
            buildings[id].id = id as u32;
            buildings[id].rooms = rooms;
        }

        let (zchunks, []) = file_bytes[self.offset..self.offset + (32 * 32)].as_chunks::<32>() else {
            unreachable!()
        };
        let zpop = zchunks.into_iter().map(|b| Bytes::copy_from_slice(b.as_ref())).collect();
        self.offset = 4;
        Ok(LotHeader {
            version,
            width,
            height,
            max_layer,
            min_layer,
            tiles,
            rooms,
            buildings,
            zpop,
        })
    }
}
