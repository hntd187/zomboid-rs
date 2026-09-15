use rand::prelude::*;
use std::path::PathBuf;
use tokio::task::JoinSet;
use tokio::time::Instant;
use zomboid_map::ZResult;
use zomboid_map::top_render::{ACCESS_COUNTS, TEXTURE_CACHE, render_top};

pub const NUM_TILES: usize = 24;
#[tokio::main]
pub async fn main() -> ZResult<()> {
    let path = PathBuf::from("D:\\SteamLibrary\\steamapps\\common\\ProjectZomboid\\media\\maps\\Muldraugh, KY\\");

    let start = Instant::now();
    let mut rng = rand::rng();

    // let mut tasks = JoinSet::new();
    println!("Spawning {} random tile generation...", NUM_TILES);
    for _ in 0..=NUM_TILES {
        let x: usize = rng.random_range(0..=61);
        let y: usize = if x > 45 { rng.random_range(3..=61) } else { rng.random_range(19..=61) };
        println!("Chose: ({},{})", x, y);
        // tasks.spawn(render_top(path.clone(), x, y, 0));
    }

    // while let Some(Ok(Ok((x, y, img)))) = tasks.join_next().await {
    //     let output = format!("./tiles/tile_{}_{}.png", x, y);
    //     img.save(output)?
    // }

    ACCESS_COUNTS.iter().filter(|e| *e.value() > 1000).for_each(|e| {
        if let Some(t) = TEXTURE_CACHE.get(e.key()) {
            println!("\"{}\" => f32x4::new({:?}), ", e.key(), t.to_array())
        }
    });
    println!("Took: {:?}", start.elapsed());
    Ok(())
}
