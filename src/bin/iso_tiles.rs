use image::imageops;
use std::path::{Path, PathBuf};
use std::time::Instant;
use zomboid_map::ZResult;

use zomboid_map::iso_render::{N, project, render_iso_viewport};
use zomboid_map::pack::TextureLibrary;
use zomboid_map::top_render::read_lots;

#[tokio::main]
pub async fn main() -> ZResult<()> {
    let path = PathBuf::from("D:\\SteamLibrary\\steamapps\\common\\ProjectZomboid\\media\\maps\\Muldraugh, KY\\");
    let texture_path = Path::new("D:\\SteamLibrary\\steamapps\\common\\ProjectZomboid\\media\\texturepacks");
    let start = Instant::now();
    let pack = read_lots(path, 27, 32).await?;
    println!("Loading packs took: {:?}", start.elapsed());
    let mut lib = TextureLibrary::new();
    let start = Instant::now();
    lib.load_dir(texture_path)?;
    println!("Loading textures took: {:?}", start.elapsed());
    eprintln!("loaded {} sprites", lib.len());

    let (cx, cy) = project(N / 2, N / 2, 0);

    const TILE: i32 = 1024;
    let cam_x = cx - TILE / 5 - TILE;
    let cam_y = cy - TILE / 5 - TILE;
    let view_size = (6 * TILE) as u32;
    let start = Instant::now();
    let mut img = render_iso_viewport(&pack, &lib, cam_x, cam_y, view_size, view_size, 0)?;
    let img2 = render_iso_viewport(&pack, &lib, cam_x, cam_y, view_size, view_size, 1)?;
    imageops::overlay(&mut img, &img2, 0, 0);
    println!("Drawing textures took: {:?}", start.elapsed());
    img.save("iso_tile.png")?;

    Ok(())
}
