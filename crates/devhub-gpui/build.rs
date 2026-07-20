use std::env;
use std::fs;
use std::path::PathBuf;

const ICON_SIZES: [u32; 9] = [16, 20, 24, 32, 40, 48, 64, 128, 256];

fn main() {
    println!("cargo:rerun-if-changed=assets/appicon.svg");
    if env::var_os("CARGO_CFG_WINDOWS").is_none() {
        return;
    }

    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let svg_path = manifest.join("assets/appicon.svg");
    let svg = fs::read(&svg_path).expect("read appicon.svg");
    let tree = resvg::usvg::Tree::from_data(&svg, &resvg::usvg::Options::default())
        .expect("parse appicon.svg");
    let images = ICON_SIZES
        .into_iter()
        .map(|size| render_icon(&tree, size))
        .collect::<Vec<_>>();

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let icon_path = out_dir.join("appicon.ico");
    let ico = encode_ico(&images);
    fs::write(&icon_path, ico).expect("write appicon.ico");

    winresource::WindowsResource::new()
        .set_icon(icon_path.to_str().expect("UTF-8 app icon path"))
        .compile()
        .expect("embed Windows app icon");
}

fn render_icon(tree: &resvg::usvg::Tree, size: u32) -> (u32, Vec<u8>) {
    let mut pixmap = resvg::tiny_skia::Pixmap::new(size, size).expect("allocate app icon pixmap");
    let transform = resvg::tiny_skia::Transform::from_scale(
        size as f32 / tree.size().width(),
        size as f32 / tree.size().height(),
    );
    resvg::render(tree, transform, &mut pixmap.as_mut());
    let png = pixmap.encode_png().expect("encode app icon PNG");
    (size, png)
}

fn encode_ico(images: &[(u32, Vec<u8>)]) -> Vec<u8> {
    const HEADER_SIZE: usize = 6;
    const DIRECTORY_ENTRY_SIZE: usize = 16;

    let data_offset = HEADER_SIZE + DIRECTORY_ENTRY_SIZE * images.len();
    let image_bytes = images.iter().map(|(_, png)| png.len()).sum::<usize>();
    let mut ico = Vec::with_capacity(data_offset + image_bytes);
    ico.extend_from_slice(&0u16.to_le_bytes());
    ico.extend_from_slice(&1u16.to_le_bytes());
    ico.extend_from_slice(&(images.len() as u16).to_le_bytes());

    let mut offset = data_offset as u32;
    for (size, png) in images {
        let dimension = if *size == 256 { 0 } else { *size as u8 };
        ico.extend_from_slice(&[dimension, dimension, 0, 0]);
        ico.extend_from_slice(&1u16.to_le_bytes());
        ico.extend_from_slice(&32u16.to_le_bytes());
        ico.extend_from_slice(&(png.len() as u32).to_le_bytes());
        ico.extend_from_slice(&offset.to_le_bytes());
        offset += png.len() as u32;
    }

    for (_, png) in images {
        ico.extend_from_slice(png);
    }
    ico
}
