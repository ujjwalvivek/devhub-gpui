#[cfg(any(windows, target_os = "macos"))]
#[path = "../../build/app_icon.rs"]
mod app_icon;

fn main() {
    const SVG_PATH: &str = "assets/appicon.svg";

    println!("cargo:rerun-if-changed={SVG_PATH}");
    #[cfg(windows)]
    app_icon::embed_windows(SVG_PATH);
    #[cfg(target_os = "macos")]
    {
        println!("cargo:rerun-if-env-changed=DEVHUB_MACOS_ICONSET_OUT");
        let output = std::env::var_os("DEVHUB_MACOS_ICONSET_OUT")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap())
                    .join("DevHub.iconset")
            });
        app_icon::write_macos_iconset(SVG_PATH, &output);
    }
}
