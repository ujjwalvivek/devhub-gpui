mod platform;

use devhub_core::{scan_directories, Project, ProjectSource, ProjectType};
use devhub_gpui::{
    next_selection, previous_selection, ScanModel, ScanState, Theme, MONO_FONT, UI_FONT,
};
use gpui::prelude::*;
use gpui::*;
use platform::{begin_window_drag, configure_windows_surface};
use std::path::PathBuf;

const TITLEBAR_HEIGHT: f32 = 34.0;
const PROJECT_ROW_HEIGHT: f32 = 30.0;
const SIDEBAR_WIDTH: f32 = 260.0;

actions!(
    devhub,
    [
        SelectPreviousProject,
        SelectNextProject,
        SelectFirstProject,
        SelectLastProject,
        ScanCurrentFolder
    ]
);

struct DevHubLite {
    scan: ScanModel,
    selected: Option<usize>,
    scan_root: PathBuf,
    focus_handle: FocusHandle,
    project_scroll: ScrollHandle,
}

#[derive(Clone, Copy)]
enum WindowCommand {
    Minimize,
    Maximize,
    Close,
}

impl DevHubLite {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        window.focus(&focus_handle);

        Self {
            scan: ScanModel::new(vec![
                fixture_project(
                    "devhub",
                    r"F:\_Engine\devhub",
                    ProjectType::Rust,
                    "Cargo.toml",
                ),
                fixture_project(
                    "gpui-playground",
                    r"F:\_Engine\gpui",
                    ProjectType::Rust,
                    "Cargo.toml",
                ),
                fixture_project(
                    "echopoint",
                    r"F:\_Engine\echopoint",
                    ProjectType::Node,
                    "package.json",
                ),
            ]),
            selected: None,
            scan_root: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            focus_handle,
            project_scroll: ScrollHandle::new(),
        }
    }

    fn select_project(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus_handle);
        self.set_selected(index, cx);
    }

    fn start_scan(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus_handle);
        self.begin_scan(cx);
    }

    fn scan_current_folder(
        &mut self,
        _: &ScanCurrentFolder,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.begin_scan(cx);
    }

    fn begin_scan(&mut self, cx: &mut Context<Self>) {
        let generation = self.scan.begin();
        let root = self.scan_root.clone();

        self.selected = None;
        cx.notify();

        // Filesystem walking is blocking work, so it runs on GPUI's background
        // executor. Only the foreground task updates the root entity.
        let scan_task = cx.background_executor().spawn(async move {
            let metadata = std::fs::metadata(&root)
                .map_err(|error| format!("Cannot scan {}: {error}", root.display()))?;
            if !metadata.is_dir() {
                return Err(format!("Scan root is not a directory: {}", root.display()));
            }

            Ok(scan_directories(&[root], 3))
        });

        cx.spawn(async move |this, cx| {
            let result = scan_task.await;
            let _ = this.update(cx, |this, cx| {
                if this.scan.apply_result(generation, result) {
                    this.selected = None;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn set_selected(&mut self, index: usize, cx: &mut Context<Self>) {
        self.selected = Some(index);
        self.project_scroll.scroll_to_item(index);
        cx.notify();
    }

    fn select_previous_project(
        &mut self,
        _: &SelectPreviousProject,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(index) = previous_selection(self.selected, self.scan.projects.len()) {
            self.set_selected(index, cx);
        }
    }

    fn select_next_project(
        &mut self,
        _: &SelectNextProject,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(index) = next_selection(self.selected, self.scan.projects.len()) {
            self.set_selected(index, cx);
        }
    }

    fn select_first_project(
        &mut self,
        _: &SelectFirstProject,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.scan.projects.is_empty() {
            self.set_selected(0, cx);
        }
    }

    fn select_last_project(
        &mut self,
        _: &SelectLastProject,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.scan.projects.is_empty() {
            self.set_selected(self.scan.projects.len() - 1, cx);
        }
    }
}

impl Focusable for DevHubLite {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for DevHubLite {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::dark();
        let window_active = window.is_window_active();
        let window_maximized = window.is_maximized();
        let project_list_focused = self.focus_handle.is_focused(window);
        let titlebar_background = if window_active {
            theme.titlebar_background
        } else {
            theme.titlebar_inactive_background
        };
        let selected_project = self
            .selected
            .and_then(|index| self.scan.projects.get(index))
            .cloned();
        let scan_button_label = if self.scan.state == ScanState::Scanning {
            "Scan again"
        } else {
            "Scan"
        };
        let scan_status = scan_status(&self.scan.state, &self.scan_root);
        let status_color = scan_state_color(theme, &self.scan.state);
        let project_count = self.scan.projects.len();

        let project_rows =
            self.scan
                .projects
                .iter()
                .cloned()
                .enumerate()
                .map(|(index, project)| {
                    let is_selected = self.selected == Some(index);
                    let background = if is_selected {
                        theme.surface_selected
                    } else {
                        theme.sidebar_background
                    };
                    let type_color = project_type_color(theme, project.project_type);

                    div()
                        .id(("project-row", index))
                        .h(px(PROJECT_ROW_HEIGHT))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .border_l_1()
                        .border_color(if is_selected {
                            if project_list_focused {
                                theme.focus
                            } else {
                                theme.border_strong
                            }
                        } else {
                            theme.sidebar_background
                        })
                        .bg(background)
                        .cursor_pointer()
                        .hover(move |style| style.bg(theme.surface_hover))
                        .active(move |style| style.bg(theme.surface_background))
                        .child(
                            div()
                                .w(px(17.0))
                                .flex_shrink_0()
                                .text_center()
                                .text_size(px(13.0))
                                .text_color(theme.accent)
                                .child(if is_selected { "›" } else { " " }),
                        )
                        .child(
                            div()
                                .min_w_0()
                                .flex_1()
                                .flex()
                                .items_baseline()
                                .gap_2()
                                .child(
                                    div()
                                        .min_w_0()
                                        .flex_1()
                                        .whitespace_nowrap()
                                        .overflow_hidden()
                                        .text_size(px(13.0))
                                        .text_color(theme.text)
                                        .child(project.name.clone()),
                                )
                                .child(
                                    div()
                                        .max_w(px(92.0))
                                        .font_family(MONO_FONT)
                                        .text_size(px(10.0))
                                        .text_color(theme.text_disabled)
                                        .whitespace_nowrap()
                                        .overflow_hidden()
                                        .child(project.path.to_string_lossy().into_owned()),
                                ),
                        )
                        .child(
                            div()
                                .mx_2()
                                .flex_shrink_0()
                                .font_family(MONO_FONT)
                                .text_size(px(10.0))
                                .text_color(type_color)
                                .child(project.project_type.label()),
                        )
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.select_project(index, window, cx);
                        }))
                });

        let project_list = match &self.scan.state {
            ScanState::Scanning => message_panel(
                "SCANNING",
                "Filesystem discovery is running in the background.",
                theme.warning,
                theme,
            ),
            ScanState::Error(error) => {
                message_panel("SCAN FAILED", error.clone(), theme.error, theme)
            }
            ScanState::Empty => message_panel(
                "NO PROJECTS",
                "No projects were found in this folder.",
                theme.text_muted,
                theme,
            ),
            _ if self.scan.projects.is_empty() => message_panel(
                "NO FIXTURES",
                "No fixture projects are available.",
                theme.text_muted,
                theme,
            ),
            _ => div().flex().flex_col().children(project_rows),
        }
        .id("project-list")
        .track_scroll(&self.project_scroll)
        .min_h_0()
        .flex_1()
        .overflow_y_scroll();

        div()
            .id("app")
            .track_focus(&self.focus_handle)
            .key_context("DevHub")
            .on_action(cx.listener(Self::select_previous_project))
            .on_action(cx.listener(Self::select_next_project))
            .on_action(cx.listener(Self::select_first_project))
            .on_action(cx.listener(Self::select_last_project))
            .on_action(cx.listener(Self::scan_current_folder))
            .size_full()
            .flex()
            .flex_col()
            .border_1()
            .border_color(theme.border)
            .bg(theme.app_background)
            .font_family(UI_FONT)
            .text_size(px(13.0))
            .text_color(theme.text)
            .child(
                div()
                    .h(px(TITLEBAR_HEIGHT))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .border_b_1()
                    .border_color(theme.border)
                    .bg(titlebar_background)
                    .child(
                        div()
                            .id("window-drag-region")
                            .min_w_0()
                            .h_full()
                            .flex_1()
                            .flex()
                            .items_center()
                            .gap_2()
                            .px_2()
                            .window_control_area(WindowControlArea::Drag)
                            .on_mouse_down(MouseButton::Left, |event, window, _| {
                                if event.click_count == 2 {
                                    window.zoom_window();
                                } else {
                                    begin_window_drag(window);
                                }
                            })
                            .child(div().size(px(6.0)).rounded_full().bg(if window_active {
                                theme.accent
                            } else {
                                theme.text_disabled
                            }))
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(if window_active {
                                        theme.text
                                    } else {
                                        theme.text_muted
                                    })
                                    .child("devhub"),
                            )
                            .child(div().text_color(theme.text_disabled).child("/"))
                            .child(
                                div()
                                    .min_w_0()
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .text_size(px(11.0))
                                    .text_color(theme.text_muted)
                                    .child("local projects"),
                            ),
                    )
                    .child(
                        div()
                            .id("scan-current-folder")
                            .h(px(23.0))
                            .px_2()
                            .flex()
                            .items_center()
                            .border_1()
                            .border_color(theme.border_strong)
                            .bg(theme.surface_background)
                            .font_family(MONO_FONT)
                            .text_size(px(10.0))
                            .text_color(theme.text_muted)
                            .cursor_pointer()
                            .hover(move |style| {
                                style
                                    .bg(theme.surface_hover)
                                    .border_color(theme.text_disabled)
                                    .text_color(theme.text)
                            })
                            .active(move |style| style.bg(theme.surface_selected))
                            .child(scan_button_label)
                            .on_click(cx.listener(Self::start_scan)),
                    )
                    .child(
                        div()
                            .h_full()
                            .ml_1()
                            .flex()
                            .child(window_control(
                                "window-minimize",
                                "−",
                                WindowCommand::Minimize,
                                window_active,
                                theme,
                            ))
                            .child(window_control(
                                "window-maximize",
                                if window_maximized { "❐" } else { "□" },
                                WindowCommand::Maximize,
                                window_active,
                                theme,
                            ))
                            .child(window_control(
                                "window-close",
                                "×",
                                WindowCommand::Close,
                                window_active,
                                theme,
                            )),
                    ),
            )
            .child(
                div()
                    .min_h_0()
                    .flex_1()
                    .flex()
                    .child(
                        div()
                            .w(px(SIDEBAR_WIDTH))
                            .flex_shrink_0()
                            .flex()
                            .flex_col()
                            .border_r_1()
                            .border_color(theme.border)
                            .bg(theme.sidebar_background)
                            .child(
                                div()
                                    .h(px(28.0))
                                    .flex_shrink_0()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .px_2()
                                    .border_b_1()
                                    .border_color(theme.border)
                                    .child(section_label("PROJECTS", theme))
                                    .child(
                                        div()
                                            .font_family(MONO_FONT)
                                            .text_size(px(10.0))
                                            .text_color(theme.text_disabled)
                                            .child(project_count.to_string()),
                                    ),
                            )
                            .child(project_list),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .bg(theme.panel_background)
                            .child(details_panel(selected_project, theme)),
                    ),
            )
            .child(
                div()
                    .h(px(22.0))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_2()
                    .border_t_1()
                    .border_color(theme.border)
                    .bg(theme.titlebar_background)
                    .text_size(px(10.0))
                    .child(
                        div()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(div().size(px(5.0)).rounded_full().bg(status_color))
                            .child(
                                div()
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .text_color(theme.text_muted)
                                    .child(scan_status),
                            ),
                    )
                    .child(
                        div()
                            .ml_3()
                            .flex()
                            .items_center()
                            .gap_3()
                            .font_family(MONO_FONT)
                            .text_color(theme.text_disabled)
                            .child("↑↓ select  ·  Ctrl+R scan")
                            .child(
                                div()
                                    .max_w(px(260.0))
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .child(self.scan_root.to_string_lossy().into_owned()),
                            ),
                    ),
            )
    }
}

fn window_control(
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
            theme.text_muted
        } else {
            theme.text_disabled
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
            WindowCommand::Maximize => window.zoom_window(),
            WindowCommand::Close => window.remove_window(),
        })
}

