use crate::platform::{begin_window_drag, configure_windows_surface, open_with_picker};
use crate::ui::{
    file_icon, message_panel, project_type_color, scan_state_color, scan_status, section_label,
    window_control, workbench_message,
};
use devhub_core::{
    list_local_subdirs, list_project_tree, list_remote_subdirs, load_projects, local_roots,
    open_remote_in_vscode, read_project_file, read_project_readme, save_projects, scan_directories,
    scan_remote_host, search_project_content, sort_projects, validate_remote_path,
    validate_ssh_host, Config, DirectoryEntry, FileEntry, Project, ProjectSource, ProjectType,
    RemoteHostConfig, SearchHit, TreeListing,
};
use devhub_gpui::{
    language_for_path, markdown_fenced_source, next_selection, previous_selection,
    sanitize_markdown_images, ScanModel, ScanState, Theme, MONO_FONT, UI_FONT,
};
use gpui::prelude::*;
use gpui::*;
use gpui_component::highlighter::HighlightTheme;
use gpui_component::input::{Input, InputState, Position};
use gpui_component::text::{TextView, TextViewStyle};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

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
        ScanCurrentFolder,
        OpenSelectedProject
    ]
);

struct DevHubLite {
    scan: ScanModel,
    selected: Option<usize>,
    filter_query: String,
    launch_error: Option<String>,
    scan_root: PathBuf,
    config: Config,
    focus_handle: FocusHandle,
    project_scroll: ScrollHandle,
    show_settings: bool,
    pending_scan_dirs: Vec<PathBuf>,
    pending_remote_hosts: Vec<RemoteHostConfig>,
    pending_max_depth: usize,
    source_picker: SourcePicker,
    picker_entries: LoadState<Vec<DirectoryEntry>>,
    picker_generation: u64,
    remote_name_input: Entity<InputState>,
    remote_host_input: Entity<InputState>,
    remote_path_input: Entity<InputState>,
    details_tab: DetailsTab,
    tree_state: LoadState<TreeListing>,
    tree_generation: u64,
    expanded_dirs: HashSet<PathBuf>,
    selected_file: Option<PathBuf>,
    file_state: LoadState<String>,
    file_generation: u64,
    document_focused: bool,
    wrap_document: bool,
    pending_document_line: Option<usize>,
    copy_feedback: Option<String>,
    copy_feedback_generation: u64,
    search_query: String,
    search_state: LoadState<Vec<SearchHit>>,
    search_generation: u64,
    show_hidden: bool,
    readme_state: LoadState<String>,
    readme_generation: u64,
    readme_preview: bool,
    tree_width_px: f32,
    dragging_split: bool,
}

