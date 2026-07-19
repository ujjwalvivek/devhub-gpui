use devhub_core::ProjectType;
use devhub_gpui::{ScanState, Theme, MONO_FONT};
use gpui::prelude::*;
use gpui::*;
use gpui_component::IconName;

use crate::app::WindowCommand;
use crate::platform::toggle_window_zoom;

pub(crate) fn window_control(
    id: &'static str,
    glyph: &'static str,
    command: WindowCommand,
    window_active: bool,
    theme: Theme,
) -> impl IntoElement {
    let area = match command {
        WindowCommand::Minimize => WindowControlArea::Min,
        WindowCommand::Maximize => WindowControlArea::Max,
        WindowCommand::Close => WindowControlArea::Close,
    };
    let is_close = matches!(command, WindowCommand::Close);

    div()
        .id(id)
        .w(px(42.0))
        .h_full()
        .flex()
        .items_center()
        .justify_center()
        .window_control_area(area)
        .font_family("Segoe UI Symbol")
        .text_size(px(if is_close { 16.0 } else { 12.0 }))
        .text_color(if window_active {
            theme.text
        } else {
            theme.text_muted
        })
        .hover(move |style| {
            if is_close {
                style.bg(theme.close_hover).text_color(theme.text)
            } else {
                style.bg(theme.surface_hover).text_color(theme.text)
            }
        })
        .active(move |style| {
            if is_close {
                style.bg(theme.close_hover.opacity(0.8))
            } else {
                style.bg(theme.surface_selected)
            }
        })
        .child(glyph)
        .on_click(move |_, window, _| match command {
            WindowCommand::Minimize => window.minimize_window(),
            WindowCommand::Maximize => toggle_window_zoom(window),
            WindowCommand::Close => window.remove_window(),
        })
}

pub(crate) fn message_panel(
    title: &'static str,
    message: impl Into<SharedString>,
    color: Hsla,
    theme: Theme,
) -> Div {
    div()
        .m_2()
        .pl_2()
        .py_1()
        .border_l_1()
        .border_color(color)
        .child(
            div()
                .font_family(MONO_FONT)
                .text_size(px(10.0))
                .text_color(color)
                .child(title),
        )
        .child(
            div()
                .mt_1()
                .text_size(px(11.0))
                .text_color(theme.text_muted)
                .child(message.into()),
        )
}

pub(crate) fn workbench_message(message: impl Into<SharedString>, color: Hsla) -> AnyElement {
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .font_family(MONO_FONT)
        .text_size(px(11.0))
        .text_color(color)
        .child(message.into())
        .into_any_element()
}

pub(crate) fn section_label(label: &'static str, theme: Theme) -> Div {
    div()
        .font_family(MONO_FONT)
        .font_weight(FontWeight::SEMIBOLD)
        .text_size(px(10.0))
        .text_color(theme.text_disabled)
        .child(label)
}

pub(crate) fn scan_status(state: &ScanState, root: &std::path::Path) -> String {
    match state {
        ScanState::Idle => "Cached projects; explicit scan required.".to_string(),
        ScanState::Scanning => format!("Scanning {}...", root.display()),
        ScanState::Loaded { count } => format!("Loaded {count} project(s)."),
        ScanState::Empty => "Scan completed with no projects.".to_string(),
        ScanState::Error(_) => "Scan failed. Review the message above.".to_string(),
    }
}

pub(crate) fn scan_state_color(theme: Theme, state: &ScanState) -> Hsla {
    match state {
        ScanState::Idle => theme.text_disabled,
        ScanState::Scanning => theme.warning,
        ScanState::Loaded { .. } => theme.success,
        ScanState::Empty => theme.text_muted,
        ScanState::Error(_) => theme.error,
    }
}

pub(crate) fn project_type_color(theme: Theme, project_type: ProjectType) -> Hsla {
    match project_type {
        ProjectType::Rust => theme.warning,
        ProjectType::Node => theme.success,
        ProjectType::Go => theme.accent,
        ProjectType::Python => theme.focus,
        ProjectType::Assembly => theme.text,
        _ => theme.text_muted,
    }
}

pub(crate) fn file_icon(name: &str) -> IconName {
    if let Some(dot) = name.rfind('.') {
        match &name[dot + 1..] {
            "md" | "txt" | "rst" => IconName::BookOpen,
            "png" | "jpg" | "jpeg" | "gif" | "svg" | "ico" | "webp" => IconName::GalleryVerticalEnd,
            "sh" | "bash" | "ps1" | "bat" | "cmd" => IconName::SquareTerminal,
            "toml" | "json" | "yaml" | "yml" | "ini" | "conf" => IconName::Settings2,
            _ => IconName::File,
        }
    } else {
        IconName::File
    }
}
