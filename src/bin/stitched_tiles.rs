use std::path::PathBuf;
use std::sync::Arc;
use tokio::time::Instant;
use zomboid_map::ZResult;
use zomboid_map::handout::{ImageCell, Img};
use zomboid_map::top_render::{read_lots, render_top};

#[tokio::main]
async fn main() -> ZResult<()> {
    let path = PathBuf::from("D:\\SteamLibrary\\steamapps\\common\\ProjectZomboid\\media\\maps\\Muldraugh, KY\\");

    let start = Instant::now();
    let width = 256 * 24;
    let height = 256 * 24;
    let ic = Arc::new(ImageCell::new(Img::new(width, height)));
    let mut total_tasks = 0;
    let mut tasks = vec![];
    for x in 0..24 {
        for y in 19..43 {
            total_tasks += 1;
            let ic = Arc::clone(&ic);
            let path = path.clone();
            let fut = async move {
                let pack = read_lots(path.clone(), x, y).await?;
                let x_offset = x * 256;
                let y_offset = (y - 19) * 256;
                render_top(ic, pack, x_offset, y_offset, 0)
            };
            tasks.push(tokio::task::spawn(fut));
        }
    }
    let mut finished_tasks = 0;
    for t in futures::future::join_all(tasks).await {
        finished_tasks += 1;
        println!("Finished ({}/{})", finished_tasks, total_tasks);
    }

    println!("Finished in: {:?}", start.elapsed());
    ic.save("final.png").expect("Save Failed");

    Ok(())
}
