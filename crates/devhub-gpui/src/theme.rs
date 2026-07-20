use devhub_core::{AppearanceMode, ThemeId};
use gpui::{rgb, Hsla, WindowAppearance};

pub const UI_FONT: &str = "Segoe UI";
pub const MONO_FONT: &str = "Consolas";
pub const TERMINAL_FONT: &str = "DepartureMono Nerd Font Mono";

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
    pub is_light: bool,
}

#[derive(Clone, Copy)]
struct Palette {
    background: u32,
    panel: u32,
    surface: u32,
    hover: u32,
    selected: u32,
    text: u32,
    muted: u32,
    disabled: u32,
    accent: u32,
    accent_dim: u32,
    border: u32,
    warning: u32,
}

impl Theme {
    pub fn for_preferences(
        theme: ThemeId,
        appearance: AppearanceMode,
        window_appearance: WindowAppearance,
    ) -> Self {
        let is_light = match appearance {
            AppearanceMode::System => matches!(
                window_appearance,
                WindowAppearance::Light | WindowAppearance::VibrantLight
            ),
            AppearanceMode::Dark => false,
            AppearanceMode::Light => true,
        };
        let palette = palette(theme, is_light);
        Self {
            app_background: color(palette.background),
            titlebar_background: color(palette.background),
            titlebar_inactive_background: color(palette.panel),
            sidebar_background: color(palette.background),
            panel_background: color(palette.panel),
            surface_background: color(palette.surface),
            surface_hover: color(palette.hover),
            surface_selected: color(palette.selected),
            border: color(palette.border),
            border_strong: color(palette.accent_dim),
            text: color(palette.text),
            text_muted: color(palette.muted),
            text_disabled: color(palette.disabled),
            accent: color(palette.accent),
            accent_hover: color(mix(palette.accent, palette.text, 0.18)),
            focus: color(mix(palette.accent, palette.text, 0.35)),
            success: color(if is_light { 0x2f7d4a } else { 0x73b88a }),
            warning: color(palette.warning),
            error: color(if is_light { 0xb4232c } else { 0xd06c70 }),
            close_hover: color(0xc42b1c),
            is_light,
        }
    }
}

fn palette(theme: ThemeId, light: bool) -> Palette {
    match (theme, light) {
        (ThemeId::CatppuccinMocha, false) => dark_palette([
            0x1e1e2e, 0x27283a, 0x313244, 0x373d55, 0x3b4561, 0xcdd6f4, 0x939abb, 0x6f7590,
            0x89b4fa, 0x526c9a, 0x45465b, 0xfab387,
        ]),
        (ThemeId::RosePineMoon, false) => dark_palette([
            0x232136, 0x26243a, 0x2a273f, 0x342f4c, 0x3b3350, 0xe0def4, 0xa9a4c6, 0x787392,
            0xea9a97, 0x8e5e69, 0x484361, 0xf6c177,
        ]),
        (ThemeId::TokyoNightStorm, false) => dark_palette([
            0x24283b, 0x292e43, 0x2f3549, 0x363e57, 0x3a4562, 0xa9b1d6, 0x7f88b0, 0x5e6687,
            0x7aa2f7, 0x4b6193, 0x434a63, 0xbb9af7,
        ]),
        (ThemeId::HorizonBold, false) => dark_palette([
            0x1c1e26, 0x20222c, 0x232530, 0x2b2d3a, 0x352d39, 0xd5d8da, 0x9ba0a6, 0x6f747d,
            0xe95678, 0x8a364b, 0x434550, 0xfab795,
        ]),
        (ThemeId::MonochromeZero, false) => dark_palette([
            0x0e0e0e, 0x0e0e0e, 0x151515, 0x191919, 0x222222, 0xd8d8d8, 0x858585, 0x555555,
            0xd8d8d8, 0x303030, 0x202020, 0xc49a5a,
        ]),
        (ThemeId::CatppuccinMocha, true) => {
            light_palette(0xeff1f5, 0xffffff, 0x1e66f5, 0xfe640b, 0x4c4f69)
        }
        (ThemeId::RosePineMoon, true) => {
            light_palette(0xfaf4ed, 0xfffaf3, 0xd7827e, 0xea9d34, 0x575279)
        }
        (ThemeId::TokyoNightStorm, true) => {
            light_palette(0xe1e2e7, 0xf4f5fa, 0x2e7de9, 0x9852e0, 0x343b58)
        }
        (ThemeId::HorizonBold, true) => {
            light_palette(0xf3f0f2, 0xffffff, 0xd4385f, 0xc77547, 0x2a2d35)
        }
        (ThemeId::MonochromeZero, true) => {
            light_palette(0xffffff, 0xf6f6f6, 0x000000, 0x4d4d4d, 0x111111)
        }
    }
}

fn dark_palette(values: [u32; 12]) -> Palette {
    Palette {
        background: values[0],
        panel: values[1],
        surface: values[2],
        hover: values[3],
        selected: values[4],
        text: values[5],
        muted: values[6],
        disabled: values[7],
        accent: values[8],
        accent_dim: values[9],
        border: values[10],
        warning: values[11],
    }
}

fn light_palette(background: u32, surface: u32, accent: u32, warning: u32, text: u32) -> Palette {
    Palette {
        background,
        panel: mix(background, surface, 0.45),
        surface,
        hover: mix(surface, accent, 0.10),
        selected: mix(surface, accent, 0.18),
        text,
        muted: mix(text, background, 0.36),
        disabled: mix(text, background, 0.58),
        accent,
        accent_dim: mix(accent, background, 0.58),
        border: mix(text, background, 0.75),
        warning,
    }
}

fn color(hex: u32) -> Hsla {
    rgb(hex).into()
}

fn mix(a: u32, b: u32, b_weight: f32) -> u32 {
    let a_weight = 1.0 - b_weight;
    let channel = |shift: u32| {
        ((((a >> shift) & 0xff_u32) as f32 * a_weight)
            + (((b >> shift) & 0xff_u32) as f32 * b_weight))
            .round() as u32
    };
    (channel(16) << 16) | (channel(8) << 8) | channel(0)
}
