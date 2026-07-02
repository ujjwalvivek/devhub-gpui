use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=../../appicon.svg");
    if env::var_os("CARGO_CFG_WINDOWS").is_none() {
        return;
    }

    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let svg_path = manifest.join("../../appicon.svg");
    let svg = fs::read(&svg_path).expect("read appicon.svg");
    let tree = resvg::usvg::Tree::from_data(&svg, &resvg::usvg::Options::default())
        .expect("parse appicon.svg");
    let mut pixmap = resvg::tiny_skia::Pixmap::new(256, 256).expect("allocate app icon pixmap");
    let transform = resvg::tiny_skia::Transform::from_scale(
        256.0 / tree.size().width(),
        256.0 / tree.size().height(),
    );
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    let png = pixmap.encode_png().expect("encode app icon PNG");

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let icon_path = out_dir.join("appicon.ico");
    let mut ico = Vec::with_capacity(22 + png.len());
    ico.extend_from_slice(&[0, 0, 1, 0, 1, 0]);
    ico.extend_from_slice(&[0, 0, 0, 0]);
    ico.extend_from_slice(&1u16.to_le_bytes());
    ico.extend_from_slice(&32u16.to_le_bytes());
    ico.extend_from_slice(&(png.len() as u32).to_le_bytes());
    ico.extend_from_slice(&22u32.to_le_bytes());
    ico.extend_from_slice(&png);
    fs::write(&icon_path, ico).expect("write appicon.ico");

    winresource::WindowsResource::new()
        .set_icon(icon_path.to_str().expect("UTF-8 app icon path"))
        .compile()
        .expect("embed Windows app icon");
}