#[derive(Clone, Copy)]
pub(crate) enum WindowCommand {
    Minimize,
    Maximize,
    Close,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DetailsTab {
    Overview,
    Files,
    Search,
}

enum LoadState<T> {
    Idle,
    Loading,
    Loaded(T),
    Empty,
    Error(String),
}

#[derive(Clone)]
enum SourcePicker {
    Closed,
    Local { current: PathBuf },
    Remote { host_index: usize, current: String },
}

impl DevHubLite {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let remote_name_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Name (optional)"));
        let remote_host_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("user@host or SSH alias"));
        let remote_path_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder("Remote path, for example /home/user")
        });
        window.focus(&focus_handle);

        let mut startup_errors = Vec::new();
        if let Err(error) = Config::ensure_dirs_exist() {
            startup_errors.push(error);
        }
        let config = match Config::load_or_create() {
            Ok(config) => config,
            Err(error) => {
                startup_errors.push(error);
                Config::default()
            }
        };

        let scan_root = config
            .scan_dirs
            .first()
            .cloned()
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."));

        let fixtures = || {
            vec![
                fixture_project(
                    "devhub",
                    r"F:\_Engine\devhub",
                    ProjectType::Rust,
                    "Cargo.toml",
                ),
                fixture_project(
                    "devhub-gpui",
                    r"F:\_Engine\devhub-gpui",
                    ProjectType::Rust,
                    "Cargo.toml",
                ),
                fixture_project(
                    "echopoint",
                    r"F:\_Engine\echopoint",
                    ProjectType::Node,
                    "package.json",
                ),
            ]
        };
        let initial_projects = match load_projects() {
            Ok(Some(projects)) => projects,
            Ok(None) => fixtures(),
            Err(error) => {
                startup_errors.push(error);
                fixtures()
            }
        };

        Self {
            scan: ScanModel::new(initial_projects),
            selected: None,
            filter_query: String::new(),
            launch_error: (!startup_errors.is_empty()).then(|| startup_errors.join(" | ")),
            scan_root,
            config,
            focus_handle,
            project_scroll: ScrollHandle::new(),
            show_settings: false,
            pending_scan_dirs: Vec::new(),
            pending_remote_hosts: Vec::new(),
            pending_max_depth: 3,
            source_picker: SourcePicker::Closed,
            picker_entries: LoadState::Idle,
            picker_generation: 0,
            remote_name_input,
            remote_host_input,
            remote_path_input,
            details_tab: DetailsTab::Overview,
            tree_state: LoadState::Idle,
            tree_generation: 0,
            expanded_dirs: HashSet::new(),
            selected_file: None,
            file_state: LoadState::Idle,
            file_generation: 0,
            document_focused: false,
            wrap_document: false,
            pending_document_line: None,
            copy_feedback: None,
            copy_feedback_generation: 0,
            search_query: String::new(),
            search_state: LoadState::Idle,
            search_generation: 0,
            show_hidden: false,
            readme_state: LoadState::Idle,
            readme_generation: 0,
            readme_preview: true,
            tree_width_px: 130.0,
            dragging_split: false,
        }
    }

    fn select_project(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus_handle);
        self.set_selected(index, cx);
        self.reset_workspace(cx);
        self.load_readme(cx);
    }

    fn reset_workspace(&mut self, cx: &mut Context<Self>) {
        self.details_tab = DetailsTab::Overview;
        self.tree_generation = self.tree_generation.wrapping_add(1);
        self.tree_state = LoadState::Idle;
        self.expanded_dirs.clear();
        self.selected_file = None;
        self.file_generation = self.file_generation.wrapping_add(1);
        self.file_state = LoadState::Idle;
        self.document_focused = false;
        self.pending_document_line = None;
        self.copy_feedback = None;
        self.search_query.clear();
        self.search_generation = self.search_generation.wrapping_add(1);
        self.search_state = LoadState::Idle;
        self.readme_generation = self.readme_generation.wrapping_add(1);
        self.readme_state = LoadState::Idle;
        cx.notify();
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
        let remote_hosts = self.config.remote_hosts.clone();
        let roots = if self.config.scan_dirs.is_empty() && remote_hosts.is_empty() {
            vec![self.scan_root.clone()]
        } else {
            self.config.scan_dirs.clone()
        };
        let max_depth = self.config.max_depth;

        self.selected = None;
        self.filter_query.clear();
        self.launch_error = None;
        cx.notify();

        let scan_task = cx.background_executor().spawn(async move {
            let mut invalid: Option<String> = None;
            for root in &roots {
                match std::fs::metadata(root) {
                    Ok(metadata) if !metadata.is_dir() => {
                        invalid = Some(format!("Scan root is not a directory: {}", root.display()));
                        break;
                    }
                    Err(error) => {
                        invalid = Some(format!("Cannot scan {}: {error}", root.display()));
                        break;
                    }
                    Ok(_) => {}
                }
            }
            if let Some(message) = invalid {
                return Err(message);
            }

            let mut projects = scan_directories(&roots, max_depth);
            let mut errors = Vec::new();
            for host in remote_hosts {
                match scan_remote_host(&host) {
                    Ok(mut remote_projects) => projects.append(&mut remote_projects),
                    Err(error) => errors.push(format!("{}: {error}", host.label())),
                }
            }
            sort_projects(&mut projects);
            Ok((projects, errors))
        });

        cx.spawn(async move |this, cx| {
            let result = scan_task.await;
            let _ = this.update(cx, |this, cx| {
                let (result, remote_errors) = match result {
                    Ok((projects, errors)) => (Ok(projects), errors),
                    Err(error) => (Err(error), Vec::new()),
                };
                if this.scan.apply_result(generation, result) {
                    this.selected = None;
                    this.filter_query.clear();
                    this.launch_error =
                        (!remote_errors.is_empty()).then(|| remote_errors.join(" | "));
                    if let ScanState::Loaded { .. } = this.scan.state {
                        if let Err(error) = save_projects(&this.scan.projects) {
                            this.launch_error = Some(error);
                        }
                    }
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

    fn filtered_indices(&self) -> Vec<usize> {
        if self.filter_query.is_empty() {
            (0..self.scan.projects.len()).collect()
        } else {
            self.scan
                .projects
                .iter()
                .enumerate()
                .filter(|(_, project)| project.search_key.contains(&self.filter_query))
                .map(|(index, _)| index)
                .collect()
        }
    }

    fn selected_project(&self) -> Option<&Project> {
        let filtered = self.filtered_indices();
        self.selected
            .and_then(|i| filtered.get(i))
            .and_then(|&idx| self.scan.projects.get(idx))
    }

    fn clamp_selection_to_filter(&mut self, cx: &mut Context<Self>) {
        let filtered_count = self.filtered_indices().len();
        match self.selected {
            Some(index) if index < filtered_count => {}
            Some(_) => {
                self.selected = None;
            }
            None => {
                if filtered_count > 0 {
                    self.selected = Some(0);
                    self.project_scroll.scroll_to_item(0);
                }
            }
        }
        cx.notify();
    }

    fn handle_filter_keydown(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.details_tab == DetailsTab::Search {
            self.handle_search_keydown(event, window, cx);
            return;
        }
        let key = event.keystroke.key.as_str();
        let mods = &event.keystroke.modifiers;

        if self.show_settings {
            if key == "escape" {
                if matches!(self.source_picker, SourcePicker::Closed) {
                    self.show_settings = false;
                    self.pending_scan_dirs.clear();
                    self.pending_remote_hosts.clear();
                    self.pending_max_depth = 3;
                } else {
                    self.source_picker = SourcePicker::Closed;
                    self.picker_generation = self.picker_generation.wrapping_add(1);
                    self.picker_entries = LoadState::Idle;
                }
                cx.notify();
            }
            return;
        }

        if self.document_focused && self.details_tab == DetailsTab::Files {
            if key == "escape" {
                self.document_focused = false;
                cx.notify();
                return;
            }
            if !mods.control && !mods.alt && !mods.function {
                return;
            }
        }

        if key == "escape" {
            if !self.filter_query.is_empty() {
                self.filter_query.clear();
                self.selected = None;
                cx.notify();
            }
            return;
        }

        if key == "backspace" {
            if !self.filter_query.is_empty() {
                self.filter_query.pop();
                self.clamp_selection_to_filter(cx);
            }
            return;
        }

        if key == "enter" {
            return;
        }

        if mods.control || mods.alt || mods.function {
            return;
        }

        let candidate = event.keystroke.key_char.as_deref().unwrap_or(key);

        if candidate.chars().count() == 1 {
            let ch = candidate.chars().next().unwrap();
            if ch.is_ascii_graphic() || ch == ' ' {
                self.filter_query.push(ch.to_ascii_lowercase());
                self.clamp_selection_to_filter(cx);
            }
        }
    }

    fn copy_document_selection(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        if let LoadState::Loaded(document) = &self.file_state {
            cx.write_to_clipboard(ClipboardItem::new_string(document.clone()));
            self.show_copy_feedback("COPIED FILE", cx);
        }
    }

    fn note_component_copy(
        &mut self,
        _: &gpui_component::input::Copy,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show_copy_feedback("COPIED SELECTION", cx);
    }

    fn show_copy_feedback(&mut self, message: &str, cx: &mut Context<Self>) {
        self.copy_feedback_generation = self.copy_feedback_generation.wrapping_add(1);
        let generation = self.copy_feedback_generation;
        self.copy_feedback = Some(message.to_string());
        cx.notify();

        let timer = cx.background_executor().timer(Duration::from_millis(1_500));
        cx.spawn(async move |this, cx| {
            timer.await;
            let _ = this.update(cx, |this, cx| {
                if this.copy_feedback_generation == generation {
                    this.copy_feedback = None;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn toggle_readme_preview(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.readme_preview = !self.readme_preview;
        cx.notify();
    }

    fn open_selected_project(
        &mut self,
        _: &OpenSelectedProject,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.filter_query.is_empty() || self.details_tab == DetailsTab::Search {
            return;
        }

        let Some(project) = self.selected_project().cloned() else {
            return;
        };

        self.launch_error = if project.source.is_remote() {
            open_remote_in_vscode(&project).err()
        } else {
            open_with_picker(&project.path, window);
            None
        };
        cx.notify();
    }

    fn toggle_settings(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.show_settings {
            self.show_settings = false;
            self.pending_scan_dirs.clear();
            self.pending_remote_hosts.clear();
            self.pending_max_depth = 3;
            self.source_picker = SourcePicker::Closed;
        } else {
            self.show_settings = true;
            self.pending_scan_dirs = self.config.scan_dirs.clone();
            self.pending_remote_hosts = self.config.remote_hosts.clone();
            self.pending_max_depth = self.config.max_depth;
            self.source_picker = SourcePicker::Closed;
            window.focus(&self.focus_handle);
        }
        cx.notify();
    }

    fn open_local_picker(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        let start = self
            .pending_scan_dirs
            .last()
            .cloned()
            .filter(|path| path.is_dir())
            .unwrap_or_else(|| self.scan_root.clone());
        self.load_local_picker(start, cx);
    }

    fn load_local_picker(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.picker_generation = self.picker_generation.wrapping_add(1);
        let generation = self.picker_generation;
        self.source_picker = SourcePicker::Local {
            current: path.clone(),
        };
        self.picker_entries = LoadState::Loading;
        cx.notify();

        let task = cx
            .background_executor()
            .spawn(async move { list_local_subdirs(&path) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.picker_generation != generation {
                    return;
                }
                this.picker_entries = match result {
                    Ok(entries) if entries.is_empty() => LoadState::Empty,
                    Ok(entries) => LoadState::Loaded(entries),
                    Err(error) => LoadState::Error(error),
                };
                cx.notify();
            });
        })
        .detach();
    }

    fn show_local_roots(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.picker_generation = self.picker_generation.wrapping_add(1);
        self.source_picker = SourcePicker::Local {
            current: PathBuf::new(),
        };
        let roots = local_roots();
        self.picker_entries = if roots.is_empty() {
            LoadState::Empty
        } else {
            LoadState::Loaded(roots)
        };
        cx.notify();
    }

    fn add_remote_host(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.remote_name_input.read(cx).value().to_string();
        let raw_host = self.remote_host_input.read(cx).value().to_string();
        match validate_ssh_host(&raw_host) {
            Ok(host) => {
                let index = if let Some(index) = self
                    .pending_remote_hosts
                    .iter()
                    .position(|candidate| candidate.host == host)
                {
                    if !name.trim().is_empty() {
                        self.pending_remote_hosts[index].name = name.trim().to_string();
                    }
                    index
                } else {
                        self.pending_remote_hosts.push(RemoteHostConfig {
                            name: name.trim().to_string(),
                            host,
                            roots: Vec::new(),
                            max_depth: self.pending_max_depth,
                        });
                        self.pending_remote_hosts.len() - 1
                };
                self.remote_name_input.update(cx, |input, cx| {
                    input.set_value("", window, cx);
                });
                self.remote_host_input.update(cx, |input, cx| {
                    input.set_value("", window, cx);
                });
                self.load_remote_picker(index, "/".into(), window, cx);
            }
            Err(error) => {
                self.launch_error = Some(error);
                cx.notify();
            }
        }
    }

    fn open_remote_picker(
        &mut self,
        index: usize,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(host) = self.pending_remote_hosts.get(index) else {
            return;
        };
        let start = host.roots.last().cloned().unwrap_or_else(|| "/".into());
        self.load_remote_picker(index, start, window, cx);
    }

    fn load_remote_picker(
        &mut self,
        host_index: usize,
        path: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(host) = self.pending_remote_hosts.get(host_index) else {
            return;
        };
        let host = host.host.clone();
        let path = match validate_remote_path(&path) {
            Ok(path) => path,
            Err(error) => {
                self.launch_error = Some(error);
                cx.notify();
                return;
            }
        };
        self.remote_path_input.update(cx, |input, cx| {
            input.set_value(path.clone(), window, cx);
        });
        self.picker_generation = self.picker_generation.wrapping_add(1);
        let generation = self.picker_generation;
        self.source_picker = SourcePicker::Remote {
            host_index,
            current: path.clone(),
        };
        self.picker_entries = LoadState::Loading;
        self.launch_error = None;
        cx.notify();

        let task = cx
            .background_executor()
            .spawn(async move { list_remote_subdirs(&host, &path) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.picker_generation != generation {
                    return;
                }
                this.picker_entries = match result {
                    Ok(entries) if entries.is_empty() => LoadState::Empty,
                    Ok(entries) => LoadState::Loaded(entries),
                    Err(error) => LoadState::Error(error),
                };
                cx.notify();
            });
        })
        .detach();
    }

    fn go_remote_path(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        let SourcePicker::Remote { host_index, .. } = self.source_picker else {
            return;
        };
        let path = self.remote_path_input.read(cx).value().to_string();
        self.load_remote_picker(host_index, path, window, cx);
    }

    fn close_source_picker(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.picker_generation = self.picker_generation.wrapping_add(1);
        self.source_picker = SourcePicker::Closed;
        self.picker_entries = LoadState::Idle;
        cx.notify();
    }

    fn add_current_source(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        match &self.source_picker {
            SourcePicker::Local { current } if !current.as_os_str().is_empty() => {
                if !self.pending_scan_dirs.contains(current) {
                    self.pending_scan_dirs.push(current.clone());
                }
            }
            SourcePicker::Remote {
                host_index,
                current,
            } => {
                if let Some(host) = self.pending_remote_hosts.get_mut(*host_index) {
                    if !host.roots.contains(current) {
                        host.roots.push(current.clone());
                    }
                }
            }
            _ => return,
        }
        self.source_picker = SourcePicker::Closed;
        self.picker_entries = LoadState::Idle;
        cx.notify();
    }

    fn remove_scan_dir(
        &mut self,
        index: usize,
        _: &ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if index < self.pending_scan_dirs.len() {
            self.pending_scan_dirs.remove(index);
            cx.notify();
        }
    }

    fn remove_remote_host(
        &mut self,
        index: usize,
        _: &ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if index < self.pending_remote_hosts.len() {
            self.pending_remote_hosts.remove(index);
            cx.notify();
        }
    }

    fn remove_remote_root(
        &mut self,
        host_index: usize,
        root_index: usize,
        _: &ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(host) = self.pending_remote_hosts.get_mut(host_index) {
            if root_index < host.roots.len() {
                host.roots.remove(root_index);
                cx.notify();
            }
        }
    }

    fn increase_max_depth(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.pending_max_depth < 20 {
            self.pending_max_depth += 1;
            cx.notify();
        }
    }

    fn decrease_max_depth(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.pending_max_depth > 1 {
            self.pending_max_depth -= 1;
            cx.notify();
        }
    }

    fn save_settings(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        let mut config = self.config.clone();
        config.scan_dirs = self.pending_scan_dirs.clone();
        config.remote_hosts = self.pending_remote_hosts.clone();
        config.max_depth = self.pending_max_depth;
        for host in &mut config.remote_hosts {
            host.max_depth = self.pending_max_depth;
        }
        config.normalize();
        match config.save() {
            Ok(()) => {
                self.config = config;
                self.show_settings = false;
                self.begin_scan(cx);
            }
            Err(error) => self.launch_error = Some(error),
        }
        cx.notify();
    }

    fn cancel_settings(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.show_settings = false;
        self.pending_scan_dirs.clear();
        self.pending_remote_hosts.clear();
        self.pending_max_depth = 3;
        self.source_picker = SourcePicker::Closed;
        cx.notify();
    }

    fn settings_panel(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        use gpui::{InteractiveElement, StatefulInteractiveElement};
        if !matches!(self.source_picker, SourcePicker::Closed) {
            return self.source_picker_panel(theme, cx);
        }

        div()
            .min_h_0()
            .flex_1()
            .flex()
            .flex_col()
            .bg(theme.panel_background)
            .child(
                div()
                    .h(px(28.0))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .px_2()
                    .border_b_1()
                    .border_color(theme.border)
                    .child(section_label("SETTINGS", theme)),
            )
            .child(
                div()
                    .id("settings-scroll")
                    .flex_1()
                    .overflow_y_scroll()
                    .px_3()
                    .py_2()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(section_label("SCAN DIRECTORIES", theme))
                    .children(self.pending_scan_dirs.iter().enumerate().map(|(i, dir)| {
                        div()
                            .id(("scan-dir-row", i))
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .flex_1()
                                    .font_family(MONO_FONT)
                                    .text_size(px(10.0))
                                    .text_color(theme.text_muted)
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .child(dir.to_string_lossy().into_owned()),
                            )
                            .child(
                                div()
                                    .id(("remove-dir", i))
                                    .flex_shrink_0()
                                    .cursor_pointer()
                                    .text_color(theme.error)
                                    .hover(move |style| style.text_color(theme.text))
                                    .child("\u{00d7}")
                                    .on_click(cx.listener(move |this, event, window, cx| {
                                        this.remove_scan_dir(i, event, window, cx);
                                    })),
                            )
                    }))
                    .child(
                        div()
                            .id("add-scan-dir")
                            .mt_1()
                            .cursor_pointer()
                            .text_size(px(11.0))
                            .text_color(theme.accent)
                            .hover(move |style| style.text_color(theme.text))
                            .child("+ Browse local folder")
                            .on_click(cx.listener(Self::open_local_picker)),
                    )
                    .child(div().h(px(12.0)))
                    .child(section_label("SSH SOURCES", theme))
                    .children(self.pending_remote_hosts.iter().enumerate().map(
                        |(host_index, host)| {
                            let host_label = format!("{}  ({})", host.label(), host.host);
                            div()
                                .border_1()
                                .border_color(theme.border)
                                .bg(theme.surface_background)
                                .p_2()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .justify_between()
                                        .child(
                                            div()
                                                .font_family(MONO_FONT)
                                                .text_size(px(10.0))
                                                .text_color(theme.text)
                                                .child(host_label),
                                        )
                                        .child(
                                            div()
                                                .id(("remove-ssh-host", host_index))
                                                .cursor_pointer()
                                                .text_color(theme.error)
                                                .child("\u{00d7}")
                                                .on_click(cx.listener(
                                                    move |this, event, window, cx| {
                                                        this.remove_remote_host(
                                                            host_index, event, window, cx,
                                                        );
                                                    },
                                                )),
                                        ),
                                )
                                .children(host.roots.iter().enumerate().map(
                                    |(root_index, root)| {
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_2()
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .font_family(MONO_FONT)
                                                    .text_size(px(10.0))
                                                    .text_color(theme.text_muted)
                                                    .child(root.clone()),
                                            )
                                            .child(
                                                div()
                                                    .id(SharedString::from(format!(
                                                        "remove-ssh-root-{host_index}-{root_index}"
                                                    )))
                                                    .cursor_pointer()
                                                    .text_color(theme.error)
                                                    .child("\u{00d7}")
                                                    .on_click(cx.listener(
                                                        move |this, event, window, cx| {
                                                            this.remove_remote_root(
                                                                host_index,
                                                                root_index,
                                                                event,
                                                                window,
                                                                cx,
                                                            );
                                                        },
                                                    )),
                                            )
                                    },
                                ))
                                .child(
                                    div()
                                        .id(("browse-ssh", host_index))
                                        .cursor_pointer()
                                        .text_size(px(10.0))
                                        .text_color(theme.accent)
                                        .child("+ Browse remote folder")
                                        .on_click(cx.listener(
                                            move |this, event, window, cx| {
                                                this.open_remote_picker(
                                                    host_index,
                                                    event,
                                                    window,
                                                    cx,
                                                );
                                            },
                                        )),
                                )
                        },
                    ))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .w(px(180.0))
                                    .child(Input::new(&self.remote_name_input).h(px(28.0))),
                            )
                            .child(
                                div()
                                    .w(px(260.0))
                                    .child(Input::new(&self.remote_host_input).h(px(28.0))),
                            )
                            .child(
                                div()
                                    .id("add-ssh-host")
                                    .px_2()
                                    .py_1()
                                    .border_1()
                                    .border_color(theme.accent)
                                    .cursor_pointer()
                                    .text_size(px(10.0))
                                    .text_color(theme.accent)
                                    .child("Add && browse")
                                    .on_click(cx.listener(Self::add_remote_host)),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(9.0))
                            .text_color(theme.text_disabled)
                            .child(
                                "SSH uses your OpenSSH config and keys in BatchMode; remote hosts must provide a POSIX sh environment.",
                            ),
                    )
                    .child(div().h(px(12.0)))
                    .child(section_label("MAX SCAN DEPTH", theme))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .id("decrease-depth")
                                    .cursor_pointer()
                                    .text_size(px(16.0))
                                    .text_color(theme.text_muted)
                                    .hover(move |style| style.text_color(theme.text))
                                    .child("\u{2212}")
                                    .on_click(cx.listener(Self::decrease_max_depth)),
                            )
                            .child(
                                div()
                                    .font_family(MONO_FONT)
                                    .text_size(px(16.0))
                                    .text_color(theme.text)
                                    .child(self.pending_max_depth.to_string()),
                            )
                            .child(
                                div()
                                    .id("increase-depth")
                                    .cursor_pointer()
                                    .text_size(px(16.0))
                                    .text_color(theme.text_muted)
                                    .hover(move |style| style.text_color(theme.text))
                                    .child("+")
                                    .on_click(cx.listener(Self::increase_max_depth)),
                            ),
                    )
                    .child(div().h(px(24.0)))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .id("settings-cancel")
                                    .px_3()
                                    .py_1()
                                    .border_1()
                                    .border_color(theme.border_strong)
                                    .text_color(theme.text)
                                    .text_size(px(11.0))
                                    .cursor_pointer()
                                    .hover(move |style| style.bg(theme.surface_hover))
                                    .child("Cancel")
                                    .on_click(cx.listener(Self::cancel_settings)),
                            )
                            .child(
                                div()
                                    .id("settings-save")
                                    .px_3()
                                    .py_1()
                                    .border_1()
                                    .border_color(theme.accent)
                                    .bg(theme.accent.opacity(0.15))
                                    .text_color(theme.accent)
                                    .text_size(px(11.0))
                                    .cursor_pointer()
                                    .hover(move |style| style.bg(theme.accent.opacity(0.25)))
                                    .child("Save && Rescan")
                                    .on_click(cx.listener(Self::save_settings)),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn source_picker_panel(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        use gpui::{InteractiveElement, StatefulInteractiveElement};

        let mode = self.source_picker.clone();
        let (title, location) = match &mode {
            SourcePicker::Closed => return div().into_any_element(),
            SourcePicker::Local { current } => (
                "LOCAL FOLDER PICKER",
                if current.as_os_str().is_empty() {
                    "Computer".to_string()
                } else {
                    current.display().to_string()
                },
            ),
            SourcePicker::Remote {
                host_index,
                current,
            } => (
                "SSH FOLDER PICKER",
                format!(
                    "{}:{}",
                    self.pending_remote_hosts
                        .get(*host_index)
                        .map(RemoteHostConfig::label)
                        .unwrap_or("unknown"),
                    current
                ),
            ),
        };

        let entries = match &self.picker_entries {
            LoadState::Loaded(entries) => entries.clone(),
            _ => Vec::new(),
        };
        let list: AnyElement = match &self.picker_entries {
            LoadState::Loading => workbench_message("loading folders...", theme.text_disabled),
            LoadState::Error(error) => workbench_message(error.clone(), theme.error),
            LoadState::Empty => {
                workbench_message("this folder has no subdirectories", theme.text_disabled)
            }
            LoadState::Idle => workbench_message("choose a location", theme.text_disabled),
            LoadState::Loaded(_) => div()
                .id("source-picker-list")
                .flex_1()
                .overflow_y_scroll()
                .children(entries.into_iter().enumerate().map(|(index, entry)| {
                    let target = entry.path.clone();
                    let row_mode = mode.clone();
                    div()
                        .id(("picker-entry", index))
                        .h(px(28.0))
                        .px_2()
                        .flex()
                        .items_center()
                        .gap_2()
                        .cursor_pointer()
                        .hover(move |style| style.bg(theme.surface_hover))
                        .child(
                            div()
                                .text_color(theme.accent)
                                .font_family(MONO_FONT)
                                .child("d"),
                        )
                        .child(
                            div()
                                .text_size(px(11.0))
                                .text_color(theme.text)
                                .child(entry.name),
                        )
                        .on_click(cx.listener(move |this, _, window, cx| match row_mode {
                            SourcePicker::Local { .. } => {
                                this.load_local_picker(PathBuf::from(&target), cx)
                            }
                            SourcePicker::Remote { host_index, .. } => {
                                this.load_remote_picker(host_index, target.clone(), window, cx)
                            }
                            SourcePicker::Closed => {}
                        }))
                }))
                .into_any_element(),
        };

        let parent_button: AnyElement = match &mode {
            SourcePicker::Local { current } if current.as_os_str().is_empty() => div()
                .id("picker-roots-current")
                .px_2()
                .py_1()
                .text_size(px(10.0))
                .text_color(theme.text_disabled)
                .child("DRIVES")
                .into_any_element(),
            SourcePicker::Local { current } => {
                let parent = current.parent().map(Path::to_path_buf);
                if let Some(parent) = parent {
                    div()
                        .id("picker-parent")
                        .px_2()
                        .py_1()
                        .cursor_pointer()
                        .text_size(px(10.0))
                        .text_color(theme.accent)
                        .child("\u{2191} Parent")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.load_local_picker(parent.clone(), cx);
                        }))
                        .into_any_element()
                } else {
                    div()
                        .id("picker-drives")
                        .px_2()
                        .py_1()
                        .cursor_pointer()
                        .text_size(px(10.0))
                        .text_color(theme.accent)
                        .child("Computer drives")
                        .on_click(cx.listener(Self::show_local_roots))
                        .into_any_element()
                }
            }
            SourcePicker::Remote {
                host_index,
                current,
            } => {
                let parent = remote_parent_path(current);
                let host_index = *host_index;
                if let Some(parent) = parent {
                    div()
                        .id("picker-remote-parent")
                        .px_2()
                        .py_1()
                        .cursor_pointer()
                        .text_size(px(10.0))
                        .text_color(theme.accent)
                        .child("\u{2191} Parent")
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.load_remote_picker(host_index, parent.clone(), window, cx);
                        }))
                        .into_any_element()
                } else {
                    div().into_any_element()
                }
            }
            SourcePicker::Closed => div().into_any_element(),
        };

        let remote_path: AnyElement = if matches!(mode, SourcePicker::Remote { .. }) {
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .flex_1()
                        .child(Input::new(&self.remote_path_input).h(px(28.0))),
                )
                .child(
                    div()
                        .id("picker-go-remote")
                        .px_2()
                        .py_1()
                        .border_1()
                        .border_color(theme.border_strong)
                        .cursor_pointer()
                        .text_size(px(10.0))
                        .text_color(theme.text)
                        .child("Go")
                        .on_click(cx.listener(Self::go_remote_path)),
                )
                .into_any_element()
        } else {
            div().into_any_element()
        };

        div()
            .min_h_0()
            .flex_1()
            .flex()
            .flex_col()
            .bg(theme.panel_background)
            .child(
                div()
                    .h(px(34.0))
                    .px_2()
                    .flex()
                    .items_center()
                    .justify_between()
                    .border_b_1()
                    .border_color(theme.border)
                    .child(section_label(title, theme))
                    .child(
                        div()
                            .id("picker-close")
                            .cursor_pointer()
                            .text_color(theme.text_muted)
                            .child("\u{00d7}")
                            .on_click(cx.listener(Self::close_source_picker)),
                    ),
            )
            .child(
                div()
                    .p_2()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .border_b_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .font_family(MONO_FONT)
                            .text_size(px(10.0))
                            .text_color(theme.text_muted)
                            .child(location),
                    )
                    .child(remote_path)
                    .child(parent_button),
            )
            .child(list)
            .child(
                div()
                    .h(px(42.0))
                    .px_2()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap_2()
                    .border_t_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .id("picker-cancel")
                            .px_3()
                            .py_1()
                            .border_1()
                            .border_color(theme.border_strong)
                            .cursor_pointer()
                            .text_size(px(11.0))
                            .text_color(theme.text)
                            .child("Cancel")
                            .on_click(cx.listener(Self::close_source_picker)),
                    )
                    .child(
                        div()
                            .id("picker-add-current")
                            .px_3()
                            .py_1()
                            .border_1()
                            .border_color(theme.accent)
                            .bg(theme.accent.opacity(0.15))
                            .cursor_pointer()
                            .text_size(px(11.0))
                            .text_color(theme.accent)
                            .child("Add this folder")
                            .on_click(cx.listener(Self::add_current_source)),
                    ),
            )
            .into_any_element()
    }

    fn select_previous_project(
        &mut self,
        _: &SelectPreviousProject,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.document_focused {
            return;
        }
        let count = self.filtered_indices().len();
        if let Some(index) = previous_selection(self.selected, count) {
            self.set_selected(index, cx);
        }
    }

    fn select_next_project(
        &mut self,
        _: &SelectNextProject,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.document_focused {
            return;
        }
        let count = self.filtered_indices().len();
        if let Some(index) = next_selection(self.selected, count) {
            self.set_selected(index, cx);
        }
    }

    fn select_first_project(
        &mut self,
        _: &SelectFirstProject,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.document_focused {
            return;
        }
        if !self.filtered_indices().is_empty() {
            self.set_selected(0, cx);
        }
    }

    fn select_last_project(
        &mut self,
        _: &SelectLastProject,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.document_focused {
            return;
        }
        let count = self.filtered_indices().len();
        if count > 0 {
            self.set_selected(count - 1, cx);
        }
    }

    fn select_tab(&mut self, tab: DetailsTab, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus_handle);
        self.details_tab = tab;
        match tab {
            DetailsTab::Files if matches!(self.tree_state, LoadState::Idle) => self.load_tree(cx),
            DetailsTab::Overview if matches!(self.readme_state, LoadState::Idle) => {
                self.load_readme(cx)
            }
            _ => {}
        }
        cx.notify();
    }

    fn load_tree(&mut self, cx: &mut Context<Self>) {
        let Some(project) = self.selected_project().cloned() else {
            return;
        };
        let show_hidden = self.show_hidden;
        self.tree_generation = self.tree_generation.wrapping_add(1);
        let generation = self.tree_generation;
        self.tree_state = LoadState::Loading;
        cx.notify();

        let request_project = project.clone();
        let task = cx
            .background_executor()
            .spawn(async move { list_project_tree(&project, 10, show_hidden) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                let project_matches = this.selected_project() == Some(&request_project);
                if this.tree_generation != generation
                    || this.show_hidden != show_hidden
                    || !project_matches
                {
                    return;
                }

                this.tree_state = match result {
                    Ok(listing) if listing.entries.is_empty() => LoadState::Empty,
                    Ok(listing) => LoadState::Loaded(listing),
                    Err(error) => LoadState::Error(error),
                };
                cx.notify();
            });
        })
        .detach();
    }

    fn toggle_hidden(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.show_hidden = !self.show_hidden;
        self.expanded_dirs.clear();
        self.load_tree(cx);
    }

    fn load_readme(&mut self, cx: &mut Context<Self>) {
        let Some(project) = self.selected_project().cloned() else {
            return;
        };
        self.readme_generation = self.readme_generation.wrapping_add(1);
        let generation = self.readme_generation;
        self.readme_state = LoadState::Loading;
        cx.notify();

        let request_project = project.clone();
        let task = cx
            .background_executor()
            .spawn(async move { read_project_readme(&project) });
        cx.spawn(async move |this, cx| {
            let content = task.await;
            let _ = this.update(cx, |this, cx| {
                let project_matches = this.selected_project() == Some(&request_project);
                if this.readme_generation != generation || !project_matches {
                    return;
                }
                this.readme_state = match content {
                    Ok(Some(content)) if content.trim().is_empty() => LoadState::Empty,
                    Ok(Some(content)) => LoadState::Loaded(content),
                    Ok(None) => LoadState::Empty,
                    Err(error) => LoadState::Error(error),
                };
                cx.notify();
            });
        })
        .detach();
    }

    fn toggle_tree_entry(
        &mut self,
        index: usize,
        _: &ClickEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let LoadState::Loaded(listing) = &self.tree_state else {
            return;
        };
        let Some(entry) = listing.entries.get(index) else {
            return;
        };
        if entry.is_dir {
            if self.expanded_dirs.contains(&entry.path) {
                self.expanded_dirs.remove(&entry.path);
            } else {
                self.expanded_dirs.insert(entry.path.clone());
            }
            cx.notify();
        } else {
            self.selected_file = Some(entry.path.clone());
            self.pending_document_line = None;
            self.load_file_content(cx);
            cx.notify();
        }
    }

    fn load_file_content(&mut self, cx: &mut Context<Self>) {
        let Some(ref path) = self.selected_file.clone() else {
            return;
        };
        let path = path.clone();
        let Some(project) = self.selected_project().cloned() else {
            return;
        };
        self.file_generation = self.file_generation.wrapping_add(1);
        let generation = self.file_generation;
        self.file_state = LoadState::Loading;
        self.document_focused = true;
        self.copy_feedback = None;
        cx.notify();

        let request_path = path.clone();
        let request_project = project.clone();
        let task = cx
            .background_executor()
            .spawn(async move { read_project_file(&project, &path) });

        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.file_generation != generation
                    || this.selected_file.as_ref() != Some(&request_path)
                    || this.selected_project() != Some(&request_project)
                {
                    return;
                }
                this.file_state = match result {
                    Ok(content) if content.is_empty() => LoadState::Empty,
                    Ok(content) => LoadState::Loaded(content),
                    Err(error) => LoadState::Error(error),
                };
                cx.notify();
            });
        })
        .detach();
    }

    fn execute_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let query = self.search_query.trim().to_string();
        if query.is_empty() {
            return;
        }
        window.focus(&self.focus_handle);

        let Some(project) = self.selected_project().cloned() else {
            return;
        };

        self.search_generation = self.search_generation.wrapping_add(1);
        let generation = self.search_generation;
        self.search_state = LoadState::Loading;
        cx.notify();

        let request_project = project.clone();
        let request_query = query.clone();
        let task = cx
            .background_executor()
            .spawn(async move { search_project_content(&project, &query) });

        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                let project_matches = this.selected_project() == Some(&request_project);
                if this.search_generation != generation
                    || this.search_query.trim() != request_query
                    || !project_matches
                {
                    return;
                }
                this.search_state = match result {
                    Ok(hits) if hits.is_empty() => LoadState::Empty,
                    Ok(hits) => LoadState::Loaded(hits),
                    Err(error) => LoadState::Error(error),
                };
                cx.notify();
            });
        })
        .detach();
    }

    fn perform_search(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.execute_search(window, cx);
    }

    fn handle_search_keydown(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.keystroke.key == "enter" {
            self.execute_search(window, cx);
            return;
        }

        let key = event.keystroke.key.as_str();
        let mods = &event.keystroke.modifiers;

        if key == "backspace" {
            self.search_query.pop();
            self.search_generation = self.search_generation.wrapping_add(1);
            self.search_state = LoadState::Idle;
            cx.notify();
            return;
        }

        if key == "escape" {
            self.search_query.clear();
            self.search_generation = self.search_generation.wrapping_add(1);
            self.search_state = LoadState::Idle;
            cx.notify();
            return;
        }

        if mods.control || mods.alt || mods.function {
            return;
        }

        let candidate = event.keystroke.key_char.as_deref().unwrap_or(key);
        if candidate.chars().count() == 1 {
            let ch = candidate.chars().next().unwrap();
            if ch.is_ascii_graphic() || ch == ' ' {
                self.search_query.push(ch.to_ascii_lowercase());
                self.search_generation = self.search_generation.wrapping_add(1);
                self.search_state = LoadState::Idle;
                cx.notify();
            }
        }
    }

    fn is_entry_visible(entry: &FileEntry, expanded: &HashSet<PathBuf>) -> bool {
        if entry.depth == 0 {
            return true;
        }
        let mut current = entry.path.parent();
        for _ in 0..entry.depth {
            match current {
                Some(dir) if expanded.contains(dir) => {
                    current = dir.parent();
                }
                _ => return false,
            }
        }
        true
    }

    fn render_details_panel(
        &self,
        theme: Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let Some(project) = self.selected_project().cloned() else {
            return div()
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
                .child("select a project")
                .into_any_element();
        };

        let active_tab = self.details_tab;

        let tab_bar = div()
            .h(px(32.0))
            .flex_shrink_0()
            .flex()
            .items_center()
            .px_2()
            .border_b_1()
            .border_color(theme.border)
            .gap_0()
            .child({
                let is_active = active_tab == DetailsTab::Overview;
                div()
                    .id("tab-overview")
                    .h(px(32.0))
                    .px_2()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .cursor_pointer()
                    .border_b_1()
                    .border_color(if is_active {
                        theme.focus
                    } else {
                        theme.panel_background
                    })
                    .text_size(px(10.0))
                    .font_family(MONO_FONT)
                    .text_color(if is_active {
                        theme.text
                    } else {
                        theme.text_disabled
                    })
                    .hover(move |style| style.text_color(theme.text))
                    .child("OVERVIEW")
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.select_tab(DetailsTab::Overview, window, cx);
                    }))
            })
            .child({
                let is_active = active_tab == DetailsTab::Files;
                div()
                    .id("tab-files")
                    .h(px(32.0))
                    .px_2()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .cursor_pointer()
                    .border_b_1()
                    .border_color(if is_active {
                        theme.focus
                    } else {
                        theme.panel_background
                    })
                    .text_size(px(10.0))
                    .font_family(MONO_FONT)
                    .text_color(if is_active {
                        theme.text
                    } else {
                        theme.text_disabled
                    })
                    .hover(move |style| style.text_color(theme.text))
                    .child("FILES")
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.select_tab(DetailsTab::Files, window, cx);
                    }))
            })
            .child({
                let is_active = active_tab == DetailsTab::Search;
                div()
                    .id("tab-search")
                    .h(px(32.0))
                    .px_2()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .cursor_pointer()
                    .border_b_1()
                    .border_color(if is_active {
                        theme.focus
                    } else {
                        theme.panel_background
                    })
                    .text_size(px(10.0))
                    .font_family(MONO_FONT)
                    .text_color(if is_active {
                        theme.text
                    } else {
                        theme.text_disabled
                    })
                    .hover(move |style| style.text_color(theme.text))
                    .child("SEARCH")
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.select_tab(DetailsTab::Search, window, cx);
                    }))
            })
            .child(div().flex_1())
            .child({
                div()
                    .id("open-project-from-tabs")
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
                    .child("OPEN")
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                        this.open_selected_project(&OpenSelectedProject, window, cx);
                    }))
            });

        let content: gpui::AnyElement = match self.details_tab {
            DetailsTab::Overview => self
                .overview_content(project, theme, window, cx)
                .into_any_element(),
            DetailsTab::Files => self.files_content(theme, window, cx),
            DetailsTab::Search => self.search_content(theme, cx),
        };

        div()
            .size_full()
            .flex()
            .flex_col()
            .child(tab_bar)
            .child(content)
            .into_any_element()
    }

    fn overview_content(
        &self,
        project: Project,
        theme: Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let readme_style = TextViewStyle {
            paragraph_gap: rems(0.65),
            heading_base_font_size: px(15.0),
            highlight_theme: HighlightTheme::default_dark(),
            is_dark: true,
            ..Default::default()
        };

        let readme: AnyElement = match &self.readme_state {
            LoadState::Loaded(document) => {
                let source = if self.readme_preview {
                    sanitize_markdown_images(document)
                } else {
                    markdown_raw_source(document)
                };
                TextView::markdown("project-readme", source, window, cx)
                    .style(readme_style)
                    .selectable(true)
                    .scrollable(true)
                    .size_full()
                    .px_3()
                    .py_2()
                    .text_color(theme.text_muted)
                    .into_any_element()
            }
            LoadState::Loading => workbench_message("loading README...", theme.text_disabled),
            LoadState::Error(error) => workbench_message(error.clone(), theme.error),
            LoadState::Idle | LoadState::Empty => {
                workbench_message("no README found", theme.text_disabled)
            }
        };

        div()
            .id("overview-panel")
            .min_h_0()
            .flex_1()
            .flex()
            .flex_col()
            .child(
                div()
                    .h(px(88.0))
                    .flex_shrink_0()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .h(px(24.0))
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .child(
                                div()
                                    .min_w_0()
                                    .flex_1()
                                    .text_size(px(15.0))
                                    .text_color(theme.text)
                                    .child(project.name.clone()),
                            )
                            .child(
                                div()
                                    .id("readme-mode-toggle")
                                    .px_2()
                                    .py(px(2.0))
                                    .font_family(MONO_FONT)
                                    .text_size(px(9.0))
                                    .text_color(theme.accent)
                                    .cursor_pointer()
                                    .hover(move |style| style.bg(theme.surface_hover))
                                    .child(if self.readme_preview {
                                        "RAW"
                                    } else {
                                        "PREVIEW"
                                    })
                                    .on_click(cx.listener(Self::toggle_readme_preview)),
                            ),
                    )
                    .child(
                        div()
                            .h(px(42.0))
                            .flex_shrink_0()
                            .flex()
                            .gap_2()
                            .child(info_card(
                                "TYPE",
                                project.project_type.label().to_string(),
                                theme,
                            ))
                            .child(info_card(
                                "SOURCE",
                                project.source.label().to_string(),
                                theme,
                            ))
                            .child(info_card(
                                "GIT",
                                if project.has_git {
                                    "repository"
                                } else {
                                    "none"
                                }
                                .to_string(),
                                theme,
                            ))
                            .child(info_card(
                                "PATH",
                                project.path.to_string_lossy().into_owned(),
                                theme,
                            )),
                    ),
            )
            .child(div().min_h_0().flex_1().child(readme))
    }

    fn files_content(
        &self,
        theme: Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let listing = match &self.tree_state {
            LoadState::Loaded(listing) => listing,
            LoadState::Idle => {
                return workbench_message("select Files to load the tree", theme.text_disabled)
            }
            LoadState::Loading => {
                return workbench_message("loading file tree...", theme.text_disabled)
            }
            LoadState::Empty => return workbench_message("project is empty", theme.text_muted),
            LoadState::Error(error) => return workbench_message(error.clone(), theme.error),
        };

        let visible: Vec<usize> = listing
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| Self::is_entry_visible(e, &self.expanded_dirs))
            .map(|(i, _)| i)
            .collect();

        let show_hidden = self.show_hidden;
        let dragging = self.dragging_split;
        let tree_notice = match (listing.truncated, listing.warnings.len()) {
            (true, 0) => "500+ entries".to_string(),
            (false, 0) => String::new(),
            (true, warnings) => format!("500+ · {warnings} warning(s)"),
            (false, warnings) => format!("{warnings} warning(s)"),
        };

        let tree_panel = div()
            .id("tree-panel")
            .w(px(self.tree_width_px))
            .flex_shrink_0()
            .flex()
            .flex_col()
            .child(
                div()
                    .h(px(24.0))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .px_2()
                    .border_b_1()
                    .border_color(theme.border)
                    .gap_1()
                    .cursor_pointer()
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(if show_hidden {
                                theme.text
                            } else {
                                theme.text_disabled
                            })
                            .child(if show_hidden { "\u{2611}" } else { "\u{2610}" }),
                    )
                    .child(
                        div()
                            .font_family(MONO_FONT)
                            .text_size(px(10.0))
                            .text_color(theme.text_disabled)
                            .child("show hidden"),
                    )
                    .child(
                        div()
                            .ml_auto()
                            .font_family(MONO_FONT)
                            .text_size(px(9.0))
                            .text_color(theme.warning)
                            .child(tree_notice),
                    )
                    .id("hidden-toggle")
                    .cursor_pointer()
                    .on_click(cx.listener(Self::toggle_hidden)),
            )
            .child(
                div()
                    .id("files-scroll")
                    .flex_1()
                    .overflow_y_scroll()
                    .children(visible.iter().map(|&idx| {
                        let entry = &listing.entries[idx];
                        let indent = 6.0 + entry.depth as f32 * 14.0;
                        let is_selected = self.selected_file.as_ref() == Some(&entry.path);
                        let is_dir = entry.is_dir;
                        let is_expanded = is_dir && self.expanded_dirs.contains(&entry.path);
                        let name = entry.name.clone();
                        let glyph = if is_dir {
                            if is_expanded {
                                "\u{25be} "
                            } else {
                                "\u{25b8} "
                            }
                        } else {
                            file_icon(&name)
                        };
                        let row_bg = if is_selected {
                            theme.surface_selected
                        } else {
                            theme.panel_background
                        };
                        let text_col = if is_selected {
                            theme.text
                        } else {
                            theme.text_muted
                        };

                        div()
                            .id(("ft", idx))
                            .h(px(22.0))
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .pl(px(indent))
                            .gap_1()
                            .font_family(MONO_FONT)
                            .text_size(px(11.0))
                            .bg(row_bg)
                            .cursor_pointer()
                            .hover(move |style| style.bg(theme.surface_hover))
                            .child(
                                div()
                                    .text_size(px(10.0))
                                    .text_color(if is_dir { theme.text } else { text_col })
                                    .child(glyph),
                            )
                            .child(div().text_color(text_col).child(name))
                            .on_click(cx.listener(move |this, event, window, cx| {
                                this.toggle_tree_entry(idx, event, window, cx);
                            }))
                    })),
            );

        let handle = div()
            .id("split-handle")
            .w(px(4.0))
            .flex_shrink_0()
            .bg(if dragging { theme.focus } else { theme.border })
            .hover(move |style| style.bg(theme.focus))
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _: &MouseDownEvent, _window, cx| {
                    this.dragging_split = true;
                    cx.notify();
                }),
            );

        let code_panel_content: gpui::AnyElement = match (&self.selected_file, &self.file_state) {
            (Some(path), LoadState::Loaded(document)) => {
                self.document_panel(path, document, theme, window, cx)
            }
            (Some(_), LoadState::Error(error)) => workbench_message(error.clone(), theme.error),
            (Some(_), LoadState::Loading) => {
                workbench_message("loading file...", theme.text_disabled)
            }
            (Some(_), LoadState::Empty) => workbench_message("file is empty", theme.text_muted),
            (Some(_), LoadState::Idle) | (None, _) => {
                workbench_message("select a file", theme.text_disabled)
            }
        };

        let code_panel = div()
            .id("code-panel")
            .flex_1()
            .min_w_0()
            .child(code_panel_content);

        div()
            .id("split-pane")
            .flex_1()
            .flex()
            .flex_row()
            .min_h_0()
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _event: &MouseUpEvent, _window, cx| {
                    if this.dragging_split {
                        this.dragging_split = false;
                        cx.notify();
                    }
                }),
            )
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _window, cx| {
                if this.dragging_split {
                    let raw: f32 = event.position.x.into();
                    let offset = raw - 264.0;
                    this.tree_width_px = offset.clamp(80.0, 600.0);
                    cx.notify();
                }
            }))
            .child(tree_panel)
            .child(handle)
            .child(code_panel)
            .into_any_element()
    }

    fn document_panel(
        &self,
        path: &std::path::Path,
        document: &str,
        theme: Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let line_count = document.lines().count().max(1);
        let language = language_for_path(path);
        let language_id = language.to_ascii_lowercase().replace(' ', "-");
        let wrap = self.wrap_document;
        let editor = window.use_keyed_state("source-code-editor", cx, |window, cx| {
            InputState::new(window, cx)
                .code_editor(language_id.clone())
                .line_number(true)
                .soft_wrap(wrap)
                .default_value(document.to_string())
        });
        if editor.read(cx).value().as_ref() != document {
            editor.update(cx, |editor, cx| {
                editor.set_highlighter(language_id, cx);
                editor.set_value(document.to_string(), window, cx);
                editor.set_soft_wrap(wrap, window, cx);
            });
        }
        let app = cx.entity();
        if let Some(target_line) = self.pending_document_line {
            let target_line = target_line.min(line_count.saturating_sub(1));
            editor.update(cx, |editor, cx| {
                let line = u32::try_from(target_line).unwrap_or(u32::MAX);
                editor.set_cursor_position(Position::new(line, 0), window, cx);
            });
            let app = app.clone();
            cx.defer(move |cx| {
                app.update(cx, |this, cx| {
                    this.pending_document_line = None;
                    cx.notify();
                });
            });
        }
        let code = Input::new(&editor)
            .disabled(true)
            .appearance(false)
            .focus_bordered(false)
            .h_full();
        let editor_for_wrap = editor.clone();

        div()
            .size_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .h(px(26.0))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .border_b_1()
                    .border_color(theme.border)
                    .font_family(MONO_FONT)
                    .text_size(px(10.0))
                    .text_color(theme.text_muted)
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .child(path.to_string_lossy().into_owned()),
                    )
                    .child(format!("{language} · {line_count} lines"))
                    .child(
                        div()
                            .id("document-copy")
                            .px_2()
                            .cursor_pointer()
                            .text_color(theme.accent)
                            .hover(move |style| style.bg(theme.surface_hover))
                            .child("COPY ALL")
                            .on_click(cx.listener(Self::copy_document_selection)),
                    )
                    .child(
                        div()
                            .id("document-wrap")
                            .px_2()
                            .cursor_pointer()
                            .text_color(if wrap {
                                theme.accent
                            } else {
                                theme.text_disabled
                            })
                            .hover(move |style| style.bg(theme.surface_hover))
                            .child(if wrap { "WRAP ON" } else { "WRAP OFF" })
                            .on_click(move |_, window, cx| {
                                let next = !app.read(cx).wrap_document;
                                app.update(cx, |this, cx| {
                                    this.wrap_document = next;
                                    cx.notify();
                                });
                                editor_for_wrap.update(cx, |editor, cx| {
                                    editor.set_soft_wrap(next, window, cx);
                                });
                            }),
                    ),
            )
            .child(div().id("code-scroll").flex_1().min_h_0().child(code))
            .into_any_element()
    }

    fn search_content(&self, theme: Theme, cx: &mut Context<Self>) -> gpui::AnyElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .h(px(28.0))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .px_2()
                    .border_b_1()
                    .border_color(theme.border)
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .h_full()
                            .flex()
                            .items_center()
                            .font_family(MONO_FONT)
                            .text_size(px(11.0))
                            .text_color(if self.search_query.is_empty() {
                                theme.text_disabled
                            } else {
                                theme.text
                            })
                            .child({
                                if self.search_query.is_empty() {
                                    "search file contents...".to_string()
                                } else {
                                    self.search_query.clone()
                                }
                            }),
                    )
                    .child(
                        div()
                            .id("search-go")
                            .cursor_pointer()
                            .w(px(24.0))
                            .h(px(22.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .border_1()
                            .border_color(if self.search_query.is_empty() {
                                theme.border
                            } else {
                                theme.accent
                            })
                            .bg(if self.search_query.is_empty() {
                                theme.panel_background
                            } else {
                                theme.accent.opacity(0.12)
                            })
                            .text_size(px(12.0))
                            .text_color(if self.search_query.is_empty() {
                                theme.text_disabled
                            } else {
                                theme.accent
                            })
                            .hover(move |style| {
                                style
                                    .bg(theme.accent.opacity(0.2))
                                    .border_color(theme.accent)
                                    .text_color(theme.accent)
                            })
                            .child("\u{2192}")
                            .on_click(cx.listener(Self::perform_search)),
                    ),
            )
            .child(match &self.search_state {
                LoadState::Idle => {
                    workbench_message("enter a query and press Enter", theme.text_disabled)
                }
                LoadState::Loading => workbench_message("searching...", theme.text_disabled),
                LoadState::Empty => workbench_message("no results", theme.text_muted),
                LoadState::Error(error) => workbench_message(error.clone(), theme.error),
                LoadState::Loaded(hits) => div()
                    .id("search-scroll")
                    .flex_1()
                    .overflow_y_scroll()
                    .children(hits.iter().enumerate().map(|(i, hit)| {
                        let path_str = hit.path.to_string_lossy().into_owned();
                        let preview = hit.preview.clone();
                        let line = hit.line;
                        let hit_path = hit.path.clone();
                        div()
                            .id(("sh", i))
                            .h(px(24.0))
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .gap_2()
                            .px_2()
                            .cursor_pointer()
                            .hover(move |style| style.bg(theme.surface_hover))
                            .child(
                                div()
                                    .font_family(MONO_FONT)
                                    .text_size(px(9.0))
                                    .text_color(theme.text_disabled)
                                    .child(format!("L{}", line)),
                            )
                            .child(
                                div()
                                    .min_w_0()
                                    .flex_1()
                                    .font_family(MONO_FONT)
                                    .text_size(px(10.0))
                                    .text_color(theme.text_muted)
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .child(preview),
                            )
                            .child(
                                div()
                                    .font_family(MONO_FONT)
                                    .text_size(px(9.0))
                                    .text_color(theme.text_disabled)
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .max_w(px(120.0))
                                    .child(path_str),
                            )
                            .on_click(cx.listener(move |this, _event, window, cx| {
                                this.selected_file = Some(hit_path.clone());
                                this.pending_document_line = Some(line.saturating_sub(1));
                                this.details_tab = DetailsTab::Files;
                                this.load_file_content(cx);
                                window.focus(&this.focus_handle);
                                cx.notify();
                            }))
                    }))
                    .into_any_element(),
            })
            .into_any_element()
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
        let filtered_indices = self.filtered_indices();
        let scan_button_label = if self.scan.state == ScanState::Scanning {
            "Scan again"
        } else {
            "Scan"
        };
        let scan_status = scan_status(&self.scan.state, &self.scan_root);
        let status_color = scan_state_color(theme, &self.scan.state);
        let total_count = self.scan.projects.len();
        let visible_count = filtered_indices.len();
        let filter_active = !self.filter_query.is_empty();

        let project_rows =
            filtered_indices
                .iter()
                .enumerate()
                .map(|(filtered_index, &project_index)| {
                    let project = self.scan.projects[project_index].clone();
                    let is_selected = self.selected == Some(filtered_index);
                    let background = if is_selected {
                        theme.surface_selected
                    } else {
                        theme.sidebar_background
                    };
                    let type_color = project_type_color(theme, project.project_type);

                    div()
                        .id(("project-row", filtered_index))
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
                            this.select_project(filtered_index, window, cx);
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
            _ if visible_count == 0 => message_panel(
                "NO MATCHES",
                format!("No projects match \"{}\".", self.filter_query),
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
            .on_action(cx.listener(Self::open_selected_project))
            .on_action(cx.listener(Self::note_component_copy))
            .on_key_down(cx.listener(Self::handle_filter_keydown))
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
                            .id("settings-button")
                            .h(px(23.0))
                            .w(px(26.0))
                            .ml_1()
                            .flex()
                            .items_center()
                            .justify_center()
                            .border_1()
                            .border_color(theme.border_strong)
                            .bg(theme.surface_background)
                            .text_size(px(12.0))
                            .text_color(if self.show_settings {
                                theme.text
                            } else {
                                theme.text_muted
                            })
                            .cursor_pointer()
                            .hover(move |style| {
                                style
                                    .bg(theme.surface_hover)
                                    .border_color(theme.text_disabled)
                                    .text_color(theme.text)
                            })
                            .active(move |style| style.bg(theme.surface_selected))
                            .child("\u{2699}")
                            .on_click(cx.listener(Self::toggle_settings)),
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
            .child(if self.show_settings {
                self.settings_panel(theme, cx).into_any_element()
            } else {
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
                                            .child(if filter_active {
                                                format!("{visible_count}/{total_count}")
                                            } else {
                                                total_count.to_string()
                                            }),
                                    ),
                            )
                            .child(project_list),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .bg(theme.panel_background)
                            .child(self.render_details_panel(theme, window, cx)),
                    )
                    .into_any()
            })
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
                            .child(div().size(px(5.0)).rounded_full().bg(
                                if self.launch_error.is_some() {
                                    theme.error
                                } else if self.copy_feedback.is_some() {
                                    theme.success
                                } else {
                                    status_color
                                },
                            ))
                            .child(div().min_w_0().flex().items_center().gap_1().child(
                                if let Some(error) = &self.launch_error {
                                    div()
                                        .text_color(theme.error)
                                        .whitespace_nowrap()
                                        .overflow_hidden()
                                        .child(error.clone())
                                } else if let Some(feedback) = &self.copy_feedback {
                                    div()
                                        .font_family(MONO_FONT)
                                        .text_color(theme.success)
                                        .whitespace_nowrap()
                                        .child(feedback.clone())
                                } else if filter_active {
                                    div()
                                        .font_family(MONO_FONT)
                                        .text_color(theme.accent)
                                        .whitespace_nowrap()
                                        .child("filter:".to_string())
                                        .child(
                                            div()
                                                .text_color(theme.text)
                                                .whitespace_nowrap()
                                                .overflow_hidden()
                                                .child(self.filter_query.clone()),
                                        )
                                } else {
                                    div()
                                        .whitespace_nowrap()
                                        .overflow_hidden()
                                        .text_color(theme.text_muted)
                                        .child(scan_status)
                                },
                            )),
                    )
                    .child(
                        div()
                            .ml_3()
                            .flex()
                            .items_center()
                            .gap_3()
                            .font_family(MONO_FONT)
                            .text_color(theme.text_disabled)
                            .child("type filter  ·  ↑↓ select  ·  Enter open  ·  Ctrl+R scan")
                            .child(
                                div()
                                    .max_w(px(200.0))
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .child(self.scan_root.to_string_lossy().into_owned()),
                            ),
                    ),
            )
    }
}

