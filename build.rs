use std::env;
use std::error::Error;
use std::fs;
use std::fs::{ File};
use std::io::Write;
use std::path::{Path, PathBuf};
use wide::f32x4;

#[inline]
pub fn alt_sum_vecs(sum: &[f32]) -> f32x4 {
    let mut out = f32x4::splat(0.0);
    for c in sum.chunks_exact(4) {
        let t = (c[3] == 1.0) as i32;
        out += f32x4::from([c[0], c[1], c[2], 1.0]) * t as f32
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

    writeln!(&mut output_file, "use phf::phf_map;")?;
    writeln!(&mut output_file, "use wide::f32x4;")?;
    writeln!(
        &mut output_file,
        "pub static PRECOMPUTED_COLORS: phf::Map<&str, wide::f32x4> = phf::phf_map! {{"
    )?;

    // let tiles: Vec<DirEntry> = fs::read_dir(&texture_path)?.try_collect()?;
    // let tups: Vec<_> = tiles
    //     .into_iter()
    //     .filter_map(|t| {
    //         let path = t.path();
    //         let file_name = t.file_name();
    //         if path.ends_with(".png") {
    //             Some((path, file_name))
    //         } else {
    //             None
    //         }
    //     })
    //     .map(|(p, f)| {
    //         let img = image::open(p).unwrap().into_rgba32f();
    //         let sum = alt_sum_vecs(&img);
    //         CollectionEntry::set_entry(f, parse_quote!(wide::f32x4::new(sum.to_array())))
    //     })
    //     .collect();
    // let vis: Visibility = parse_quote!(pub);
    // let cd = CollectionEmitter::new(&parse_quote!(&'static str))
    //     .value_type(&parse_quote!(wide::f32x4))
    //     .symbol_name("PRECOMPUTED_COLORS")
    //     .static_instance(true)
    //     .visibility(vis)
    //     .const_keys(true)
    //     .const_values(true)
    //     .emit_hash_collection(tups)?;

    // writeln!(&mut output_file, "{cd}")?;

    for tile in fs::read_dir(&texture_path)? {
        let tile = tile?;
        if let Ok(tile_file_name) = tile.file_name().into_string()
            && let Some(file_name) = tile_file_name.strip_suffix(".png")
        {
            let img = image::open(tile.path())?.into_rgba32f();
            let sum = alt_sum_vecs(&img);
            writeln!(
                &mut output_file,
                "\"{}\" => wide::f32x4::new({:?}),",
                file_name,
                sum.to_array()
            )?;
        }
    }
    //
    writeln!(&mut output_file, "}};\n")?;

    Ok(())
}
