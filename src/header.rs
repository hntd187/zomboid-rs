use crate::{Building, LotHeader, Obj, Rect, Room, ZResult};
use bytes::Bytes;
use std::path::PathBuf;
use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};

#[inline]
pub async fn read_tiles(data: &mut BufReader<File>, num_tiles: usize) -> ZResult<Vec<String>> {
    let mut output = vec![];
    for _ in 0..num_tiles {
        let mut buf = vec![];
        data.read_until(b'\n', &mut buf).await?;
        output.push(std::str::from_utf8(&buf)?.trim().to_string());
    }
    Ok(output)
}

pub async fn load_lotheader(file_path: &PathBuf) -> ZResult<LotHeader> {
    let file = File::open(file_path).await?;
    let mut reader = BufReader::new(file);
    let mut version_buf = [0; 4];
    reader.read_exact(&mut version_buf).await?;

    let version = if version_buf == [b'L', b'O', b'T', b'H'] {
        reader.read_u32_le().await?
    } else {
        u32::from_le_bytes(version_buf.try_into()?)
    };

    let num_tiles = reader.read_u32_le().await?;
    let tiles = read_tiles(&mut reader, num_tiles as usize).await?;

    let width = reader.read_u32_le().await?;
    let height = reader.read_u32_le().await?;
    let min_layer = reader.read_i32_le().await?;
    let max_layer = reader.read_i32_le().await? + 1;
    let room_number = reader.read_u32_le().await?;
    let mut rooms: Vec<Room> = Vec::with_capacity(room_number as usize);

    for id in 0..room_number {
        let mut room_name_buf = vec![];
        reader.read_until(b'\n', &mut room_name_buf).await?;
        let name = std::str::from_utf8(&room_name_buf)?.trim().to_string();
        let layer = reader.read_i32_le().await?;
        let num_rects = reader.read_u32_le().await?;
        let mut rects = Vec::with_capacity(num_rects as usize);
        let (mut area, mut xmin, mut xmax, mut ymin, mut ymax) = (0, 0, 0, 0, 0);
        for _ in 0..num_rects {
            let x = reader.read_i32_le().await?;
            let y = reader.read_i32_le().await?;
            let w = reader.read_i32_le().await?;
            let h = reader.read_i32_le().await?;
            area += w * h;
            rects.push(Rect(x, y, w, h));
            xmin = xmin.min(x);
            ymin = ymin.min(y);
            xmax = xmax.max(x + w);
            ymax = ymax.max(y + h);
        }

        let obj_num = reader.read_i32_le().await?;
        let mut objects = Vec::with_capacity(obj_num as usize);
        for _ in 0..obj_num {
            let obj_type = reader.read_i32_le().await?;
            let x = reader.read_i32_le().await?;
            let y = reader.read_i32_le().await?;
            objects.push(Obj(obj_type, x, y));
        }
        rooms.push(Room {
            id,
            name,
            layer,
            area,
            rects,
            objects,
            xmin,
            ymin,
            xmax,
            ymax,
        })
    }

    let building_num = reader.read_u32_le().await?;
    let mut buildings = Vec::with_capacity(building_num as usize);
    for id in 0..building_num {
        let room_num = reader.read_u32_le().await?;
        let mut rooms = Vec::with_capacity(room_num as usize);
        for _ in 0..room_num {
            rooms.push(reader.read_u32_le().await?);
        }
        buildings.push(Building { id, rooms })
    }
    let mut zpop = Vec::with_capacity(32);
    for _ in 0..32 {
        let mut bytes = [0u8; 32];
        reader.read_exact(&mut bytes).await?;
        zpop.push(Bytes::from_owner(bytes));
    }

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
