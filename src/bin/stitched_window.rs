//! Windowed (native) stitched-map viewer, built on the `Backend` abstraction.
//!
//! Flow: build the whole stitched canvas once on the CPU (async pack reads +
//! parallel render on the blocking pool), then hand it to a `WgpuBackend` and
//! display it in a winit window. The same `WgpuBackend::present` call is what
//! a web build would use — only this file's windowing/entry differs.
//!
//! SKETCH ONLY. Requires:
//!   * the `render_top(&mut CellColors, ...)` refactor from the design notes
//!   * `pub mod render_backend; pub mod gpu;` added to `lib.rs`
//!   * deps: wgpu, winit = "0.30", pollster, bytemuck, raw-window-handle
//! Not declared in Cargo.toml; cargo auto-discovers files in src/bin, so it
//! will try (and, until the above is in place, fail) to build.

use std::path::PathBuf;
use std::sync::Arc;

use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

use zomboid_map::gpu::WgpuBackend;
use zomboid_map::render_backend::{Backend, CellColors};
use zomboid_map::top_render::{read_lots, render_cell, render_top};
use zomboid_map::{ZError, ZResult};

const CELLS: u32 = 256; // squares per pack edge
const X_RANGE: std::ops::Range<usize> = 0..24;
const Y_RANGE: std::ops::Range<usize> = 19..43;

async fn build_canvas(path: PathBuf) -> ZResult<CellColors> {
    let (nx, ny) = (X_RANGE.len() as u32, Y_RANGE.len() as u32);
    let mut canvas = CellColors::new(CELLS * nx, CELLS * ny);

    let mut tasks = Vec::new();
    for x in X_RANGE {
        for y in Y_RANGE {
            let path = path.clone();
            tasks.push(tokio::spawn(async move {
                let pack = read_lots(path, x, y).await?;

                let tile = tokio::task::spawn_blocking(move || {
                    let mut tile = CellColors::new(CELLS, CELLS);
                    render_cell(&mut tile, pack, 0, 0, 0).expect("TODO: panic message");
                    tile
                })
                .await
                .expect("render task panicked");
                Ok::<_, ZError>((x, y, tile))
            }));
        }
    }

    for joined in futures::future::join_all(tasks).await {
        let (x, y, tile) = joined.expect("join failed")?;
        let x_off = x as u32 * CELLS;
        let y_off = (y as u32 - Y_RANGE.start as u32) * CELLS;
        // Blit the tile into its slot. One-time copy; negligible vs. the render.
        for ty in 0..CELLS {
            let dst_row = ((y_off + ty) * canvas.width + x_off) as usize;
            let src_row = (ty * CELLS) as usize;
            canvas.pixels[dst_row..dst_row + CELLS as usize].copy_from_slice(&tile.pixels[src_row..src_row + CELLS as usize]);
        }
    }
    Ok(canvas)
}

#[derive(Default)]
struct App {
    colors: Option<Arc<CellColors>>,
    window: Option<Arc<Window>>,
    backend: Option<WgpuBackend>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.window.is_some() {
            return; // `resumed` can fire again (e.g. on Android); build once.
        }
        let attrs = Window::default_attributes().with_title("zomboid map");
        let window = Arc::new(el.create_window(attrs).expect("create window"));
        let size = window.inner_size();

        // Native init is blocking; on web this would be `spawn_local(async { .. })`.
        let backend = pollster::block_on(WgpuBackend::new(window.clone(), size.width.max(1), size.height.max(1))).expect("wgpu init");

        self.backend = Some(backend);
        self.window = Some(window);
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let (Some(backend), Some(window), Some(colors)) = (self.backend.as_mut(), self.window.as_ref(), self.colors.as_ref()) else {
            return;
        };
        match event {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::Resized(size) => {
                backend.resize(size.width, size.height);
                window.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                // Colors are static here, so we just re-present. Once you add
                // pan/zoom, update a transform uniform and redraw on input.
                if let Err(e) = backend.present(colors, &[]) {
                    eprintln!("present failed: {e}");
                }
            }
            _ => {}
        }
    }
}

fn main() -> ZResult<()> {
    let path = PathBuf::from("D:\\SteamLibrary\\steamapps\\common\\ProjectZomboid\\media\\maps\\Muldraugh, KY\\");

    // 1. Compute the canvas on a tokio runtime (reads + parallel render).
    let rt = tokio::runtime::Runtime::new()?;
    let colors = rt.block_on(build_canvas(path))?;
    println!("canvas built: {}x{}", colors.width, colors.height);

    // 2. Display it. `Wait` keeps the loop idle until an event (no busy redraw).
    let mut app = App {
        colors: Some(Arc::new(colors)),
        ..Default::default()
    };
    let event_loop = EventLoop::new().expect("event loop");
    event_loop.set_control_flow(ControlFlow::Wait);
    event_loop.run_app(&mut app).expect("run_app");
    Ok(())
}
