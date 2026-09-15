use clap::Parser;
use std::path::PathBuf;
use std::sync::Arc;
use zomboid_map::ZResult;
use zomboid_map::handout::*;
use zomboid_map::top_render::{read_lots, render_top};

#[derive(Parser, Debug)]
pub struct Args {
    #[arg(short, long)]
    map: PathBuf,
    #[arg(short, long)]
    x: usize,
    #[arg(short, long)]
    y: usize,
    #[arg(short, long)]
    layer: i32,
}

#[tokio::main]
async fn main() -> ZResult<()> {
    let Args { map, x, y, layer } = Args::parse();
    let img = Arc::new(ImageCell::new(Img::new(256, 256)));
    let pack = read_lots(map, x, y).await?;
    render_top(Arc::clone(&img), pack, 0, 0, layer)?;
    Ok(img.save(format!("tile_{}_{}_{}.png", x, y, layer))?)
}
