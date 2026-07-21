use std::borrow::Cow;

use gpui::{App, AssetSource, Result, SharedString};

pub(crate) struct Assets;

impl Assets {
    pub(crate) fn register_fonts(cx: &App) -> Result<()> {
        cx.text_system()
            .add_fonts(vec![Cow::Borrowed(include_bytes!(
                "../assets/fonts/DepartureMono/DepartureMonoNerdFontMono-Regular.otf"
            ))])
    }
}

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if path == "zed.svg" {
            return Ok(Some(Cow::Borrowed(include_bytes!("../assets/zed.svg"))));
        }
        if path == "git-branch.svg" {
            return Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/git-branch.svg"
            ))));
        }
        if path == "history.svg" {
            return Ok(Some(Cow::Borrowed(include_bytes!("../assets/history.svg"))));
        }
        if path == "scan-search.svg" {
            return Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/scan-search.svg"
            ))));
        }
        if path == "md-code.svg" {
            return Ok(Some(Cow::Borrowed(include_bytes!("../assets/md-code.svg"))));
        }
        if path == "md-preview.svg" {
            return Ok(Some(Cow::Borrowed(include_bytes!(
                "../assets/md-preview.svg"
            ))));
        }
        if path == "mcp.svg" {
            return Ok(Some(Cow::Borrowed(include_bytes!("../assets/mcp.svg"))));
        }
        gpui_component_assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut assets = gpui_component_assets::Assets.list(path)?;
        if path.is_empty() {
            assets.push("zed.svg".into());
            assets.push("git-branch.svg".into());
            assets.push("history.svg".into());
            assets.push("scan-search.svg".into());
            assets.push("md-code.svg".into());
            assets.push("md-preview.svg".into());
            assets.push("mcp.svg".into());
        }
        Ok(assets)
    }
}