fn info_card(label: &'static str, value: String, theme: Theme) -> Div {
    div()
        .min_w_0()
        .flex_1()
        .h_full()
        .flex()
        .flex_col()
        .justify_center()
        .px_2()
        .border_1()
        .border_color(theme.border)
        .bg(theme.surface_background)
        .child(
            div()
                .font_family(MONO_FONT)
                .text_size(px(9.0))
                .text_color(theme.text_disabled)
                .child(label),
        )
        .child(
            div()
                .font_family(MONO_FONT)
                .text_size(px(10.0))
                .text_color(theme.text_muted)
                .whitespace_nowrap()
                .overflow_hidden()
                .child(value),
        )
}

fn markdown_raw_source(source: &str) -> String {
    markdown_fenced_source("text", source)
}

fn remote_parent_path(path: &str) -> Option<String> {
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        return None;
    }
    let parent = trimmed.rsplit_once('/').map(|(parent, _)| parent)?;
    Some(if parent.is_empty() { "/" } else { parent }.to_string())
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

pub(crate) fn run() {
    Application::new().run(|cx: &mut App| {
        gpui_component::init(cx);
        gpui_component::Theme::change(gpui_component::ThemeMode::Dark, None, cx);
        let component_theme = gpui_component::Theme::global_mut(cx);
        component_theme.radius = px(1.0);
        component_theme.radius_lg = px(1.0);
        component_theme.font_family = UI_FONT.into();
        component_theme.mono_font_family = MONO_FONT.into();
        cx.bind_keys([
            KeyBinding::new("up", SelectPreviousProject, Some("DevHub")),
            KeyBinding::new("down", SelectNextProject, Some("DevHub")),
            KeyBinding::new("home", SelectFirstProject, Some("DevHub")),
            KeyBinding::new("end", SelectLastProject, Some("DevHub")),
            KeyBinding::new("ctrl-r", ScanCurrentFolder, Some("DevHub")),
            KeyBinding::new("enter", OpenSelectedProject, Some("DevHub")),
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
                let app = cx.new(|cx| DevHubLite::new(window, cx));
                cx.new(|cx| gpui_component::Root::new(app, window, cx))
            },
        )
        .expect("failed to open the main window");

        cx.activate(true);
    });
}
