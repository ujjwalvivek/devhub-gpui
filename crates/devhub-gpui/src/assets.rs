use std::borrow::Cow;

use gpui::{AssetSource, Result, SharedString};

pub(crate) struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if path == "zed.svg" {
            return Ok(Some(Cow::Borrowed(include_bytes!("../assets/zed.svg"))));
        }
        gpui_component_assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut assets = gpui_component_assets::Assets.list(path)?;
        if path.is_empty() {
            assets.push("zed.svg".into());
        }
        Ok(assets)
    }
}