fn details_panel(project: Option<Project>, theme: Theme) -> Div {
    match project {
        Some(project) => div()
            .size_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .h(px(42.0))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_3()
                    .border_b_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .min_w_0()
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_size(px(15.0))
                            .text_color(theme.text)
                            .child(project.name.clone()),
                    )
                    .child(
                        div()
                            .ml_3()
                            .flex()
                            .items_center()
                            .gap_2()
                            .font_family(MONO_FONT)
                            .text_size(px(10.0))
                            .text_color(theme.text_disabled)
                            .child(
                                div()
                                    .text_color(project_type_color(theme, project.project_type))
                                    .child(project.project_type.label()),
                            )
                            .child("·")
                            .child(project.source.label().to_string())
                            .child("·")
                            .child(if project.has_git { "git" } else { "no git" }),
                    ),
            )
            .child(
                div()
                    .min_h_0()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .px_3()
                    .pt_3()
                    .child(section_label("OVERVIEW", theme))
                    .child(
                        div()
                            .mt_2()
                            .mb_3()
                            .text_size(px(12.0))
                            .text_color(theme.text_muted)
                            .child(project_summary(project.project_type)),
                    )
                    .child(metadata_row(
                        "PATH",
                        project.path.to_string_lossy().into_owned(),
                        theme,
                    ))
                    .child(metadata_row(
                        "MARKERS",
                        if project.markers_found.is_empty() {
                            "none".to_string()
                        } else {
                            project.markers_found.join(", ")
                        },
                        theme,
                    ))
                    .child(metadata_row(
                        "REMOTE",
                        project
                            .git_remote
                            .clone()
                            .unwrap_or_else(|| "not configured".to_string()),
                        theme,
                    )),
            ),
        None => div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_1()
            .font_family(MONO_FONT)
            .text_size(px(11.0))
            .text_color(theme.text_disabled)
            .child(div().text_size(px(19.0)).child("<>"))
            .child("select a project"),
    }
}

