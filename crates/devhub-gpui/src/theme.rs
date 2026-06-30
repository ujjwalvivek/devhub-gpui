use gpui::{rgb, Hsla};

pub const UI_FONT: &str = "Segoe UI";
pub const MONO_FONT: &str = "Consolas";

#[derive(Clone, Copy)]
pub struct Theme {
    pub app_background: Hsla,
    pub titlebar_background: Hsla,
    pub titlebar_inactive_background: Hsla,
    pub sidebar_background: Hsla,
    pub panel_background: Hsla,
    pub surface_background: Hsla,
    pub surface_hover: Hsla,
    pub surface_selected: Hsla,
    pub border: Hsla,
    pub border_strong: Hsla,
    pub text: Hsla,
    pub text_muted: Hsla,
    pub text_disabled: Hsla,
    pub accent: Hsla,
    pub accent_hover: Hsla,
    pub focus: Hsla,
    pub success: Hsla,
    pub warning: Hsla,
    pub error: Hsla,
    pub close_hover: Hsla,
}

impl Theme {
    pub fn dark() -> Self {
        Self {
            app_background: color(0x0e0e0e),
            titlebar_background: color(0x0e0e0e),
            titlebar_inactive_background: color(0x111111),
            sidebar_background: color(0x0e0e0e),
            panel_background: color(0x0e0e0e),
            surface_background: color(0x151515),
            surface_hover: color(0x191919),
            surface_selected: color(0x222222),
            border: color(0x202020),
            border_strong: color(0x303030),
            text: color(0xd8d8d8),
            text_muted: color(0x858585),
            text_disabled: color(0x555555),
            accent: color(0x5b9bd5),
            accent_hover: color(0x72afe5),
            focus: color(0xa8c7e8),
            success: color(0x73b88a),
            warning: color(0xc49a5a),
            error: color(0xd06c70),
            close_hover: color(0xc42b1c),
        }
    }
}

fn color(hex: u32) -> Hsla {
    rgb(hex).into()
}
