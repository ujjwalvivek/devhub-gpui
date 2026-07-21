#[cfg(windows)]
#[path = "../../build/app_icon.rs"]
mod app_icon;

fn main() {
    const SVG_PATH: &str = "../devhub-gpui/assets/appicon.svg";

    println!("cargo:rerun-if-changed={SVG_PATH}");
    #[cfg(windows)]
    app_icon::embed_windows(SVG_PATH);
}
