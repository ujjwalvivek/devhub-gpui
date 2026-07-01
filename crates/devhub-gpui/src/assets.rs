use std::borrow::Cow;

use gpui::{AssetSource, Result, SharedString};

pub(crate) struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if path == "appicon.svg" {
            return Ok(Some(Cow::Borrowed(include_bytes!("../../../appicon.svg"))));
        }
        if path == "appicon.png" {
            return Ok(Some(Cow::Borrowed(include_bytes!(concat!(
                env!("OUT_DIR"),
                "/appicon.png"
            )))));
        }
        gpui_component_assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut assets = gpui_component_assets::Assets.list(path)?;
        if "appicon.svg".starts_with(path) {
            assets.push("appicon.svg".into());
        }
        if "appicon.png".starts_with(path) {
            assets.push("appicon.png".into());
        }
        Ok(assets)
    }
}
