//! Render an entire Project Zomboid map to an isometric DeepZoom (DZI) tile
//! pyramid, in the same spirit as pzmap2dzi.
//!
//! Pipeline:
//!   1. Scan the map directory for `world_{x}_{y}.lotpack` cells and compute the
//!      global isometric pixel bounds ([`World`]).
//!   2. Render every base-level tile; each tile independently composites the
//!      cells that overlap it, so there is no shared mutable canvas — only the
//!      (synchronised) cell cache and immutable textures.
//!   3. Build every coarser pyramid level by 2×2 downscaling the level above.
//!   4. Emit the `<name>.dzi` descriptor.
//!
//! Concurrency mirrors the other binaries: CPU-bound work runs on
//! `spawn_blocking`, throttled by a `Semaphore` so at most `--threads` tiles
//! render at once, and results are awaited with `join_all`.
//!
//! Example:
//!   iso-map \
//!     --map "/path/to/ProjectZomboid/media/maps/Muldraugh, KY" \
//!     --textures "/path/to/ProjectZomboid/media/texturepacks" \
//!     --out ./out --name muldraugh --tile-size 1024 --max-floor 7

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use clap::Parser;
use tokio::sync::Semaphore;

use zomboid_map::dzi::{Dzi, downscale_tile};
use zomboid_map::pack::TextureLibrary;
use zomboid_map::world::World;
use zomboid_map::{ZError, ZResult};

#[derive(Parser, Debug)]
#[command(about = "Render a PZ map to an isometric DZI tile pyramid")]
struct Args {
    /// Map cell directory (contains `world_{x}_{y}.lotpack` / `{x}_{y}.lotheader`).
    #[arg(short, long)]
    map: PathBuf,
    /// Directory of `.pack` texture packs (e.g. `media/texturepacks`).
    #[arg(short, long)]
    textures: PathBuf,
    /// Output directory for the DZI descriptor and tile tree.
    #[arg(short, long, default_value = "out")]
    out: PathBuf,
    /// Base name for `<name>.dzi` and `<name>_files/`.
    #[arg(short, long, default_value = "map")]
    name: String,
    /// Tile edge length in pixels.
    #[arg(long, default_value_t = 1024)]
    tile_size: u32,
    /// Lowest floor (z-layer) to render, inclusive. Use a negative value for basements.
    #[arg(long, default_value_t = 0)]
    min_floor: i32,
    /// Highest floor (z-layer) to render, inclusive.
    #[arg(long, default_value_t = 7)]
    max_floor: i32,
    /// Max cells kept resident in the LRU cache.
    #[arg(long, default_value_t = 64)]
    cache_cells: usize,
    /// Concurrent tile renders (defaults to all logical cores).
    #[arg(long)]
    threads: Option<usize>,
}

/// Render one level's tiles: spawn a throttled `spawn_blocking` job per tile and
/// await them all. `f` does the CPU work for a `(col, row)` and reports whether a
/// (non-empty) tile was written. Returns the number of tiles written.
async fn run_level<F>(label: &str, coords: Vec<(u32, u32)>, concurrency: usize, f: Arc<F>) -> ZResult<usize>
where
    F: Fn(u32, u32) -> ZResult<bool> + Send + Sync + 'static,
{
    let total = coords.len();
    let sem = Arc::new(Semaphore::new(concurrency.max(1)));
    let written = Arc::new(AtomicUsize::new(0));
    let done = Arc::new(AtomicUsize::new(0));
    let label = label.to_string();

    let mut tasks = Vec::with_capacity(total);
    for (col, row) in coords {
        // Acquire before spawning so at most `concurrency` tiles are in flight.
        let permit = Arc::clone(&sem).acquire_owned().await.expect("semaphore closed");
        let f = Arc::clone(&f);
        let written = Arc::clone(&written);
        let done = Arc::clone(&done);
        let label = label.clone();
        tasks.push(tokio::spawn(async move {
            let _permit = permit; // released on task completion
            let wrote = tokio::task::spawn_blocking(move || f(col, row)).await.expect("render task panicked")?;
            if wrote {
                written.fetch_add(1, Ordering::Relaxed);
            }
            let n = done.fetch_add(1, Ordering::Relaxed) + 1;
            if n % 64 == 0 || n == total {
                println!("  {label} {n}/{total} ({} written)", written.load(Ordering::Relaxed));
            }
            Ok::<(), ZError>(())
        }));
    }

    for r in futures::future::join_all(tasks).await {
        r.expect("join failed")?;
    }
    Ok(written.load(Ordering::Relaxed))
}