fn message_panel(
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

fn section_label(label: &'static str, theme: Theme) -> Div {
    div()
        .font_family(MONO_FONT)
        .text_size(px(10.0))
        .text_color(theme.text_disabled)
        .child(label)
}

fn metadata_row(label: &'static str, value: String, theme: Theme) -> Div {
    div()
        .h(px(32.0))
        .flex_shrink_0()
        .flex()
        .items_center()
        .border_t_1()
        .border_color(theme.border)
        .child(div().w(px(68.0)).child(section_label(label, theme)))
        .child(
            div()
                .min_w_0()
                .flex_1()
                .font_family(MONO_FONT)
                .text_size(px(11.0))
                .text_color(theme.text_muted)
                .whitespace_nowrap()
                .overflow_hidden()
                .child(value),
        )
}

fn scan_status(state: &ScanState, root: &std::path::Path) -> String {
    match state {
        ScanState::Idle => "Fixture data; current folder has not been scanned.".to_string(),
        ScanState::Scanning => format!("Scanning {}...", root.display()),
        ScanState::Loaded { count } => format!("Loaded {count} project(s)."),
        ScanState::Empty => "Scan completed with no projects.".to_string(),
        ScanState::Error(_) => "Scan failed. Review the message above.".to_string(),
    }
}

fn scan_state_color(theme: Theme, state: &ScanState) -> Hsla {
    match state {
        ScanState::Idle => theme.text_disabled,
        ScanState::Scanning => theme.warning,
        ScanState::Loaded { .. } => theme.success,
        ScanState::Empty => theme.text_muted,
        ScanState::Error(_) => theme.error,
    }
}

fn project_type_color(theme: Theme, project_type: ProjectType) -> Hsla {
    match project_type {
        ProjectType::Rust => theme.warning,
        ProjectType::Node => theme.success,
        ProjectType::Go => theme.accent,
        ProjectType::Python => theme.focus,
        ProjectType::Assembly => theme.text,
        _ => theme.text_muted,
    }
}

fn fixture_project(name: &str, path: &str, project_type: ProjectType, marker: &str) -> Project {
    let mut project = Project {
        name: name.to_string(),
        path: PathBuf::from(path),
        source: ProjectSource::Local,
        project_type,
        has_git: false,
        git_remote: None,
        markers_found: vec![marker.to_string()],
        last_modified: None,
        search_key: String::new(),
    };
    project.refresh_search_key();
    project
}

fn project_summary(project_type: ProjectType) -> &'static str {
    match project_type {
        ProjectType::Rust => "Rust project detected from its Cargo manifest.",
        ProjectType::Node => "Node project detected from its package manifest.",
        _ => "Project represented by the shared DevHub core model.",
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        cx.bind_keys([
            KeyBinding::new("up", SelectPreviousProject, Some("DevHub")),
            KeyBinding::new("down", SelectNextProject, Some("DevHub")),
            KeyBinding::new("home", SelectFirstProject, Some("DevHub")),
            KeyBinding::new("end", SelectLastProject, Some("DevHub")),
            KeyBinding::new("ctrl-r", ScanCurrentFolder, Some("DevHub")),
        ]);

        let bounds = Bounds::centered(None, size(px(900.0), px(600.0)), cx);

        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(680.0), px(440.0))),
                titlebar: Some(TitlebarOptions {
                    title: Some("DevHub".into()),
                    appears_transparent: true,
                    traffic_light_position: None,
                }),
                app_id: Some("devhub-gpui".to_string()),
                ..Default::default()
            },
            |window, cx| {
                configure_windows_surface(window);
                cx.new(|cx| DevHubLite::new(window, cx))
            },
        )
        .expect("failed to open the main window");

        cx.activate(true);
    });
}
