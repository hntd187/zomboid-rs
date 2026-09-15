use std::env;
use std::error::Error;
use std::fs;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use wide::f32x4;

#[inline]
pub fn sum_vecs(pixels: &[f32]) -> f32x4 {
    let mut out = f32x4::ZERO;
    let (chunks, _) = pixels.as_chunks::<4>();
    for c in chunks.iter() {
        out += f32x4::from([c[0], c[1], c[2], 1.0]) * c[3]
    }
    out
}

fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo::rerun-if-changed=build.rs");
    // let texture_path = PathBuf::from(env::var("ZOMBOID_TEXTURE_PATH")?);
    let texture_path = PathBuf::from("D:\\pzmap\\texture\\default");
    let out_dir = env::var("OUT_DIR")?;
    let dest_path = Path::new(&out_dir).join("static_textures.rs");
    let mut output_file = File::create(dest_path)?;

    writeln!(&mut output_file, "pub static PRECOMPUTED_COLORS: phf::Map<&str, wide::f32x4> = phf::phf_map! {{")?;

    for tile in fs::read_dir(&texture_path)? {
        let tile = tile?;
        if let Ok(tile_file_name) = tile.file_name().into_string()
            && let Some(file_name) = tile_file_name.strip_suffix(".png")
        {
            let img = image::open(tile.path())?.into_rgba32f();
            let sum = sum_vecs(&img);
            writeln!(&mut output_file, "\"{}\" => wide::f32x4::new({:?}),", file_name, sum.to_array())?;
        }
    }

    writeln!(&mut output_file, "}};\n")?;

    Ok(())
}