#[tokio::main]
async fn main() -> ZResult<()> {
    let args = Args::parse();

    let concurrency = args
        .threads
        .unwrap_or_else(|| std::thread::available_parallelism().map(|n| n.get()).unwrap_or(8));

    assert!(args.min_floor <= args.max_floor, "--min-floor must be <= --max-floor");
    let layers: Vec<i32> = (args.min_floor..=args.max_floor).collect();

    // Textures are immutable once loaded and shared across all tile renders.
    let mut lib = TextureLibrary::new();
    let t = Instant::now();
    lib.load_dir(&args.textures)?;
    println!("Loaded {} sprites in {:?}", lib.len(), t.elapsed());
    let lib = Arc::new(lib);

    let t = Instant::now();
    let world = Arc::new(World::scan(&args.map, layers, args.cache_cells)?);
    let (min_cell, max_cell) = world.cell_bounds();
    println!(
        "Scanned {} cells [{:?}..={:?}] -> {}x{} px in {:?}",
        world.cell_count(),
        min_cell,
        max_cell,
        world.width(),
        world.height(),
        t.elapsed()
    );

    let dzi = Dzi::new(world.width(), world.height(), args.tile_size);
    let files_dir = args.out.join(format!("{}_files", args.name));
    std::fs::create_dir_all(&files_dir)?;

    // ---- Base level: render from the world. ----
    let (cols, rows) = dzi.level_grid(dzi.max_level);
    let base_dir = files_dir.join(dzi.max_level.to_string());
    std::fs::create_dir_all(&base_dir)?;
    println!("Rendering base level {} ({}x{} = {} tiles)...", dzi.max_level, cols, rows, cols as usize * rows as usize);

    let base_coords: Vec<(u32, u32)> = (0..rows).flat_map(|r| (0..cols).map(move |c| (c, r))).collect();
    let t = Instant::now();
    let base_f = {
        let world = Arc::clone(&world);
        let lib = Arc::clone(&lib);
        let base_dir = base_dir.clone();
        let tile_size = dzi.tile_size;
        Arc::new(move |col: u32, row: u32| -> ZResult<bool> {
            match world.render_tile(&lib, col, row, tile_size) {
                Some(img) => {
                    img.save(base_dir.join(format!("{col}_{row}.png")))?;
                    Ok(true)
                }
                None => Ok(false),
            }
        })
    };
    let written = run_level("base", base_coords, concurrency, base_f).await?;
    println!("Base level done in {:?} ({} non-empty tiles)", t.elapsed(), written);

    // ---- Coarser levels: downscale the level above. ----
    for level in (0..dzi.max_level).rev() {
        let (lc, lr) = dzi.level_grid(level);
        let level_dir = files_dir.join(level.to_string());
        std::fs::create_dir_all(&level_dir)?;
        let coords: Vec<(u32, u32)> = (0..lr).flat_map(|r| (0..lc).map(move |c| (c, r))).collect();

        let t = Instant::now();
        let f = {
            let files_dir = files_dir.clone();
            let level_dir = level_dir.clone();
            Arc::new(move |col: u32, row: u32| -> ZResult<bool> {
                match downscale_tile(&files_dir, &dzi, level, col, row)? {
                    Some(img) => {
                        img.save(level_dir.join(format!("{col}_{row}.png")))?;
                        Ok(true)
                    }
                    None => Ok(false),
                }
            })
        };
        run_level(&format!("level {level}"), coords, concurrency, f).await?;
        println!("Level {level} ({lc}x{lr}) done in {:?}", t.elapsed());
    }

    // ---- Descriptor. ----
    let dzi_path = args.out.join(format!("{}.dzi", args.name));
    std::fs::write(&dzi_path, dzi.descriptor_xml())?;
    println!("Wrote {}", dzi_path.display());

    Ok(())
}
