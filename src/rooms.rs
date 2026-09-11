use crate::cell::LotPack;
use crate::{ImageRef, Rect, Room, ZResult};
use image::Rgba;
use imageproc::drawing::draw_hollow_polygon_mut;
use imageproc::rect;
use nalgebra::*;

pub fn draw_rooms(
    img: ImageRef,
    pack: LotPack,
    x_offset: u32,
    y_offset: u32,
    room_layer: i32,
) -> ZResult<()> {
    for room in pack.header.rooms {
        let Room {
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
        } = room;
        if layer != room_layer {
            continue;
        }
        for rect in rects {
            println!("{:?}", rect);
            let Rect(x, y, w, h) = rect;

            let x = (x * 4) as f64;
            let y = (y * 4) as f64;
            let w = (w * 4) as f64;
            let h = (h * 4) as f64;
            let wc = w / 2.0;
            let hc = h / 2.0;
            let rot = Isometry2::new(Vector2::new(0.5, 0.5), 1.2);
            // println!("{}", 2.0.sqrt() / 2.0);
            let p1 = rot.transform_point(&Point2::new(x, y));
            let p2 = rot.transform_point(&Point2::new(x, y + h));
            let p3 = rot.transform_point(&Point2::new(x + w, y));
            let p4 = rot.transform_point(&Point2::new(x + w, y + h));
            // println!("{:?} {:?} {:?} {:?}", p1, p2, p3, p4);
            let r = rect::Rect::at(x as i32, y as i32).of_size(w as u32, h as u32);

            unsafe {
                let mut handout = img.request_handout(0, 0);

                // let mut si = handout.subimage(x1 as u32 + x_offset, y1 as u32 + y_offset, x2 as u32, y2 as u32);
                let mut si = handout.subimage(0, 0, 1024, 1024);
                draw_hollow_polygon_mut(
                    si.inner_mut(),
                    &[
                        imageproc::point::Point::new(188.0, 108.0),
                        imageproc::point::Point::new(252.0, 76.0),
                        imageproc::point::Point::new(316.0, 108.0),
                        imageproc::point::Point::new(252.0, 140.0),
                    ],
                    Rgba([0, 255, 0, 255]),
                );
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
        let path = PathBuf::from(
            "D:\\SteamLibrary\\steamapps\\common\\ProjectZomboid\\media\\maps\\Muldraugh, KY\\",
        );
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
