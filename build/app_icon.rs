#[cfg(any(windows, target_os = "macos"))]
use std::env;
#[cfg(any(windows, target_os = "macos"))]
use std::fs;
#[cfg(target_os = "macos")]
use std::path::Path;
#[cfg(any(windows, target_os = "macos"))]
use std::path::PathBuf;

#[cfg(any(windows, target_os = "macos"))]
fn load_svg(svg_relative_path: &str) -> resvg::usvg::Tree {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let svg_path = manifest.join(svg_relative_path);
    let svg = fs::read(&svg_path).expect("read appicon.svg");
    resvg::usvg::Tree::from_data(&svg, &resvg::usvg::Options::default()).expect("parse appicon.svg")
}

#[cfg(any(windows, target_os = "macos"))]
fn render_icon(tree: &resvg::usvg::Tree, size: u32) -> Vec<u8> {
    let mut pixmap = resvg::tiny_skia::Pixmap::new(size, size).expect("allocate app icon pixmap");
    let transform = resvg::tiny_skia::Transform::from_scale(
        size as f32 / tree.size().width(),
        size as f32 / tree.size().height(),
    );
    resvg::render(tree, transform, &mut pixmap.as_mut());
    pixmap.encode_png().expect("encode app icon PNG")
}

#[cfg(windows)]
pub fn embed_windows(svg_relative_path: &str) {
    const ICON_SIZES: [u32; 9] = [16, 20, 24, 32, 40, 48, 64, 128, 256];

    let tree = load_svg(svg_relative_path);
    let images = ICON_SIZES
        .into_iter()
        .map(|size| (size, render_icon(&tree, size)))
        .collect::<Vec<_>>();
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let icon_path = out_dir.join("appicon.ico");
    fs::write(&icon_path, encode_ico(&images)).expect("write appicon.ico");

    winresource::WindowsResource::new()
        .set_icon(icon_path.to_str().expect("UTF-8 app icon path"))
        .compile()
        .expect("embed Windows app icon");
}

#[cfg(target_os = "macos")]
pub fn write_macos_iconset(svg_relative_path: &str, output: &Path) {
    const ICON_FILES: [(&str, u32); 10] = [
        ("icon_16x16.png", 16),
        ("icon_16x16@2x.png", 32),
        ("icon_32x32.png", 32),
        ("icon_32x32@2x.png", 64),
        ("icon_128x128.png", 128),
        ("icon_128x128@2x.png", 256),
        ("icon_256x256.png", 256),
        ("icon_256x256@2x.png", 512),
        ("icon_512x512.png", 512),
        ("icon_512x512@2x.png", 1024),
    ];

    let tree = load_svg(svg_relative_path);
    fs::create_dir_all(output).expect("create macOS app iconset");
    for (name, size) in ICON_FILES {
        fs::write(output.join(name), render_icon(&tree, size)).expect("write macOS app icon PNG");
    }
}

#[cfg(windows)]
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
