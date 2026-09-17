use std::path::{Path, PathBuf};
use zomboid_map::ZResult;

use zomboid_map::iso_render::{N, project, render_iso_viewport};
use zomboid_map::pack::TextureLibrary;
use zomboid_map::top_render::read_lots;

#[tokio::main]
pub async fn main() -> ZResult<()> {
    let path = PathBuf::from("D:\\SteamLibrary\\steamapps\\common\\ProjectZomboid\\media\\maps\\Muldraugh, KY\\");
    let pack = read_lots(path, 27, 32).await?;

    // Load the game's texture packs (walls, fixtures, everything).
    let mut lib = TextureLibrary::new();
    lib.load_dir(Path::new("D:\\SteamLibrary\\steamapps\\common\\ProjectZomboid\\media\\texturepacks"))?;
    eprintln!("loaded {} sprites", lib.len());

    let (cx, cy) = project(N / 2, N / 2, 0);
    dbg!(cx, cy);

    // Center tile + one ring of surrounding tiles: a 3x3 grid of 1024px tiles.
    // Center tile's top-left is (cx - TILE/2, cy - TILE/2); back off one more
    // tile on each axis and triple the extent.
    const TILE: i32 = 1024;
    let cam_x = cx - TILE / 2 - TILE;
    let cam_y = cy - TILE / 2 - TILE;
    let (img, missing) = render_iso_viewport(&pack, &mut lib, cam_x, cam_y, (3 * TILE) as u32, (3 * TILE) as u32)?;
    img.save("iso_tile.png")?;

    for (name, c) in missing.iter().take(40) {
        eprintln!("{name}: {c}");
    }
    Ok(())
}
