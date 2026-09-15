use crate::cell::LotPack;
use crate::{ImageRef, Rect, Room, ZResult};
use image::Rgba;
use imageproc::drawing::draw_antialiased_line_segment_mut;
use imageproc::pixelops::interpolate;
use imageproc::point::Point;
use phf::phf_map;

static ROOM_COLORS: phf::Map<&str, Rgba<u8>> = phf_map! {
    "farmstorage" => Rgba([34, 195, 176, 255]),
    "foyer" => Rgba([195, 34, 171, 255]),
    "bathroom" => Rgba([158, 34, 195, 255]),
    "bedroom" => Rgba([255, 255, 0, 255]),
    "livingroom" => Rgba([0, 255, 255, 255]),
    "kitchen" => Rgba([0, 0, 0, 255]),
    "closet" => Rgba([0, 255, 0, 255]),
    "laundry" => Rgba([0, 255, 0, 255]),
    "hall" => Rgba([255, 0, 0, 255]),
    "garagestorage" => Rgba([0, 255, 0, 255]),
    "derelict" => Rgba([0, 255, 0, 255]),
    "office" => Rgba([0, 255, 0, 255]),
    "shed" => Rgba([195, 195, 34, 255]),
};

pub fn project_point(x: f32, y: f32, global_min_x: f32, global_min_y: f32, global_max_y: f32, scale_w: f32, scale_h: f32) -> Point<i32> {
    let iso_x = (x - y) * scale_w;
    let iso_y = (x + y) * scale_h;
    let offset_x = -(global_min_x - global_max_y) * scale_w;
    let offset_y = -(global_min_x + global_min_y) * scale_h;

    Point::new((iso_x + offset_x) as i32, (iso_y + offset_y) as i32)
}

pub fn draw_rooms(img: ImageRef, pack: LotPack, x_offset: u32, y_offset: u32, room_layer: i32) -> ZResult<()> {
    let mut global_xmin = f32::MAX;
    let mut global_ymin = f32::MAX;
    let mut global_xmax = f32::MIN;
    let mut global_ymax = f32::MIN;

    for room in &pack.header.rooms {
        if room.layer == room_layer {
            global_xmin = global_xmin.min(room.xmin as f32);
            global_ymin = global_ymin.min(room.ymin as f32);
            global_xmax = global_xmax.max(room.xmax as f32);
            global_ymax = global_ymax.max(room.ymax as f32);
        }
    }

    let total_width_units = global_xmax - global_xmin;
    let scale_w = if total_width_units > 0.0 { 512.0 / total_width_units } else { 16.0 };
    let scale_h = scale_w / 2.0;
    println!("Scale X: {scale_w} Scale H: {scale_h}");
    for room in pack.header.rooms {
        let Room { name, layer, rects, .. } = room;
        if layer != room_layer {
            continue;
        }

        for rect in rects {
            let Rect(x, y, w, h) = rect;

            let x = x as f32;
            let y = y as f32;
            let w = w as f32;
            let h = h as f32;

            let p1 = project_point(x, y, global_xmin, global_ymin, global_ymax, scale_w, scale_h);
            let p2 = project_point(x, y + h, global_xmin, global_ymin, global_ymax, scale_w, scale_h);
            let p3 = project_point(x + w, y, global_xmin, global_ymin, global_ymax, scale_w, scale_h);
            let p4 = project_point(x + w, y + h, global_xmin, global_ymin, global_ymax, scale_w, scale_h);

            // Point::new(x + w, y + h);
            let center = Point::new(x + (w / 2.0), y + (h / 2.0));
            if name == "farmstorage" {
                println!("{name} {:?} {:?} {:?} {:?}", p1, p2, p3, p4);
            }

            let color = *ROOM_COLORS.get(&name).unwrap_or(&Rgba([0, 255, 0, 255]));
            unsafe {
                let mut handout = img.request_handout(0, 0);

                let mut si = handout.subimage(0, 0, 1024, 1024);
                draw_antialiased_line_segment_mut(si.inner_mut(), (p1.x, p1.y), (p2.x, p2.y), color, interpolate);
                draw_antialiased_line_segment_mut(si.inner_mut(), (p2.x, p2.y), (p4.x, p4.y), color, interpolate);
                draw_antialiased_line_segment_mut(si.inner_mut(), (p4.x, p4.y), (p3.x, p3.y), color, interpolate);
                draw_antialiased_line_segment_mut(si.inner_mut(), (p3.x, p3.y), (p1.x, p1.y), color, interpolate);
                // draw_antialiased_polygon_mut(si.inner_mut(), &[p1, p2, p4, p3], color, interpolate);
                // draw_text_mut(si.inner_mut(), color, p1.x as i32, p1.y as i32, 10.0, &font, "1");
                // draw_text_mut(si.inner_mut(), color, p2.x as i32, p2.y as i32, 10.0, &font, "2");
                // draw_text_mut(si.inner_mut(), color, p3.x as i32, p3.y as i32, 10.0, &font, "3");
                // draw_text_mut(si.inner_mut(), color, p4.x as i32, p4.y as i32, 10.0, &font, "4");
                // draw_hollow_polygon_mut(
                //     si.inner_mut(),
                //     &[
                //         center - Point::new(1.0, 1.0),
                //         center + Point::new(1.0, 0.0),
                //         center + Point::new(1.0, 1.0),
                //         center + Point::new(0.0, 1.0),
                //     ],
                //     color,
                // );
                // draw_text_mut(si.inner_mut(), color, center.x as i32, center.y as i32, 10.0, &font, "C");
                // draw_hollow_rect_mut(si.inner_mut(), r, Rgba([0, 255, 0, 255]))
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::ImageRef;
    use crate::handout::{ImageCell, Img};
    use crate::rooms::draw_rooms;
    use crate::top_render::read_lots;
    use std::error::Error;
    use std::path::PathBuf;
    use std::sync::Arc;

    #[tokio::test]
    pub async fn test_read() -> Result<(), Box<dyn Error>> {
        let path = PathBuf::from("D:\\SteamLibrary\\steamapps\\common\\ProjectZomboid\\media\\maps\\Muldraugh, KY\\");
        let img: ImageRef = Arc::new(ImageCell::new(Img::new(1024, 1024)));
        let pack = read_lots(path, 27, 32).await?;

        // for r in &pack.header.rooms {
        //     println!("{:?}", r);
        // }
        draw_rooms(Arc::clone(&img), pack, 0, 0, 0)?;

        img.save("room_tiles.png")?;

        Ok(())
    }
}
