use crate::assets::Assets;
use crate::platform::{begin_window_drag, configure_windows_surface, toggle_window_zoom};
use crate::ui::{
    file_icon, message_panel, project_type_color, scan_state_color, scan_status, section_label,
    window_control, workbench_message,
};
use devhub_core::{
    cache_path, git_branches_cancellable, git_commit_cancellable, git_commit_diff_cancellable,
    git_diff_cancellable, git_discard_cancellable, git_fetch_cancellable, git_log_cancellable,
    git_push_cancellable, git_remote_to_github_url, git_stage_all_cancellable,
    git_stage_cancellable, git_status_cancellable, git_status_summary_cancellable,
    git_switch_branch_cancellable, git_unstage_all_cancellable, git_unstage_cancellable,
    list_local_subdirs, list_project_tree_cancellable, list_remote_subdirs_cancellable,
    load_projects_with_diagnostics, local_roots, open_project_in_zed,
    read_project_file_cancellable, read_project_readme_cancellable, save_projects_with_diagnostics,
    scan_directories_cancellable, scan_remote_host_cancellable, search_project_content_cancellable,
    sort_projects, validate_remote_path, validate_ssh_host, AppearanceMode, CancellationToken,
    CommitEntry, CommitFileChange, Config, DirectoryEntry, FileEntry, GitBranch, GitDiffKind,
    GitError, GitErrorKind, GitFileChange, GitOperationResult, GitStatus, PersistenceEvent,
    PersistenceFailure, Project, ProjectLocator, RemoteHostConfig, SearchHit, ThemeId, TreeListing,
    HISTORY_PAGE_SIZE,
};
use devhub_gpui::{
    detect_editors, filtered_commands, filtered_editors, filtered_project_indices, filtered_themes,
    has_scan_sources, language_for_path, next_selection, omit_markdown_images, parse_unified_diff,
    partition_local_scan_roots, persistence_status_text, previous_selection, scan_sources_changed,
    should_show_ftue, visible_project_row, Activity, CommandId, CommandSpec, DetectedEditor,
    DiffLine, DiffLineKind, PersistenceHistory, ScanModel, ScanState, TerminalLaunch,
    TerminalPanel, Theme, MONO_FONT, TERMINAL_FONT, UI_FONT,
};
use gpui::prelude::*;
use gpui::*;
use gpui_component::button::{Button, ButtonCustomVariant, ButtonVariants};
use gpui_component::checkbox::Checkbox;
use gpui_component::highlighter::HighlightTheme;
use gpui_component::input::{Input, InputEvent, InputState, Position};
use gpui_component::text::{TextView, TextViewStyle};
use gpui_component::{Disableable, Icon, IconName, IconNamed, Selectable, Sizable};
use notify::{RecursiveMode, Watcher};
use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

const TITLEBAR_HEIGHT: f32 = 28.0;
const PROJECT_ROW_HEIGHT: f32 = 30.0;
const STATUSBAR_HEIGHT: f32 = 22.0;
const MIN_TERMINAL_HEIGHT: f32 = 120.0;
const GIT_WATCH_TICK: Duration = Duration::from_millis(100);
const GIT_WATCH_QUIET_TICKS: usize = 2;
const GIT_REMOTE_POLL_INTERVAL: Duration = Duration::from_millis(1_500);
const MAX_RENDERED_DIFF_LINES: usize = 5_000;

#[derive(Clone, Copy)]
struct ZedIcon;

impl IconNamed for ZedIcon {
    fn path(self) -> SharedString {
        "zed.svg".into()
    }
}

#[derive(Clone, Copy)]
struct GitBranchIcon;

impl IconNamed for GitBranchIcon {
    fn path(self) -> SharedString {
        "git-branch.svg".into()
    }
}

#[derive(Clone, Copy)]
struct HistoryIcon;

impl IconNamed for HistoryIcon {
    fn path(self) -> SharedString {
        "history.svg".into()
    }
}

#[derive(Clone, Copy)]
struct ScanSearchIcon;

impl IconNamed for ScanSearchIcon {
    fn path(self) -> SharedString {
        "scan-search.svg".into()
    }
}

actions!(
    devhub,
    [
        SelectPreviousProject,
        SelectNextProject,
        SelectFirstProject,
        SelectLastProject,
        ScanCurrentFolder,
        OpenSelectedProject,
        FocusProjectFilter,
        ShowCommandPalette,
        ShowProjectSwitcher,
        ToggleProjectCatalog,
        ShowOverview,
        ShowFiles,
        ShowSearch,
        ShowGit,
        ShowHistory,
        ToggleContextPane,
        DismissLauncher,
        AcceptLauncher,
        ToggleTerminal,
    ]
);

struct DevHubLite {
    window_handle: AnyWindowHandle,
    scan: ScanModel,
    scan_cancellation: Option<CancellationToken>,
    selected: Option<usize>,
    missing_project: Option<ProjectLocator>,
    filter_query: String,
    launch_error: Option<String>,
    persistence_history: PersistenceHistory,
    scan_root: PathBuf,
    config: Config,
    focus_handle: FocusHandle,
    project_scroll: UniformListScrollHandle,
    filter_input: Entity<InputState>,
    _filter_subscription: Subscription,
    show_settings: bool,
    pending_scan_dirs: Vec<PathBuf>,
    pending_remote_hosts: Vec<RemoteHostConfig>,
    pending_max_depth: usize,
    pending_theme: ThemeId,
    pending_appearance: AppearanceMode,
    source_picker: SourcePicker,
    picker_entries: LoadState<Vec<DirectoryEntry>>,
    picker_generation: u64,
    picker_cancellation: Option<CancellationToken>,
    remote_name_input: Entity<InputState>,
    remote_host_input: Entity<InputState>,
    remote_path_input: Entity<InputState>,
    activity: Activity,
    project_catalog_open: bool,
    terminal_visible: bool,
    terminal_entity: Option<Entity<TerminalPanel>>,
    terminal_owner: Option<ProjectLocator>,
    context_pane_visible: bool,
    tree_state: LoadState<TreeListing>,
    tree_generation: u64,
    tree_cancellation: Option<CancellationToken>,
    expanded_dirs: HashSet<PathBuf>,
    selected_file: Option<PathBuf>,
    file_state: LoadState<String>,
    file_generation: u64,
    file_cancellation: Option<CancellationToken>,
    document_focused: bool,
    wrap_document: bool,
    pending_document_line: Option<usize>,
    copy_feedback: Option<String>,
    copy_feedback_generation: u64,
    launcher: Option<LauncherMode>,
    launcher_query: String,
    launcher_selected: usize,
    launcher_input: Entity<InputState>,
    launcher_scroll: ScrollHandle,
    detected_editors: Vec<DetectedEditor>,
    editor_discovery_complete: bool,
    _launcher_subscription: Subscription,
    search_query: String,
    search_input: Entity<InputState>,
    _search_subscription: Subscription,
    search_state: LoadState<Vec<SearchHit>>,
    search_generation: u64,
    search_cancellation: Option<CancellationToken>,
    git_status_state: LoadState<GitStatus>,
    git_status_generation: u64,
    git_status_cancellation: Option<CancellationToken>,
    git_branches_state: LoadState<Vec<GitBranch>>,
    git_branches_generation: u64,
    git_branches_cancellation: Option<CancellationToken>,
    git_refresh_generation: u64,
    git_selection: Option<GitSelection>,
    git_diff_state: LoadState<String>,
    git_diff_generation: u64,
    git_diff_cancellation: Option<CancellationToken>,
    git_operation_generation: u64,
    git_operation_cancellation: Option<CancellationToken>,
    git_notice: Option<GitNotice>,
    git_scroll: ScrollHandle,
    git_tree_view: bool,
    git_collapsed_dirs: HashSet<(GitDiffKind, PathBuf)>,
    git_context_menu: Option<(GitSelection, f32, f32)>,
    git_commit_input: Entity<InputState>,
    _git_commit_subscription: Subscription,
    git_commit_message: String,
    git_amend: bool,
    git_commit_menu_open: bool,
    git_pending_discard: Option<GitFileChange>,
    show_hidden: bool,
    readme_state: LoadState<String>,
    readme_generation: u64,
    readme_cancellation: Option<CancellationToken>,
    readme_preview: bool,
    history_state: LoadState<()>,
    history_commits: Vec<CommitEntry>,
    history_generation: u64,
    history_cancellation: Option<CancellationToken>,
    history_more_cancellation: Option<CancellationToken>,
    history_scroll: UniformListScrollHandle,
    history_selected: Option<usize>,
    history_selected_file: Option<String>,
    history_diff_state: LoadState<String>,
    history_diff_generation: u64,
    history_diff_cancellation: Option<CancellationToken>,
    history_skip: usize,
    history_has_more: bool,
    history_more_error: Option<String>,
    history_open_error: Option<String>,

    files_context_width_px: f32,
    search_context_width_px: f32,
    git_context_width_px: f32,
    project_catalog_width_px: f32,
    terminal_height_px: f32,
    resize_target: Option<ResizeTarget>,
    context_menu: Option<(usize, f32, f32)>,
}

#[derive(Clone, Copy)]
pub(crate) enum WindowCommand {
    Minimize,
    Maximize,
    Close,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LauncherMode {
    Branches,
    Commands,
    Editors,
    Projects,
    Themes,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ResizeTarget {
    WorkspaceContext,
    ProjectCatalog,
    Terminal,
}

#[derive(Clone, PartialEq, Eq)]
struct GitSelection {
    change: GitFileChange,
    kind: GitDiffKind,
}

#[derive(Clone)]
struct GitTreeEntry {
    path: PathBuf,
    label: String,
    depth: usize,
    selection: Option<GitSelection>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum GitNoticeTone {
    Working,
    Success,
    Error,
}

struct GitNotice {
    text: String,
    tone: GitNoticeTone,
    automatic: bool,
}

enum GitAction {
    Stage(Vec<PathBuf>),
    StageAll,
    Unstage(Vec<PathBuf>),
    UnstageAll,
    Discard(GitFileChange),
    Commit { message: String, amend: bool },
    Fetch,
    Push { set_upstream: bool },
    SwitchBranch(String),
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
        let filter_input = cx.new(|cx| InputState::new(window, cx).placeholder("Filter projects"));
        let _filter_subscription =
            cx.subscribe_in(&filter_input, window, Self::on_filter_input_event);
        let launcher_input = cx.new(|cx| InputState::new(window, cx).placeholder("Type to filter"));
        let _launcher_subscription =
            cx.subscribe_in(&launcher_input, window, Self::on_launcher_input_event);
        let search_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Search file contents"));
        let _search_subscription =
            cx.subscribe_in(&search_input, window, Self::on_search_input_event);
        let git_commit_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Commit message")
                .multi_line(true)
                .rows(2)
        });
        let _git_commit_subscription =
            cx.subscribe_in(&git_commit_input, window, Self::on_git_commit_input_event);
        window.focus(&focus_handle);

        let mut startup_errors = Vec::new();
        let mut persistence_history = PersistenceHistory::default();
        let config_existed = Config::config_path().is_some_and(|path| path.is_file());
        let cache_existed = cache_path().is_some_and(|path| path.is_file());
        let show_ftue = should_show_ftue(config_existed, cache_existed);
        if let Err(error) = Config::ensure_dirs_exist() {
            startup_errors.push(error);
        }
        let config = match Config::load_or_create_with_diagnostics() {
            Ok(report) => {
                persistence_history.record_events(report.events);
                report.value
            }
            Err(error) => {
                persistence_history.record_failure(&error);
                startup_errors.push(error.to_string());
                Config::default()
            }
        };

        let scan_root = config
            .scan_dirs
            .first()
            .cloned()
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."));

        let initial_projects = match load_projects_with_diagnostics() {
            Ok(report) => {
                persistence_history.record_events(report.events);
                report.value.unwrap_or_default()
            }
            Err(error) => {
                persistence_history.record_failure(&error);
                startup_errors.push(error.to_string());
                Vec::new()
            }
        };
        let pending_theme = config.theme;
        let pending_appearance = config.appearance;

        Self {
            window_handle: window.window_handle(),
            scan: ScanModel::new(initial_projects),
            scan_cancellation: None,
            selected: None,
            missing_project: None,
            filter_query: String::new(),
            launch_error: (!startup_errors.is_empty()).then(|| startup_errors.join(" | ")),
            persistence_history,
            scan_root,
            config,
            focus_handle,
            project_scroll: UniformListScrollHandle::new(),
            filter_input,
            _filter_subscription,
            show_settings: show_ftue,
            pending_scan_dirs: Vec::new(),
            pending_remote_hosts: Vec::new(),
            pending_max_depth: 3,
            pending_theme,
            pending_appearance,
            source_picker: SourcePicker::Closed,
            picker_entries: LoadState::Idle,
            picker_generation: 0,
            picker_cancellation: None,
            remote_name_input,
            remote_host_input,
            remote_path_input,
            activity: Activity::Overview,
            project_catalog_open: false,
            terminal_visible: false,
            terminal_entity: None,
            terminal_owner: None,
            context_pane_visible: true,
            tree_state: LoadState::Idle,
            tree_generation: 0,
            tree_cancellation: None,
            expanded_dirs: HashSet::new(),
            selected_file: None,
            file_state: LoadState::Idle,
            file_generation: 0,
            file_cancellation: None,
            document_focused: false,
            wrap_document: false,
            pending_document_line: None,
            copy_feedback: None,
            copy_feedback_generation: 0,
            launcher: None,
            launcher_query: String::new(),
            launcher_selected: 0,
            launcher_input,
            launcher_scroll: ScrollHandle::new(),
            detected_editors: Vec::new(),
            editor_discovery_complete: false,
            _launcher_subscription,
            search_query: String::new(),
            search_input,
            _search_subscription,
            search_state: LoadState::Idle,
            search_generation: 0,
            search_cancellation: None,
            git_status_state: LoadState::Idle,
            git_status_generation: 0,
            git_status_cancellation: None,
            git_branches_state: LoadState::Idle,
            git_branches_generation: 0,
            git_branches_cancellation: None,
            git_refresh_generation: 0,
            git_selection: None,
            git_diff_state: LoadState::Idle,
            git_diff_generation: 0,
            git_diff_cancellation: None,
            git_operation_generation: 0,
            git_operation_cancellation: None,
            git_notice: None,
            git_scroll: ScrollHandle::new(),
            git_tree_view: false,
            git_collapsed_dirs: HashSet::new(),
            git_context_menu: None,
            git_commit_input,
            _git_commit_subscription,
            git_commit_message: String::new(),
            git_amend: false,
            git_commit_menu_open: false,
            git_pending_discard: None,
            show_hidden: false,
            readme_state: LoadState::Idle,
            readme_generation: 0,
            readme_cancellation: None,
            readme_preview: true,
            history_state: LoadState::Idle,
            history_commits: Vec::new(),
            history_generation: 0,
            history_cancellation: None,
            history_more_cancellation: None,
            history_scroll: UniformListScrollHandle::new(),
            history_selected: None,
            history_selected_file: None,
            history_diff_state: LoadState::Idle,
            history_diff_generation: 0,
            history_diff_cancellation: None,
            history_skip: 0,
            history_has_more: true,
            history_more_error: None,
            history_open_error: None,

            files_context_width_px: 160.0,
            search_context_width_px: 205.0,
            git_context_width_px: 285.0,
            project_catalog_width_px: 276.0,
            terminal_height_px: 200.0,
            resize_target: None,
            context_menu: None,
        }
    }

    fn select_project(
        &mut self,
        project_index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus_handle);
        self.context_menu = None;
        self.git_context_menu = None;
        self.project_catalog_open = false;
        self.resize_target = None;
        self.set_selected(project_index, cx);
    }

    fn reset_workspace(&mut self, cx: &mut Context<Self>) {
        Self::cancel_token(&mut self.tree_cancellation);
        Self::cancel_token(&mut self.file_cancellation);
        Self::cancel_token(&mut self.search_cancellation);
        Self::cancel_token(&mut self.readme_cancellation);
        Self::cancel_token(&mut self.git_status_cancellation);
        Self::cancel_token(&mut self.git_branches_cancellation);
        Self::cancel_token(&mut self.git_diff_cancellation);
        Self::cancel_token(&mut self.git_operation_cancellation);
        self.tree_generation = self.tree_generation.wrapping_add(1);
        self.tree_state = LoadState::Idle;
        self.expanded_dirs.clear();
        self.selected_file = None;
        self.file_generation = self.file_generation.wrapping_add(1);
        self.file_state = LoadState::Idle;
        self.document_focused = false;
        self.pending_document_line = None;
        self.copy_feedback = None;
        self.search_generation = self.search_generation.wrapping_add(1);
        self.search_state = LoadState::Idle;
        self.readme_generation = self.readme_generation.wrapping_add(1);
        self.readme_state = LoadState::Idle;
        self.git_status_generation = self.git_status_generation.wrapping_add(1);
        self.git_refresh_generation = self.git_refresh_generation.wrapping_add(1);
        self.git_status_state = LoadState::Idle;
        self.git_branches_generation = self.git_branches_generation.wrapping_add(1);
        self.git_branches_state = LoadState::Idle;
        self.git_selection = None;
        self.git_diff_generation = self.git_diff_generation.wrapping_add(1);
        self.git_diff_state = LoadState::Idle;
        self.git_operation_generation = self.git_operation_generation.wrapping_add(1);
        self.git_notice = None;
        self.clear_git_commit_message(cx);
        self.git_amend = false;
        self.git_commit_menu_open = false;
        self.git_pending_discard = None;
        Self::cancel_token(&mut self.history_cancellation);
        Self::cancel_token(&mut self.history_more_cancellation);
        Self::cancel_token(&mut self.history_diff_cancellation);
        self.history_generation = self.history_generation.wrapping_add(1);
        self.history_state = LoadState::Idle;
        self.history_commits.clear();
        self.history_selected = None;
        self.history_selected_file = None;
        self.history_diff_generation = self.history_diff_generation.wrapping_add(1);
        self.history_diff_state = LoadState::Idle;
        self.history_skip = 0;
        self.history_has_more = true;
        self.history_more_error = None;
        self.history_open_error = None;
        self.git_context_menu = None;
        cx.notify();
    }

    fn cancel_token(token: &mut Option<CancellationToken>) {
        if let Some(token) = token.take() {
            token.cancel();
        }
    }

    fn clear_git_commit_message(&mut self, cx: &mut Context<Self>) {
        self.git_commit_message.clear();
        let input = self.git_commit_input.clone();
        let window_handle = self.window_handle;
        let _ = cx.update_window(window_handle, |_, window, cx| {
            input.update(cx, |input, cx| input.set_value("", window, cx));
        });
    }

    fn stop_scan(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.scan_cancellation.is_none() {
            return;
        }
        Self::cancel_token(&mut self.scan_cancellation);
        self.scan.cancel();
        cx.notify();
    }

    fn is_pinned(&self, project: &Project) -> bool {
        self.config.pinned_projects.contains(&project.path)
    }

    fn record_persistence_failure(&mut self, error: PersistenceFailure) {
        self.persistence_history.record_failure(&error);
        self.launch_error = Some(error.to_string());
    }

    fn save_config(&mut self) -> bool {
        match self.config.save_with_diagnostics() {
            Ok(report) => {
                self.persistence_history.record_events(report.events);
                true
            }
            Err(error) => {
                self.record_persistence_failure(error);
                false
            }
        }
    }

    fn restore_last_project(&mut self, cx: &mut Context<Self>) {
        self.selected = None;
        self.missing_project = None;
        if let Some(locator) = self.config.last_project.clone() {
            if let Some(index) = self
                .scan
                .projects
                .iter()
                .position(|project| project_locator_matches(project, &locator))
            {
                let project = &self.scan.projects[index];
                if self.is_hidden(project) {
                } else if project.source.is_remote() || project.path.is_dir() {
                    self.set_selected(index, cx);
                    return;
                } else {
                    self.missing_project = Some(locator);
                    self.end_terminal_session();
                    self.reset_workspace(cx);
                    return;
                }
            } else {
                self.missing_project = Some(locator);
                self.end_terminal_session();
                self.reset_workspace(cx);
                return;
            }
        }

        if let Some(index) = self
            .scan
            .projects
            .iter()
            .position(|project| !self.is_hidden(project))
        {
            self.set_selected(index, cx);
        } else {
            self.end_terminal_session();
            self.reset_workspace(cx);
        }
    }

    fn remember_project(&mut self, project: &Project) {
        let locator = project_locator(project);
        if self.config.last_project.as_ref() == Some(&locator) {
            return;
        }
        self.config.last_project = Some(locator);
        self.save_config();
    }

    fn forget_missing_project(&mut self, cx: &mut Context<Self>) {
        let Some(locator) = self.missing_project.take() else {
            return;
        };
        self.scan
            .projects
            .retain(|project| !project_locator_matches(project, &locator));
        self.config.last_project = None;
        self.save_config();
        match save_projects_with_diagnostics(&self.scan.projects) {
            Ok(report) => self.persistence_history.record_events(report.events),
            Err(error) => self.record_persistence_failure(error),
        }
        self.restore_last_project(cx);
    }

    fn toggle_pin(&mut self, project_index: usize, cx: &mut Context<Self>) {
        let Some(project) = self.scan.projects.get(project_index) else {
            return;
        };
        let path = &project.path;
        if let Some(pos) = self.config.pinned_projects.iter().position(|p| p == path) {
            self.config.pinned_projects.remove(pos);
        } else {
            self.config.pinned_projects.push(path.clone());
        }
        self.save_config();
        self.context_menu = None;
        cx.notify();
    }

    fn is_hidden(&self, project: &Project) -> bool {
        self.config.hidden_projects.contains(&project.path)
    }

    fn toggle_hide(&mut self, project_index: usize, cx: &mut Context<Self>) {
        let Some(project) = self.scan.projects.get(project_index) else {
            return;
        };
        let path = &project.path;
        if let Some(pos) = self.config.hidden_projects.iter().position(|p| p == path) {
            self.config.hidden_projects.remove(pos);
        } else {
            self.config.hidden_projects.push(path.clone());
        }
        self.save_config();
        self.context_menu = None;
        if self.selected == Some(project_index) {
            self.end_terminal_session();
        }
        self.selected = None;
        self.reset_workspace(cx);
        cx.notify();
    }

    fn unhide_project(&mut self, project_index: usize, cx: &mut Context<Self>) {
        let Some(project) = self.scan.projects.get(project_index) else {
            return;
        };
        if let Some(pos) = self
            .config
            .hidden_projects
            .iter()
            .position(|p| p == &project.path)
        {
            self.config.hidden_projects.remove(pos);
        }
        self.save_config();
        cx.notify();
    }

    fn scan_current_folder(
        &mut self,
        _: &ScanCurrentFolder,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.begin_scan(window, cx);
    }

    fn begin_scan(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let remote_hosts = self.config.remote_hosts.clone();
        let roots = self.config.scan_dirs.clone();
        if !has_scan_sources(roots.len(), remote_hosts.len()) {
            self.launch_error =
                Some("Add at least one local folder or SSH source before scanning.".into());
            cx.notify();
            return;
        }

        let generation = self.scan.begin();
        let max_depth = self.config.max_depth;
        Self::cancel_token(&mut self.scan_cancellation);
        let cancellation = CancellationToken::new();
        self.scan_cancellation = Some(cancellation.clone());

        self.clear_filter(window, cx);
        self.selected = None;
        self.reset_workspace(cx);
        self.launch_error = None;
        cx.notify();

        let scan_task = cx.background_executor().spawn(async move {
            let (available_roots, mut errors) = partition_local_scan_roots(roots);
            let mut successful_sources = usize::from(!available_roots.is_empty());
            let mut projects =
                scan_directories_cancellable(&available_roots, max_depth, &cancellation)?;
            for host in remote_hosts {
                cancellation.check()?;
                match scan_remote_host_cancellable(&host, &cancellation) {
                    Ok(mut remote_projects) => {
                        successful_sources += 1;
                        projects.append(&mut remote_projects);
                    }
                    Err(error) => errors.push(format!("{}: {error}", host.label())),
                }
            }
            if successful_sources == 0 && !errors.is_empty() {
                return Err(errors.join(" | "));
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
                    this.scan_cancellation = None;
                    this.selected = None;
                    this.launch_error =
                        (!remote_errors.is_empty()).then(|| remote_errors.join(" | "));
                    if matches!(this.scan.state, ScanState::Loaded { .. } | ScanState::Empty) {
                        match save_projects_with_diagnostics(&this.scan.projects) {
                            Ok(report) => {
                                this.persistence_history.record_events(report.events);
                            }
                            Err(error) => {
                                this.record_persistence_failure(error);
                            }
                        }
                        this.restore_last_project(cx);
                    }
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn set_selected(&mut self, project_index: usize, cx: &mut Context<Self>) {
        if project_index >= self.scan.projects.len() {
            return;
        }
        if self.selected == Some(project_index) {
            return;
        }
        let project = self.scan.projects[project_index].clone();
        let owner = project_locator(&project);
        if self.terminal_owner.as_ref() != Some(&owner) {
            self.end_terminal_session();
        }
        self.selected = Some(project_index);
        self.missing_project = None;
        self.remember_project(&project);
        if let Some(row) = visible_project_row(&self.filtered_indices(), self.selected) {
            self.project_scroll
                .scroll_to_item(row, ScrollStrategy::Center);
        }
        self.reset_workspace(cx);
        if project.has_git {
            self.load_git_branches(cx);
        }
        match self.activity {
            Activity::Overview => self.load_readme(cx),
            Activity::Files => self.load_tree(cx),
            Activity::Search => {}
            Activity::Git => {
                self.load_git_status(cx);
                self.start_git_auto_refresh(cx);
            }
            Activity::History => {
                self.load_history(cx);
            }
        }
    }

    fn filtered_indices(&self) -> Vec<usize> {
        self.project_indices_for_query(&self.filter_query)
    }

    fn project_indices_for_query(&self, query: &str) -> Vec<usize> {
        let mut indices = filtered_project_indices(&self.scan.projects, query);
        indices.retain(|&idx| {
            self.scan
                .projects
                .get(idx)
                .is_some_and(|p| !self.is_hidden(p))
        });
        let pinned: std::collections::HashSet<&std::path::PathBuf> =
            self.config.pinned_projects.iter().collect();
        indices.sort_by_key(|&idx| {
            let project = &self.scan.projects[idx];
            (
                if pinned.contains(&project.path) { 0 } else { 1 },
                project.source.label().to_lowercase(),
                project.name.to_lowercase(),
                project.path.to_string_lossy().to_lowercase(),
            )
        });
        indices
    }

    fn launcher_project_indices(&self) -> Vec<usize> {
        self.project_indices_for_query(&self.launcher_query)
    }

    fn on_filter_input_event(
        &mut self,
        state: &Entity<InputState>,
        event: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::Change => {
                self.filter_query = state.read(cx).value().to_lowercase();
                if self.scan.state == ScanState::Scanning {
                    self.selected = None;
                    cx.notify();
                } else {
                    self.clamp_selection_to_filter(cx);
                }
            }
            InputEvent::PressEnter { .. } => {
                self.open_selected_project(&OpenSelectedProject, window, cx);
            }
            _ => {}
        }
    }

    fn on_launcher_input_event(
        &mut self,
        state: &Entity<InputState>,
        event: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::Change => {
                let query = state.read(cx).value().to_lowercase();
                if query != self.launcher_query {
                    self.launcher_query = query;
                    self.launcher_selected = 0;
                    self.launcher_scroll.scroll_to_item(0);
                    self.preview_selected_theme(cx);
                    cx.notify();
                }
            }
            InputEvent::PressEnter { .. } => self.accept_launcher_selection(window, cx),
            _ => {}
        }
    }

    fn on_search_input_event(
        &mut self,
        state: &Entity<InputState>,
        event: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::Change => {
                Self::cancel_token(&mut self.search_cancellation);
                self.search_query = state.read(cx).value().to_string();
                self.search_generation = self.search_generation.wrapping_add(1);
                self.search_state = LoadState::Idle;
                cx.notify();
            }
            InputEvent::PressEnter { .. } => self.execute_search(window, cx),
            _ => {}
        }
    }

    fn on_git_commit_input_event(
        &mut self,
        state: &Entity<InputState>,
        event: &InputEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::Change => {
                self.git_commit_message = state.read(cx).value().to_string();
                self.git_commit_menu_open = false;
                cx.notify();
            }
            InputEvent::PressEnter { secondary: true } => self.commit_git(cx),
            _ => {}
        }
    }

    fn git_selections(status: &GitStatus) -> Vec<GitSelection> {
        let mut selections = Vec::new();
        selections.extend(
            status
                .changes
                .iter()
                .filter(|change| change.is_conflicted())
                .cloned()
                .map(|change| GitSelection {
                    change,
                    kind: GitDiffKind::Unstaged,
                }),
        );
        selections.extend(
            status
                .changes
                .iter()
                .filter(|change| change.is_unstaged() && !change.is_conflicted())
                .cloned()
                .map(|change| GitSelection {
                    change,
                    kind: GitDiffKind::Unstaged,
                }),
        );
        selections.extend(
            status
                .changes
                .iter()
                .filter(|change| change.is_staged() && !change.is_conflicted())
                .cloned()
                .map(|change| GitSelection {
                    change,
                    kind: GitDiffKind::Staged,
                }),
        );
        selections
    }

    fn load_git_status(&mut self, cx: &mut Context<Self>) {
        self.load_git_status_inner(false, false, true, cx);
    }

    fn reload_git_status_after_operation(&mut self, cx: &mut Context<Self>) {
        self.load_git_status_inner(true, false, true, cx);
    }

    fn auto_refresh_git_status(&mut self, include_stats: bool, cx: &mut Context<Self>) {
        if self.activity != Activity::Git
            || self.git_operation_running()
            || self.git_status_cancellation.is_some()
        {
            return;
        }
        self.load_git_status_inner(true, true, include_stats, cx);
    }

    fn load_git_status_inner(
        &mut self,
        preserve_notice: bool,
        quiet: bool,
        include_stats: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = self.selected_project().cloned() else {
            return;
        };
        self.git_status_generation = self.git_status_generation.wrapping_add(1);
        let generation = self.git_status_generation;
        Self::cancel_token(&mut self.git_status_cancellation);
        let cancellation = CancellationToken::new();
        self.git_status_cancellation = Some(cancellation.clone());
        if !quiet {
            Self::cancel_token(&mut self.git_diff_cancellation);
            self.git_status_state = LoadState::Loading;
            self.git_diff_state = LoadState::Idle;
        }
        if !preserve_notice {
            self.git_notice = None;
        }
        cx.notify();

        let request_project = project.clone();
        let task = cx.background_executor().spawn(async move {
            if include_stats {
                git_status_cancellable(&project, &cancellation)
            } else {
                git_status_summary_cancellable(&project, &cancellation)
            }
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.git_status_generation != generation
                    || this.selected_project() != Some(&request_project)
                {
                    return;
                }
                this.git_status_cancellation = None;
                match result {
                    Ok(mut status) => {
                        if !include_stats {
                            if let LoadState::Loaded(previous) = &this.git_status_state {
                                status.inherit_line_stats(previous);
                            }
                        }
                        let selections = Self::git_selections(&status);
                        let selected = this
                            .git_selection
                            .clone()
                            .filter(|selected| selections.contains(selected))
                            .or_else(|| selections.first().cloned());
                        this.git_selection = selected;
                        this.git_status_state = LoadState::Loaded(status);
                        if this
                            .git_notice
                            .as_ref()
                            .is_some_and(|notice| notice.automatic)
                        {
                            this.git_notice = None;
                        }
                        this.load_git_diff_inner(quiet, cx);
                    }
                    Err(error) => {
                        if !quiet {
                            this.git_selection = None;
                            this.git_diff_state = LoadState::Idle;
                            this.git_status_state = LoadState::Error(error.status_text().into());
                        }
                        this.git_notice = Some(GitNotice {
                            text: git_error_notice(&error),
                            tone: GitNoticeTone::Error,
                            automatic: quiet,
                        });
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn load_git_diff(&mut self, cx: &mut Context<Self>) {
        self.load_git_diff_inner(false, cx);
    }

    fn load_git_branches(&mut self, cx: &mut Context<Self>) {
        let Some(project) = self.selected_project().cloned() else {
            return;
        };
        self.git_branches_generation = self.git_branches_generation.wrapping_add(1);
        let generation = self.git_branches_generation;
        Self::cancel_token(&mut self.git_branches_cancellation);
        let cancellation = CancellationToken::new();
        self.git_branches_cancellation = Some(cancellation.clone());
        if !matches!(self.git_branches_state, LoadState::Loaded(_)) {
            self.git_branches_state = LoadState::Loading;
        }
        cx.notify();

        let request_project = project.clone();
        let task = cx
            .background_executor()
            .spawn(async move { git_branches_cancellable(&project, &cancellation) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.git_branches_generation != generation
                    || this.selected_project() != Some(&request_project)
                {
                    return;
                }
                this.git_branches_cancellation = None;
                this.git_branches_state = match result {
                    Ok(branches) if branches.is_empty() => LoadState::Empty,
                    Ok(branches) => LoadState::Loaded(branches),
                    Err(error) => LoadState::Error(git_error_notice(&error)),
                };
                if this.launcher == Some(LauncherMode::Branches) {
                    this.launcher_selected = this
                        .launcher_branches()
                        .iter()
                        .position(|branch| branch.current)
                        .unwrap_or_default();
                    this.launcher_scroll.scroll_to_item(this.launcher_selected);
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn open_branch_launcher(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_launcher(LauncherMode::Branches, window, cx);
        self.load_git_branches(cx);
    }

    fn load_history(&mut self, cx: &mut Context<Self>) {
        let Some(project) = self.selected_project().cloned() else {
            return;
        };
        self.history_generation = self.history_generation.wrapping_add(1);
        let generation = self.history_generation;
        Self::cancel_token(&mut self.history_cancellation);
        let cancellation = CancellationToken::new();
        self.history_cancellation = Some(cancellation.clone());
        self.history_state = LoadState::Loading;
        self.history_commits.clear();
        self.history_selected = None;
        self.history_selected_file = None;
        Self::cancel_token(&mut self.history_diff_cancellation);
        self.history_diff_generation = self.history_diff_generation.wrapping_add(1);
        self.history_diff_state = LoadState::Idle;
        self.history_skip = 0;
        self.history_has_more = true;
        self.history_more_error = None;
        self.history_open_error = None;
        cx.notify();

        let request_project = project.clone();
        let task = cx.background_executor().spawn(async move {
            git_log_cancellable(&project, HISTORY_PAGE_SIZE, 0, &cancellation)
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.history_generation != generation
                    || this.selected_project() != Some(&request_project)
                {
                    return;
                }
                this.history_cancellation = None;
                match result {
                    Ok(commits) if commits.is_empty() => {
                        this.history_state = LoadState::Empty;
                        this.history_has_more = false;
                    }
                    Ok(commits) => {
                        this.history_commits = commits;
                        this.history_skip = HISTORY_PAGE_SIZE;
                        this.history_has_more = this.history_commits.len() >= HISTORY_PAGE_SIZE;
                        this.history_state = LoadState::Loaded(());
                    }
                    Err(error) => {
                        this.history_state = LoadState::Error(git_error_notice(&error));
                    }
                };
                cx.notify();
            });
        })
        .detach();
    }

    fn load_more_history(&mut self, cx: &mut Context<Self>) {
        if !self.history_has_more || self.history_more_cancellation.is_some() {
            return;
        }
        let Some(project) = self.selected_project().cloned() else {
            return;
        };
        let generation = self.history_generation;
        let skip = self.history_skip;
        let cancellation = CancellationToken::new();
        self.history_more_cancellation = Some(cancellation.clone());
        self.history_more_error = None;
        cx.notify();

        let request_project = project.clone();
        let task = cx.background_executor().spawn(async move {
            git_log_cancellable(&project, HISTORY_PAGE_SIZE, skip, &cancellation)
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.history_generation != generation
                    || this.selected_project() != Some(&request_project)
                {
                    return;
                }
                this.history_more_cancellation = None;
                match result {
                    Ok(commits) => {
                        let count = commits.len();
                        this.history_commits.extend(commits);
                        this.history_skip = this.history_skip.saturating_add(count);
                        this.history_has_more = count >= HISTORY_PAGE_SIZE;
                    }
                    Err(error) => {
                        this.history_has_more = false;
                        this.history_more_error = Some(git_error_notice(&error));
                    }
                };
                cx.notify();
            });
        })
        .detach();
    }

    fn load_history_file_diff(
        &mut self,
        commit_hash: String,
        path: String,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = self.selected_project().cloned() else {
            return;
        };
        self.history_selected_file = Some(path.clone());
        self.history_diff_generation = self.history_diff_generation.wrapping_add(1);
        let generation = self.history_diff_generation;
        Self::cancel_token(&mut self.history_diff_cancellation);
        let cancellation = CancellationToken::new();
        self.history_diff_cancellation = Some(cancellation.clone());
        self.history_diff_state = LoadState::Loading;
        cx.notify();

        let request_project = project.clone();
        let request_path = path.clone();
        let task = cx.background_executor().spawn(async move {
            git_commit_diff_cancellable(&project, &commit_hash, &path, &cancellation)
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.history_diff_generation != generation
                    || this.selected_project() != Some(&request_project)
                    || this.history_selected_file.as_deref() != Some(&request_path)
                {
                    return;
                }
                this.history_diff_cancellation = None;
                this.history_diff_state = match result {
                    Ok(diff) if diff.trim().is_empty() => LoadState::Empty,
                    Ok(diff) => LoadState::Loaded(diff),
                    Err(error) => LoadState::Error(git_error_notice(&error)),
                };
                cx.notify();
            });
        })
        .detach();
    }

    fn close_history_file_diff(&mut self, cx: &mut Context<Self>) {
        Self::cancel_token(&mut self.history_diff_cancellation);
        self.history_diff_generation = self.history_diff_generation.wrapping_add(1);
        self.history_selected_file = None;
        self.history_diff_state = LoadState::Idle;
        cx.notify();
    }

    fn load_git_diff_inner(&mut self, quiet: bool, cx: &mut Context<Self>) {
        let (Some(project), Some(selection)) =
            (self.selected_project().cloned(), self.git_selection.clone())
        else {
            self.git_diff_state = LoadState::Idle;
            cx.notify();
            return;
        };
        self.git_diff_generation = self.git_diff_generation.wrapping_add(1);
        let generation = self.git_diff_generation;
        Self::cancel_token(&mut self.git_diff_cancellation);
        let cancellation = CancellationToken::new();
        self.git_diff_cancellation = Some(cancellation.clone());
        if !quiet {
            self.git_diff_state = LoadState::Loading;
        }
        cx.notify();

        let request_project = project.clone();
        let request_selection = selection.clone();
        let task = cx.background_executor().spawn(async move {
            git_diff_cancellable(&project, &selection.change, selection.kind, &cancellation)
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.git_diff_generation != generation
                    || this.selected_project() != Some(&request_project)
                    || this.git_selection.as_ref() != Some(&request_selection)
                {
                    return;
                }
                this.git_diff_cancellation = None;
                this.git_diff_state = match result {
                    Ok(diff) if diff.trim().is_empty() => LoadState::Empty,
                    Ok(diff) => LoadState::Loaded(diff),
                    Err(error) => LoadState::Error(error.status_text().into()),
                };
                cx.notify();
            });
        })
        .detach();
    }

    fn start_git_auto_refresh(&mut self, cx: &mut Context<Self>) {
        let Some(project) = self.selected_project().cloned() else {
            return;
        };
        self.git_refresh_generation = self.git_refresh_generation.wrapping_add(1);
        let generation = self.git_refresh_generation;

        if project.source.is_remote() {
            cx.spawn(async move |this, cx| {
                let mut polls = 0usize;
                loop {
                    cx.background_executor()
                        .timer(GIT_REMOTE_POLL_INTERVAL)
                        .await;
                    polls = polls.wrapping_add(1);
                    let active = this
                        .update(cx, |this, cx| {
                            if this.git_refresh_generation != generation
                                || this.activity != Activity::Git
                                || this.show_settings
                                || this.selected_project() != Some(&project)
                            {
                                return false;
                            }
                            this.auto_refresh_git_status(polls.is_multiple_of(3), cx);
                            true
                        })
                        .unwrap_or(false);
                    if !active {
                        break;
                    }
                }
            })
            .detach();
            return;
        }

        let root = project.path.clone();
        let git_directory = local_git_directory(&root);
        cx.spawn(async move |this, cx| {
            let (sender, receiver) =
                std::sync::mpsc::sync_channel::<notify::Result<notify::Event>>(256);
            let mut watcher = notify::recommended_watcher(move |event| {
                let _ = sender.try_send(event);
            })
            .ok();
            let watching = watcher.as_mut().is_some_and(|watcher| {
                if watcher.watch(&root, RecursiveMode::Recursive).is_err() {
                    return false;
                }
                git_directory.starts_with(&root)
                    || watcher
                        .watch(&git_directory, RecursiveMode::Recursive)
                        .is_ok()
            });
            let mut dirty = false;
            let mut quiet_ticks = 0usize;
            let mut fallback_ticks = 0usize;

            loop {
                cx.background_executor().timer(GIT_WATCH_TICK).await;
                let active = this
                    .update(cx, |this, _| {
                        this.git_refresh_generation == generation
                            && this.activity == Activity::Git
                            && !this.show_settings
                            && this.selected_project() == Some(&project)
                    })
                    .unwrap_or(false);
                if !active {
                    break;
                }

                if watching {
                    let mut saw_relevant_event = false;
                    while let Ok(event) = receiver.try_recv() {
                        if event.as_ref().is_ok_and(|event| {
                            git_watch_event_is_relevant(&root, &git_directory, event)
                        }) {
                            saw_relevant_event = true;
                        }
                    }
                    if saw_relevant_event {
                        dirty = true;
                        quiet_ticks = 0;
                    } else if dirty {
                        quiet_ticks += 1;
                    }
                    if dirty && quiet_ticks >= GIT_WATCH_QUIET_TICKS {
                        let _ = this.update(cx, |this, cx| this.auto_refresh_git_status(true, cx));
                        dirty = false;
                        quiet_ticks = 0;
                    }
                } else {
                    fallback_ticks += 1;
                    if fallback_ticks >= 10 {
                        let _ = this.update(cx, |this, cx| this.auto_refresh_git_status(true, cx));
                        fallback_ticks = 0;
                    }
                }
            }
        })
        .detach();
    }

    fn select_git_change(&mut self, selection: GitSelection, cx: &mut Context<Self>) {
        self.git_selection = Some(selection);
        self.load_git_diff(cx);
    }

    fn select_relative_git_change(&mut self, delta: isize, cx: &mut Context<Self>) {
        let LoadState::Loaded(status) = &self.git_status_state else {
            return;
        };
        let selections = Self::git_selections(status);
        if selections.is_empty() {
            return;
        }
        let current = self
            .git_selection
            .as_ref()
            .and_then(|selected| selections.iter().position(|item| item == selected))
            .unwrap_or(0);
        let next = if delta < 0 {
            current.checked_sub(1).unwrap_or(selections.len() - 1)
        } else {
            (current + 1) % selections.len()
        };
        self.git_selection = Some(selections[next].clone());
        self.git_scroll.scroll_to_item(next);
        self.load_git_diff(cx);
    }

    fn select_git_edge(&mut self, last: bool, cx: &mut Context<Self>) {
        let LoadState::Loaded(status) = &self.git_status_state else {
            return;
        };
        let selections = Self::git_selections(status);
        let Some(index) =
            (!selections.is_empty()).then(|| if last { selections.len() - 1 } else { 0 })
        else {
            return;
        };
        self.git_selection = Some(selections[index].clone());
        self.git_scroll.scroll_to_item(index);
        self.load_git_diff(cx);
    }

    fn select_relative_history_commit(&mut self, delta: isize, cx: &mut Context<Self>) {
        if self.history_commits.is_empty() {
            return;
        }
        let current = self.history_selected.unwrap_or_else(|| {
            if delta < 0 {
                0
            } else {
                self.history_commits.len().saturating_sub(1)
            }
        });
        let next = if delta < 0 {
            current
                .checked_sub(1)
                .unwrap_or(self.history_commits.len() - 1)
        } else {
            (current + 1) % self.history_commits.len()
        };
        self.select_history_commit(next, cx);
    }

    fn select_history_edge(&mut self, last: bool, cx: &mut Context<Self>) {
        let Some(index) = (!self.history_commits.is_empty()).then(|| {
            if last {
                self.history_commits.len() - 1
            } else {
                0
            }
        }) else {
            return;
        };
        self.select_history_commit(index, cx);
    }

    fn select_history_commit(&mut self, index: usize, cx: &mut Context<Self>) {
        if index >= self.history_commits.len() {
            return;
        }
        self.close_history_file_diff(cx);
        self.history_selected = Some(index);
        self.history_open_error = None;
        self.history_scroll
            .scroll_to_item(index, ScrollStrategy::Center);
        if index.saturating_add(5) >= self.history_commits.len() {
            self.load_more_history(cx);
        }
        cx.notify();
    }

    fn close_history_sidebar(&mut self, cx: &mut Context<Self>) {
        self.close_history_file_diff(cx);
        self.history_selected = None;
        self.history_open_error = None;
        cx.notify();
    }

    fn run_git_action(&mut self, action: GitAction, cx: &mut Context<Self>) {
        let Some(project) = self.selected_project().cloned() else {
            return;
        };
        self.git_operation_generation = self.git_operation_generation.wrapping_add(1);
        let generation = self.git_operation_generation;
        Self::cancel_token(&mut self.git_operation_cancellation);
        let cancellation = CancellationToken::new();
        self.git_operation_cancellation = Some(cancellation.clone());
        self.git_pending_discard = None;
        self.git_context_menu = None;
        self.git_notice = Some(GitNotice {
            text: match &action {
                GitAction::Fetch => "Fetching remotes...".into(),
                GitAction::Push { .. } => "Pushing current branch...".into(),
                GitAction::SwitchBranch(branch) => format!("Switching to {branch}..."),
                GitAction::Commit { amend: true, .. } => "Amending commit...".into(),
                GitAction::Commit { .. } => "Creating commit...".into(),
                _ => "Updating working tree...".into(),
            },
            tone: GitNoticeTone::Working,
            automatic: false,
        });
        cx.notify();

        let clears_commit_message = matches!(action, GitAction::Commit { .. });
        let refreshes_branches = matches!(action, GitAction::SwitchBranch(_));
        let request_project = project.clone();
        let task = cx.background_executor().spawn(async move {
            match action {
                GitAction::Stage(paths) => git_stage_cancellable(&project, &paths, &cancellation),
                GitAction::StageAll => git_stage_all_cancellable(&project, &cancellation),
                GitAction::Unstage(paths) => {
                    git_unstage_cancellable(&project, &paths, &cancellation)
                }
                GitAction::UnstageAll => git_unstage_all_cancellable(&project, &cancellation),
                GitAction::Discard(change) => {
                    git_discard_cancellable(&project, &change, &cancellation)
                }
                GitAction::Commit { message, amend } => {
                    git_commit_cancellable(&project, &message, amend, &cancellation)
                }
                GitAction::Fetch => git_fetch_cancellable(&project, &cancellation),
                GitAction::Push { set_upstream } => {
                    git_push_cancellable(&project, set_upstream, &cancellation)
                }
                GitAction::SwitchBranch(branch) => {
                    git_switch_branch_cancellable(&project, &branch, &cancellation)
                }
            }
        });
        cx.spawn(async move |this, cx| {
            let result: Result<GitOperationResult, _> = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.git_operation_generation != generation
                    || this.selected_project() != Some(&request_project)
                {
                    return;
                }
                this.git_operation_cancellation = None;
                match result {
                    Ok(result) => {
                        this.git_notice = Some(GitNotice {
                            text: result.summary,
                            tone: GitNoticeTone::Success,
                            automatic: false,
                        });
                        if clears_commit_message {
                            this.clear_git_commit_message(cx);
                            this.git_amend = false;
                            this.git_commit_menu_open = false;
                        }
                        this.reload_git_status_after_operation(cx);
                        if refreshes_branches {
                            this.load_git_branches(cx);
                        }
                    }
                    Err(error) => {
                        this.git_notice = Some(GitNotice {
                            text: git_error_notice(&error),
                            tone: GitNoticeTone::Error,
                            automatic: false,
                        });
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }

    fn commit_git(&mut self, cx: &mut Context<Self>) {
        if self.git_commit_message.trim().is_empty()
            || (!self.git_amend && !self.git_has_staged_changes())
        {
            return;
        }
        self.run_git_action(
            GitAction::Commit {
                message: self.git_commit_message.clone(),
                amend: self.git_amend,
            },
            cx,
        );
    }

    fn git_has_staged_changes(&self) -> bool {
        matches!(&self.git_status_state, LoadState::Loaded(status) if status.staged_count() > 0)
    }

    fn git_operation_running(&self) -> bool {
        self.git_operation_cancellation.is_some()
    }

    fn launcher_branches(&self) -> Vec<GitBranch> {
        let LoadState::Loaded(branches) = &self.git_branches_state else {
            return Vec::new();
        };
        let query = self.launcher_query.trim().to_lowercase();
        branches
            .iter()
            .filter(|branch| query.is_empty() || branch.name.to_lowercase().contains(&query))
            .cloned()
            .collect()
    }

    fn launcher_editors(&self) -> Vec<DetectedEditor> {
        let Some(project) = self.selected_project() else {
            return Vec::new();
        };
        filtered_editors(&self.detected_editors, &self.launcher_query, project)
    }

    fn open_editor_launcher(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_project().is_none() {
            return;
        }
        if !self.editor_discovery_complete {
            self.detected_editors = detect_editors();
            self.editor_discovery_complete = true;
        }
        self.open_launcher(LauncherMode::Editors, window, cx);
    }

    fn open_launcher(&mut self, mode: LauncherMode, window: &mut Window, cx: &mut Context<Self>) {
        self.context_menu = None;
        self.git_context_menu = None;
        self.project_catalog_open = false;
        self.resize_target = None;
        self.launcher = Some(mode);
        self.launcher_query.clear();
        if mode == LauncherMode::Themes {
            self.pending_theme = self.config.theme;
            self.pending_appearance = self.config.appearance;
        }
        self.launcher_input.update(cx, |input, cx| {
            input.set_value("", window, cx);
            input.focus(window, cx);
        });
        self.launcher_selected = match mode {
            LauncherMode::Branches => self
                .launcher_branches()
                .iter()
                .position(|branch| branch.current)
                .unwrap_or_default(),
            LauncherMode::Projects => self
                .selected
                .and_then(|current| {
                    self.launcher_project_indices()
                        .iter()
                        .position(|candidate| *candidate == current)
                })
                .unwrap_or_default(),
            LauncherMode::Commands => 0,
            LauncherMode::Editors => 0,
            LauncherMode::Themes => filtered_themes("")
                .iter()
                .position(|selection| {
                    selection.is_active(self.config.theme, self.config.appearance)
                })
                .unwrap_or_default(),
        };
        self.launcher_scroll.scroll_to_item(self.launcher_selected);
        cx.notify();
    }

    fn close_launcher(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.launcher == Some(LauncherMode::Themes) {
            self.pending_theme = self.config.theme;
            self.pending_appearance = self.config.appearance;
        }
        self.launcher = None;
        self.launcher_query.clear();
        self.launcher_selected = 0;
        window.focus(&self.focus_handle);
        cx.notify();
    }

    fn launcher_item_count(&self) -> usize {
        match self.launcher {
            Some(LauncherMode::Branches) => self.launcher_branches().len(),
            Some(LauncherMode::Commands) => self.launcher_commands().len(),
            Some(LauncherMode::Editors) => self.launcher_editors().len(),
            Some(LauncherMode::Projects) => self.launcher_project_indices().len(),
            Some(LauncherMode::Themes) => filtered_themes(&self.launcher_query).len(),
            None => 0,
        }
    }

    fn preview_selected_theme(&mut self, cx: &mut Context<Self>) {
        if self.launcher != Some(LauncherMode::Themes) {
            return;
        }
        if let Some(selection) = filtered_themes(&self.launcher_query)
            .get(self.launcher_selected)
            .copied()
        {
            (self.pending_theme, self.pending_appearance) =
                selection.preferences(self.config.theme);
            cx.notify();
        }
    }

    fn accept_launcher_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.launcher {
            Some(LauncherMode::Branches) => {
                let Some(branch) = self
                    .launcher_branches()
                    .get(self.launcher_selected)
                    .cloned()
                else {
                    return;
                };
                self.close_launcher(window, cx);
                if !branch.current && !self.git_operation_running() {
                    self.run_git_action(GitAction::SwitchBranch(branch.name), cx);
                }
            }
            Some(LauncherMode::Commands) => {
                let Some(command) = self
                    .launcher_commands()
                    .get(self.launcher_selected)
                    .copied()
                else {
                    return;
                };
                self.close_launcher(window, cx);
                self.execute_command(command.id, window, cx);
            }
            Some(LauncherMode::Editors) => {
                let Some(editor) = self.launcher_editors().get(self.launcher_selected).cloned()
                else {
                    return;
                };
                self.close_launcher(window, cx);
                let Some(project) = self.selected_project().cloned() else {
                    return;
                };
                self.launch_error = editor.launch(&project).err();
                cx.notify();
            }
            Some(LauncherMode::Projects) => {
                let Some(&project_index) =
                    self.launcher_project_indices().get(self.launcher_selected)
                else {
                    return;
                };
                self.close_launcher(window, cx);
                self.show_settings = false;
                self.clear_filter(window, cx);
                self.set_selected(project_index, cx);
            }
            Some(LauncherMode::Themes) => {
                let Some(selection) = filtered_themes(&self.launcher_query)
                    .get(self.launcher_selected)
                    .copied()
                else {
                    return;
                };
                let (theme, appearance) = selection.preferences(self.config.theme);
                let mut config = self.config.clone();
                config.theme = theme;
                config.appearance = appearance;
                match config.save_with_diagnostics() {
                    Ok(report) => {
                        self.persistence_history.record_events(report.events);
                        self.config = config;
                        self.pending_theme = theme;
                        self.pending_appearance = appearance;
                    }
                    Err(error) => self.record_persistence_failure(error),
                }
                self.close_launcher(window, cx);
            }
            None => {}
        }
    }

    fn execute_command(&mut self, command: CommandId, window: &mut Window, cx: &mut Context<Self>) {
        if !self.command_enabled(command) {
            return;
        }
        match command {
            CommandId::ToggleProjectCatalog => self.toggle_project_catalog_open(window, cx),
            CommandId::ShowOverview => self.set_activity(Activity::Overview, window, cx),
            CommandId::ShowFiles => self.set_activity(Activity::Files, window, cx),
            CommandId::ShowSearch => self.set_activity(Activity::Search, window, cx),
            CommandId::ShowGit => self.set_activity(Activity::Git, window, cx),
            CommandId::ShowHistory => self.set_activity(Activity::History, window, cx),
            CommandId::RefreshGit => self.load_git_status(cx),
            CommandId::StageSelectedChange => {
                if let Some(selection) = &self.git_selection {
                    self.run_git_action(GitAction::Stage(vec![selection.change.path.clone()]), cx);
                }
            }
            CommandId::StageAllChanges => self.run_git_action(GitAction::StageAll, cx),
            CommandId::UnstageSelectedChange => {
                if let Some(selection) = &self.git_selection {
                    self.run_git_action(
                        GitAction::Unstage(vec![selection.change.path.clone()]),
                        cx,
                    );
                }
            }
            CommandId::UnstageAllChanges => self.run_git_action(GitAction::UnstageAll, cx),
            CommandId::DiscardSelectedChange => {
                self.git_pending_discard = self
                    .git_selection
                    .as_ref()
                    .filter(|selection| selection.kind == GitDiffKind::Unstaged)
                    .map(|selection| selection.change.clone());
                cx.notify();
            }
            CommandId::FocusGitCommit => {
                self.git_commit_input
                    .update(cx, |input, cx| input.focus(window, cx));
            }
            CommandId::FetchGitRemotes => self.run_git_action(GitAction::Fetch, cx),
            CommandId::PushGitBranch => {
                let set_upstream = matches!(
                    &self.git_status_state,
                    LoadState::Loaded(status) if status.upstream.is_none()
                );
                self.run_git_action(GitAction::Push { set_upstream }, cx);
            }
            CommandId::OpenInZed => self.launch_selected_in_zed(cx),
            CommandId::OpenInEditor => self.open_editor_launcher(window, cx),
            CommandId::ToggleProjectPin => {
                if let Some(index) = self.selected {
                    self.toggle_pin(index, cx);
                }
            }
            CommandId::HideProject => {
                if let Some(index) = self.selected {
                    self.toggle_hide(index, cx);
                }
            }
            CommandId::CopyProjectPath => {
                if let Some(path) = self
                    .selected_project()
                    .map(|project| project.path.to_string_lossy().into_owned())
                {
                    cx.write_to_clipboard(ClipboardItem::new_string(path));
                    self.show_copy_feedback("COPIED PROJECT PATH", cx);
                }
            }
            CommandId::RefreshProjects => self.begin_scan(window, cx),
            CommandId::ToggleContextPane => {
                self.context_pane_visible = !self.context_pane_visible;
                cx.notify();
            }
            CommandId::ToggleReadmePreview => {
                self.readme_preview = !self.readme_preview;
                cx.notify();
            }
            CommandId::ToggleFileWrap => {
                self.wrap_document = !self.wrap_document;
                cx.notify();
            }
            CommandId::SelectTheme => self.open_launcher(LauncherMode::Themes, window, cx),
            CommandId::ShowSettings => self.open_settings(window, cx),
            CommandId::ToggleTerminal => self.toggle_terminal(window, cx),
        }
    }

    fn command_enabled(&self, command: CommandId) -> bool {
        match command {
            CommandId::OpenInZed
            | CommandId::OpenInEditor
            | CommandId::ToggleProjectPin
            | CommandId::HideProject
            | CommandId::CopyProjectPath => self.selected_project().is_some(),
            CommandId::ToggleReadmePreview => {
                self.selected_project().is_some() && self.activity == Activity::Overview
            }
            CommandId::ToggleFileWrap => {
                self.activity == Activity::Files && self.selected_file.is_some()
            }
            CommandId::ToggleContextPane => {
                matches!(
                    self.activity,
                    Activity::Files | Activity::Search | Activity::Git
                )
            }
            CommandId::RefreshGit | CommandId::FocusGitCommit => {
                self.activity == Activity::Git
                    && self.selected_project().is_some()
                    && !self.git_operation_running()
            }
            CommandId::StageAllChanges => {
                self.activity == Activity::Git
                    && !self.git_operation_running()
                    && matches!(
                        &self.git_status_state,
                        LoadState::Loaded(status) if status.unstaged_count() > 0
                    )
            }
            CommandId::UnstageAllChanges => {
                self.activity == Activity::Git
                    && !self.git_operation_running()
                    && self.git_has_staged_changes()
            }
            CommandId::FetchGitRemotes | CommandId::PushGitBranch => {
                self.activity == Activity::Git
                    && !self.git_operation_running()
                    && self
                        .selected_project()
                        .is_some_and(|project| project.has_git)
                    && (command != CommandId::PushGitBranch
                        || matches!(
                            &self.git_status_state,
                            LoadState::Loaded(status)
                                if status.branch.as_deref().is_some_and(|branch| branch != "detached")
                        ))
            }
            CommandId::StageSelectedChange => {
                self.activity == Activity::Git
                    && !self.git_operation_running()
                    && self
                        .git_selection
                        .as_ref()
                        .is_some_and(|selection| selection.kind == GitDiffKind::Unstaged)
            }
            CommandId::UnstageSelectedChange => {
                self.activity == Activity::Git
                    && !self.git_operation_running()
                    && self
                        .git_selection
                        .as_ref()
                        .is_some_and(|selection| selection.kind == GitDiffKind::Staged)
            }
            CommandId::DiscardSelectedChange => {
                self.activity == Activity::Git
                    && !self.git_operation_running()
                    && self
                        .git_selection
                        .as_ref()
                        .is_some_and(|selection| selection.kind == GitDiffKind::Unstaged)
            }
            _ => true,
        }
    }

    fn launcher_commands(&self) -> Vec<CommandSpec> {
        filtered_commands(&self.launcher_query)
            .into_iter()
            .filter(|command| self.command_enabled(command.id))
            .collect()
    }

    fn clear_filter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.context_menu = None;
        self.filter_query.clear();
        self.filter_input
            .update(cx, |input, cx| input.set_value("", window, cx));
    }

    fn selected_project(&self) -> Option<&Project> {
        self.selected
            .and_then(|project_index| self.scan.projects.get(project_index))
    }

    fn clamp_selection_to_filter(&mut self, cx: &mut Context<Self>) {
        let filtered = self.filtered_indices();
        if self
            .selected
            .is_some_and(|selected| filtered.contains(&selected))
        {
            cx.notify();
        } else if let Some(&project_index) = filtered.first() {
            self.set_selected(project_index, cx);
            self.project_scroll.scroll_to_item(0, ScrollStrategy::Top);
        } else {
            self.end_terminal_session();
            self.selected = None;
            self.reset_workspace(cx);
        }
    }

    fn handle_filter_keydown(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();
        let mods = &event.keystroke.modifiers;

        if self.launcher.is_some() {
            return;
        }

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

        if self.document_focused && matches!(self.activity, Activity::Files | Activity::Search) {
            if key == "escape" {
                self.document_focused = false;
                if self.activity == Activity::Search {
                    self.search_input
                        .update(cx, |input, cx| input.focus(window, cx));
                } else {
                    window.focus(&self.focus_handle);
                }
                cx.notify();
                return;
            }
            if !mods.control && !mods.alt && !mods.function {
                return;
            }
        }

        if key == "escape" && self.activity == Activity::History {
            if self.history_selected_file.is_some() {
                self.close_history_file_diff(cx);
            } else if self.history_selected.is_some() {
                self.close_history_sidebar(cx);
            }
            return;
        }

        if key == "escape" && !self.filter_query.is_empty() {
            self.clear_filter(window, cx);
        }
    }

    fn copy_document(&mut self, cx: &mut Context<Self>) {
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

    fn open_selected_project(
        &mut self,
        _: &OpenSelectedProject,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.activity == Activity::Search && !self.project_catalog_open {
            return;
        }

        self.project_catalog_open = false;
        self.launch_selected_in_zed(cx);
    }

    fn launch_selected_in_zed(&mut self, cx: &mut Context<Self>) {
        let Some(project) = self.selected_project().cloned() else {
            return;
        };

        self.launch_error = open_project_in_zed(&project).err();
        cx.notify();
    }

    fn toggle_settings(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.show_settings {
            self.show_settings = false;
            self.pending_scan_dirs.clear();
            self.pending_remote_hosts.clear();
            self.pending_max_depth = 3;
            self.pending_theme = self.config.theme;
            self.pending_appearance = self.config.appearance;
            self.source_picker = SourcePicker::Closed;
            if self.activity == Activity::Git {
                self.start_git_auto_refresh(cx);
            }
        } else {
            self.open_settings(window, cx);
        }
        cx.notify();
    }

    fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.git_refresh_generation = self.git_refresh_generation.wrapping_add(1);
        self.show_settings = true;
        self.project_catalog_open = false;
        self.launcher = None;
        self.context_menu = None;
        self.pending_scan_dirs = self.config.scan_dirs.clone();
        self.pending_remote_hosts = self.config.remote_hosts.clone();
        self.pending_max_depth = self.config.max_depth;
        self.pending_theme = self.config.theme;
        self.pending_appearance = self.config.appearance;
        self.source_picker = SourcePicker::Closed;
        window.focus(&self.focus_handle);
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
        Self::cancel_token(&mut self.picker_cancellation);
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
        Self::cancel_token(&mut self.picker_cancellation);
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
        Self::cancel_token(&mut self.picker_cancellation);
        let cancellation = CancellationToken::new();
        self.picker_cancellation = Some(cancellation.clone());
        cx.notify();

        let task = cx
            .background_executor()
            .spawn(async move { list_remote_subdirs_cancellable(&host, &path, &cancellation) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.picker_generation != generation {
                    return;
                }
                this.picker_cancellation = None;
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
        Self::cancel_token(&mut self.picker_cancellation);
        self.picker_generation = self.picker_generation.wrapping_add(1);
        self.source_picker = SourcePicker::Closed;
        self.picker_entries = LoadState::Idle;
        cx.notify();
    }

    fn add_current_source(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        Self::cancel_token(&mut self.picker_cancellation);
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

    fn change_remote_depth(&mut self, host_index: usize, delta: isize, cx: &mut Context<Self>) {
        if let Some(host) = self.pending_remote_hosts.get_mut(host_index) {
            host.max_depth = host.max_depth.saturating_add_signed(delta).clamp(1, 20);
            cx.notify();
        }
    }

    fn save_settings(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        let sources_changed = scan_sources_changed(
            &self.config,
            &self.pending_scan_dirs,
            &self.pending_remote_hosts,
            self.pending_max_depth,
        );
        let mut config = self.config.clone();
        config.scan_dirs = self.pending_scan_dirs.clone();
        config.remote_hosts = self.pending_remote_hosts.clone();
        config.max_depth = self.pending_max_depth;
        config.theme = self.pending_theme;
        config.appearance = self.pending_appearance;
        config.normalize();
        match config.save_with_diagnostics() {
            Ok(report) => {
                self.persistence_history.record_events(report.events);
                self.config = config;
                self.show_settings = false;
                if sources_changed {
                    self.begin_scan(window, cx);
                } else if self.activity == Activity::Git {
                    self.start_git_auto_refresh(cx);
                }
            }
            Err(error) => self.record_persistence_failure(error),
        }
        cx.notify();
    }

    fn cancel_settings(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.show_settings = false;
        self.pending_scan_dirs.clear();
        self.pending_remote_hosts.clear();
        self.pending_max_depth = 3;
        self.pending_theme = self.config.theme;
        self.pending_appearance = self.config.appearance;
        self.source_picker = SourcePicker::Closed;
        if self.activity == Activity::Git {
            self.start_git_auto_refresh(cx);
        }
        cx.notify();
    }

    fn settings_panel(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        use gpui::{InteractiveElement, StatefulInteractiveElement};
        if !matches!(self.source_picker, SourcePicker::Closed) {
            return self.source_picker_panel(theme, cx);
        }

        let (local_hidden, remote_hidden): (Vec<_>, Vec<_>) = self
            .scan
            .projects
            .iter()
            .enumerate()
            .filter(|(_, project)| self.is_hidden(project))
            .map(|(index, _)| index)
            .partition(|index| !self.scan.projects[*index].source.is_remote());
        let has_local_hidden = !local_hidden.is_empty();
        let has_remote_hidden = !remote_hidden.is_empty();

        let local_sources = self
            .pending_scan_dirs
            .iter()
            .enumerate()
            .map(|(index, dir)| {
                div()
                    .id(("scan-dir-row", index))
                    .h(px(27.0))
                    .flex()
                    .items_center()
                    .gap_1()
                    .border_b_1()
                    .border_color(theme.border.opacity(0.35))
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .font_family(MONO_FONT)
                            .text_size(px(9.0))
                            .text_color(theme.text_muted)
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .child(dir.to_string_lossy().into_owned()),
                    )
                    .child(
                        Button::new(("remove-dir", index))
                            .icon(IconName::Delete)
                            .tooltip("Remove local source")
                            .xsmall()
                            .compact()
                            .ghost()
                            .on_click(cx.listener(move |this, event, window, cx| {
                                this.remove_scan_dir(index, event, window, cx);
                            })),
                    )
            });

        let local_hidden_rows = local_hidden.into_iter().filter_map(|index| {
            let project = self.scan.projects.get(index)?;
            Some(
                div()
                    .id(("hidden-local-project", index))
                    .h(px(25.0))
                    .flex()
                    .items_center()
                    .gap_1()
                    .border_b_1()
                    .border_color(theme.border.opacity(0.35))
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .font_family(MONO_FONT)
                            .text_size(px(9.0))
                            .text_color(theme.text_muted)
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .child(project.name.clone()),
                    )
                    .child(
                        Button::new(("unhide-local-project", index))
                            .icon(IconName::Eye)
                            .tooltip("Show project")
                            .xsmall()
                            .compact()
                            .ghost()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.unhide_project(index, cx);
                            })),
                    )
                    .into_any_element(),
            )
        });

        let remote_hosts =
            self.pending_remote_hosts
                .iter()
                .enumerate()
                .map(|(host_index, host)| {
                    let host_label = format!("{}  ({})", host.label(), host.host);
                    div()
                        .border_b_1()
                        .border_color(theme.border)
                        .py_1()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .h(px(24.0))
                                .flex()
                                .items_center()
                                .child(
                                    div()
                                        .min_w_0()
                                        .flex_1()
                                        .font_family(MONO_FONT)
                                        .text_size(px(9.0))
                                        .text_color(theme.text)
                                        .whitespace_nowrap()
                                        .overflow_hidden()
                                        .child(host_label),
                                )
                                .child(
                                    Button::new(("remove-ssh-host", host_index))
                                        .icon(IconName::Delete)
                                        .tooltip("Remove SSH source")
                                        .xsmall()
                                        .compact()
                                        .ghost()
                                        .on_click(cx.listener(move |this, event, window, cx| {
                                            this.remove_remote_host(host_index, event, window, cx);
                                        })),
                                ),
                        )
                        .child(
                            div()
                                .h(px(22.0))
                                .flex()
                                .items_center()
                                .gap_1()
                                .font_family(MONO_FONT)
                                .text_size(px(9.0))
                                .text_color(theme.text_muted)
                                .child("depth")
                                .child(
                                    Button::new(("remote-depth-down", host_index))
                                        .icon(IconName::Minus)
                                        .tooltip("Decrease SSH scan depth")
                                        .xsmall()
                                        .compact()
                                        .ghost()
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.change_remote_depth(host_index, -1, cx);
                                        })),
                                )
                                .child(host.max_depth.to_string())
                                .child(
                                    Button::new(("remote-depth-up", host_index))
                                        .icon(IconName::Plus)
                                        .tooltip("Increase SSH scan depth")
                                        .xsmall()
                                        .compact()
                                        .ghost()
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.change_remote_depth(host_index, 1, cx);
                                        })),
                                ),
                        )
                        .children(host.roots.iter().enumerate().map(|(root_index, root)| {
                            div()
                                .h(px(24.0))
                                .flex()
                                .items_center()
                                .gap_1()
                                .child(
                                    div()
                                        .min_w_0()
                                        .flex_1()
                                        .font_family(MONO_FONT)
                                        .text_size(px(9.0))
                                        .text_color(theme.text_muted)
                                        .whitespace_nowrap()
                                        .overflow_hidden()
                                        .child(root.clone()),
                                )
                                .child(
                                    Button::new(SharedString::from(format!(
                                        "remove-ssh-root-{host_index}-{root_index}"
                                    )))
                                    .icon(IconName::Delete)
                                    .tooltip("Remove remote folder")
                                    .xsmall()
                                    .compact()
                                    .ghost()
                                    .on_click(cx.listener(move |this, event, window, cx| {
                                        this.remove_remote_root(
                                            host_index, root_index, event, window, cx,
                                        );
                                    })),
                                )
                        }))
                        .child(
                            Button::new(("browse-ssh", host_index))
                                .icon(IconName::Plus)
                                .label("Remote folder")
                                .xsmall()
                                .compact()
                                .ghost()
                                .on_click(cx.listener(move |this, event, window, cx| {
                                    this.open_remote_picker(host_index, event, window, cx);
                                })),
                        )
                });

        let remote_hidden_rows = remote_hidden.into_iter().filter_map(|index| {
            let project = self.scan.projects.get(index)?;
            Some(
                div()
                    .id(("hidden-remote-project", index))
                    .h(px(25.0))
                    .flex()
                    .items_center()
                    .gap_1()
                    .border_b_1()
                    .border_color(theme.border.opacity(0.35))
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .font_family(MONO_FONT)
                            .text_size(px(9.0))
                            .text_color(theme.text_muted)
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .child(format!("{}  {}", project.source.label(), project.name)),
                    )
                    .child(
                        Button::new(("unhide-remote-project", index))
                            .icon(IconName::Eye)
                            .tooltip("Show project")
                            .xsmall()
                            .compact()
                            .ghost()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.unhide_project(index, cx);
                            })),
                    )
                    .into_any_element(),
            )
        });

        div()
            .min_h_0()
            .flex_1()
            .flex()
            .flex_col()
            .bg(theme.panel_background)
            .child(
                div()
                    .min_h_0()
                    .flex_1()
                    .flex()
                    .child(
                        div()
                            .id("local-settings-scroll")
                            .min_w_0()
                            .flex_1()
                            .overflow_y_scroll()
                            .px_3()
                            .py_2()
                            .border_r_1()
                            .border_color(theme.border)
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(section_label("LOCAL", theme))
                            .children(local_sources)
                            .child(
                                Button::new("add-scan-dir")
                                    .icon(IconName::Plus)
                                    .label("Local folder")
                                    .xsmall()
                                    .compact()
                                    .ghost()
                                    .on_click(cx.listener(Self::open_local_picker)),
                            )
                            .child(div().h(px(8.0)))
                            .child(section_label("SCAN DEPTH", theme))
                            .child(
                                div()
                                    .h(px(28.0))
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .child(
                                        Button::new("decrease-depth")
                                            .icon(IconName::Minus)
                                            .tooltip("Decrease local scan depth")
                                            .xsmall()
                                            .compact()
                                            .ghost()
                                            .on_click(cx.listener(Self::decrease_max_depth)),
                                    )
                                    .child(
                                        div()
                                            .w(px(20.0))
                                            .text_center()
                                            .font_family(MONO_FONT)
                                            .text_size(px(12.0))
                                            .text_color(theme.text)
                                            .child(self.pending_max_depth.to_string()),
                                    )
                                    .child(
                                        Button::new("increase-depth")
                                            .icon(IconName::Plus)
                                            .tooltip("Increase local scan depth")
                                            .xsmall()
                                            .compact()
                                            .ghost()
                                            .on_click(cx.listener(Self::increase_max_depth)),
                                    ),
                            )
                            .when(has_local_hidden, |pane| {
                                pane.child(div().h(px(8.0)))
                                    .child(section_label("HIDDEN PROJECTS", theme))
                                    .children(local_hidden_rows)
                            }),
                    )
                    .child(
                        div()
                            .id("remote-settings-scroll")
                            .min_w_0()
                            .flex_1()
                            .overflow_y_scroll()
                            .px_3()
                            .py_2()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(section_label("SSH", theme))
                            .children(remote_hosts)
                            .child(
                                div()
                                    .pt_1()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(
                                        Input::new(&self.remote_name_input)
                                            .h(px(24.0))
                                            .bg(theme.surface_background)
                                            .border_color(theme.border),
                                    )
                                    .child(
                                        Input::new(&self.remote_host_input)
                                            .h(px(24.0))
                                            .bg(theme.surface_background)
                                            .border_color(theme.border),
                                    )
                                    .child(
                                        Button::new("add-ssh-host")
                                            .icon(IconName::Plus)
                                            .label("Add host")
                                            .xsmall()
                                            .compact()
                                            .primary()
                                            .on_click(cx.listener(Self::add_remote_host)),
                                    ),
                            )
                            .when(has_remote_hidden, |pane| {
                                pane.child(div().h(px(8.0)))
                                    .child(section_label("HIDDEN PROJECTS", theme))
                                    .children(remote_hidden_rows)
                            }),
                    ),
            )
            .child(
                div()
                    .h(px(38.0))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap_1()
                    .px_3()
                    .border_t_1()
                    .border_color(theme.border)
                    .child(
                        Button::new("settings-cancel")
                            .label("Cancel")
                            .xsmall()
                            .compact()
                            .on_click(cx.listener(Self::cancel_settings)),
                    )
                    .child(
                        Button::new("settings-save")
                            .label("Save")
                            .xsmall()
                            .compact()
                            .primary()
                            .on_click(cx.listener(Self::save_settings)),
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
                    div().flex_1().child(
                        Input::new(&self.remote_path_input)
                            .h(px(24.0))
                            .bg(theme.surface_background)
                            .border_color(theme.border),
                    ),
                )
                .child(
                    div()
                        .id("picker-go-remote")
                        .h(px(24.0))
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
        if self.launcher.is_some() {
            let count = self.launcher_item_count();
            if count > 0 {
                self.launcher_selected = self.launcher_selected.checked_sub(1).unwrap_or(count - 1);
                self.launcher_scroll.scroll_to_item(self.launcher_selected);
                self.preview_selected_theme(cx);
                cx.notify();
            }
            return;
        }
        if self.document_focused {
            return;
        }
        if self.activity == Activity::History {
            self.select_relative_history_commit(-1, cx);
            return;
        }
        if self.activity == Activity::Git {
            self.select_relative_git_change(-1, cx);
            return;
        }
        let filtered = self.filtered_indices();
        let current = visible_project_row(&filtered, self.selected);
        if let Some(row) = previous_selection(current, filtered.len()) {
            self.set_selected(filtered[row], cx);
        }
    }

    fn focus_project_filter(
        &mut self,
        _: &FocusProjectFilter,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.show_settings {
            return;
        }
        self.set_activity(Activity::Search, window, cx);
    }

    fn select_next_project(
        &mut self,
        _: &SelectNextProject,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.launcher.is_some() {
            let count = self.launcher_item_count();
            if count > 0 {
                self.launcher_selected = (self.launcher_selected + 1) % count;
                self.launcher_scroll.scroll_to_item(self.launcher_selected);
                self.preview_selected_theme(cx);
                cx.notify();
            }
            return;
        }
        if self.document_focused {
            return;
        }
        if self.activity == Activity::History {
            self.select_relative_history_commit(1, cx);
            return;
        }
        if self.activity == Activity::Git {
            self.select_relative_git_change(1, cx);
            return;
        }
        let filtered = self.filtered_indices();
        let current = visible_project_row(&filtered, self.selected);
        if let Some(row) = next_selection(current, filtered.len()) {
            self.set_selected(filtered[row], cx);
        }
    }

    fn select_first_project(
        &mut self,
        _: &SelectFirstProject,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.launcher.is_some() {
            if self.launcher_item_count() > 0 {
                self.launcher_selected = 0;
                self.launcher_scroll.scroll_to_item(0);
                self.preview_selected_theme(cx);
                cx.notify();
            }
            return;
        }
        if self.document_focused {
            return;
        }
        if self.activity == Activity::History {
            self.select_history_edge(false, cx);
            return;
        }
        if self.activity == Activity::Git {
            self.select_git_edge(false, cx);
            return;
        }
        if let Some(&project_index) = self.filtered_indices().first() {
            self.set_selected(project_index, cx);
        }
    }

    fn select_last_project(
        &mut self,
        _: &SelectLastProject,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.launcher.is_some() {
            let count = self.launcher_item_count();
            if count > 0 {
                self.launcher_selected = count - 1;
                self.launcher_scroll.scroll_to_item(count - 1);
                self.preview_selected_theme(cx);
                cx.notify();
            }
            return;
        }
        if self.document_focused {
            return;
        }
        if self.activity == Activity::History {
            self.select_history_edge(true, cx);
            return;
        }
        if self.activity == Activity::Git {
            self.select_git_edge(true, cx);
            return;
        }
        if let Some(&project_index) = self.filtered_indices().last() {
            self.set_selected(project_index, cx);
        }
    }

    fn set_activity(&mut self, activity: Activity, window: &mut Window, cx: &mut Context<Self>) {
        self.git_refresh_generation = self.git_refresh_generation.wrapping_add(1);
        self.show_settings = false;
        self.project_catalog_open = false;
        self.context_menu = None;
        self.git_context_menu = None;
        self.activity = activity;
        match activity {
            Activity::Files if matches!(self.tree_state, LoadState::Idle) => self.load_tree(cx),
            Activity::Overview if matches!(self.readme_state, LoadState::Idle) => {
                self.load_readme(cx)
            }
            Activity::Search => {
                self.context_pane_visible = true;
                self.search_input
                    .update(cx, |input, cx| input.focus(window, cx));
            }
            Activity::Git => {
                self.context_pane_visible = true;
                if matches!(self.git_status_state, LoadState::Idle) {
                    self.load_git_status(cx);
                }
                self.start_git_auto_refresh(cx);
                window.focus(&self.focus_handle);
            }
            Activity::History => {
                self.context_pane_visible = true;
                if matches!(self.history_state, LoadState::Idle) {
                    self.load_history(cx);
                }
                window.focus(&self.focus_handle);
            }
            _ => window.focus(&self.focus_handle),
        }
        cx.notify();
    }

    fn show_command_palette(
        &mut self,
        _: &ShowCommandPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_launcher(LauncherMode::Commands, window, cx);
    }

    fn show_project_switcher(
        &mut self,
        _: &ShowProjectSwitcher,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_launcher(LauncherMode::Projects, window, cx);
    }

    fn toggle_project_catalog(
        &mut self,
        _: &ToggleProjectCatalog,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_project_catalog_open(window, cx);
    }

    fn toggle_project_catalog_open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.show_settings = false;
        self.context_menu = None;
        self.launcher = None;
        self.launcher_query.clear();
        self.launcher_selected = 0;
        self.project_catalog_open = !self.project_catalog_open;
        self.resize_target = None;
        if self.project_catalog_open {
            self.filter_input
                .update(cx, |input, cx| input.focus(window, cx));
        } else {
            window.focus(&self.focus_handle);
        }
        cx.notify();
    }

    fn end_terminal_session(&mut self) {
        self.terminal_entity = None;
        self.terminal_owner = None;
        self.terminal_visible = false;
    }

    fn spawn_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = self.selected_project().cloned() else {
            return;
        };
        let launch = match TerminalLaunch::for_project(&project) {
            Ok(launch) => launch,
            Err(error) => {
                self.launch_error = Some(format!("Terminal: {error}"));
                cx.notify();
                return;
            }
        };
        let owner = project_locator(&project);
        let (cols, rows) = terminal_grid_size(window, self.terminal_height_px);
        let theme = Theme::for_preferences(
            self.config.theme,
            self.config.appearance,
            window.appearance(),
        );
        let entity = cx
            .new(|cx| TerminalPanel::new(cx, launch, cols, rows, theme.text, theme.app_background));
        entity.read(cx).focus(window);
        self.terminal_entity = Some(entity);
        self.terminal_owner = Some(owner);
        self.terminal_visible = true;
    }

    fn toggle_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = self.selected_project() else {
            return;
        };
        let owner = project_locator(project);
        let session_matches = self.terminal_owner.as_ref() == Some(&owner);
        let session_running = session_matches
            && self
                .terminal_entity
                .as_ref()
                .is_some_and(|entity| entity.read(cx).is_running());

        if !session_running {
            self.end_terminal_session();
            self.spawn_terminal(window, cx);
        } else if self.terminal_visible {
            self.terminal_visible = false;
            window.focus(&self.focus_handle);
        } else {
            self.terminal_visible = true;
            if let Some(entity) = self.terminal_entity.as_ref() {
                entity.read(cx).focus(window);
            }
        }
        self.resize_target = None;
        cx.notify();
    }

    fn collapse_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.terminal_visible = false;
        self.resize_target = None;
        window.focus(&self.focus_handle);
        cx.notify();
    }

    fn show_overview(&mut self, _: &ShowOverview, window: &mut Window, cx: &mut Context<Self>) {
        self.set_activity(Activity::Overview, window, cx);
    }

    fn show_files(&mut self, _: &ShowFiles, window: &mut Window, cx: &mut Context<Self>) {
        self.set_activity(Activity::Files, window, cx);
    }

    fn show_search(&mut self, _: &ShowSearch, window: &mut Window, cx: &mut Context<Self>) {
        self.set_activity(Activity::Search, window, cx);
    }

    fn show_git(&mut self, _: &ShowGit, window: &mut Window, cx: &mut Context<Self>) {
        self.set_activity(Activity::Git, window, cx);
    }

    fn toggle_context_pane(
        &mut self,
        _: &ToggleContextPane,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !matches!(
            self.activity,
            Activity::Files | Activity::Search | Activity::Git | Activity::History
        ) {
            return;
        }
        self.context_pane_visible = !self.context_pane_visible;
        self.resize_target = None;
        cx.notify();
    }

    fn dismiss_launcher(
        &mut self,
        _: &DismissLauncher,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.project_catalog_open {
            self.project_catalog_open = false;
            self.resize_target = None;
            window.focus(&self.focus_handle);
            cx.notify();
        } else {
            self.close_launcher(window, cx);
        }
    }

    fn accept_launcher(&mut self, _: &AcceptLauncher, window: &mut Window, cx: &mut Context<Self>) {
        self.accept_launcher_selection(window, cx);
    }

    fn load_tree(&mut self, cx: &mut Context<Self>) {
        let Some(project) = self.selected_project().cloned() else {
            return;
        };
        let show_hidden = self.show_hidden;
        self.tree_generation = self.tree_generation.wrapping_add(1);
        let generation = self.tree_generation;
        Self::cancel_token(&mut self.tree_cancellation);
        let cancellation = CancellationToken::new();
        self.tree_cancellation = Some(cancellation.clone());
        self.tree_state = LoadState::Loading;
        cx.notify();

        let request_project = project.clone();
        let task = cx.background_executor().spawn(async move {
            list_project_tree_cancellable(&project, 10, show_hidden, &cancellation)
        });
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
                this.tree_cancellation = None;

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

    fn load_readme(&mut self, cx: &mut Context<Self>) {
        let Some(project) = self.selected_project().cloned() else {
            return;
        };
        self.readme_generation = self.readme_generation.wrapping_add(1);
        let generation = self.readme_generation;
        Self::cancel_token(&mut self.readme_cancellation);
        let cancellation = CancellationToken::new();
        self.readme_cancellation = Some(cancellation.clone());
        self.readme_state = LoadState::Loading;
        cx.notify();

        let request_project = project.clone();
        let task = cx
            .background_executor()
            .spawn(async move { read_project_readme_cancellable(&project, &cancellation) });
        cx.spawn(async move |this, cx| {
            let content = task.await;
            let _ = this.update(cx, |this, cx| {
                let project_matches = this.selected_project() == Some(&request_project);
                if this.readme_generation != generation || !project_matches {
                    return;
                }
                this.readme_cancellation = None;
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
        Self::cancel_token(&mut self.file_cancellation);
        let cancellation = CancellationToken::new();
        self.file_cancellation = Some(cancellation.clone());
        self.file_state = LoadState::Loading;
        self.document_focused = true;
        self.copy_feedback = None;
        cx.notify();

        let request_path = path.clone();
        let request_project = project.clone();
        let task = cx
            .background_executor()
            .spawn(async move { read_project_file_cancellable(&project, &path, &cancellation) });

        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.file_generation != generation
                    || this.selected_file.as_ref() != Some(&request_path)
                    || this.selected_project() != Some(&request_project)
                {
                    return;
                }
                this.file_cancellation = None;
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
        Self::cancel_token(&mut self.search_cancellation);
        let cancellation = CancellationToken::new();
        self.search_cancellation = Some(cancellation.clone());
        self.search_state = LoadState::Loading;
        cx.notify();

        let request_project = project.clone();
        let request_query = query.clone();
        let task = cx.background_executor().spawn(async move {
            search_project_content_cancellable(&project, &query, &cancellation)
        });

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
                this.search_cancellation = None;
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
            if let Some(locator) = &self.missing_project {
                let app = cx.entity();
                return div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap_2()
                    .text_color(theme.text_muted)
                    .child(
                        div()
                            .text_size(px(15.0))
                            .text_color(theme.text)
                            .child("Project not found"),
                    )
                    .child(
                        div()
                            .max_w(px(560.0))
                            .font_family(MONO_FONT)
                            .text_size(px(10.0))
                            .text_color(theme.text_disabled)
                            .child(locator.path.to_string_lossy().into_owned()),
                    )
                    .child(
                        Button::new("forget-missing-project")
                            .icon(IconName::Delete)
                            .label("Remove entry")
                            .small()
                            .compact()
                            .danger()
                            .on_click(move |_, _, cx| {
                                app.update(cx, |this, cx| this.forget_missing_project(cx));
                            }),
                    )
                    .into_any_element();
            }
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

        match self.activity {
            Activity::Overview => self
                .overview_content(project, theme, window, cx)
                .into_any_element(),
            Activity::Files => self.files_content(theme, window, cx),
            Activity::Search => self.search_content(theme, window, cx),
            Activity::Git => self.git_content(theme, window, cx),
            Activity::History => self.commit_history_content(theme, window, cx),
        }
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
            highlight_theme: if theme.is_light {
                HighlightTheme::default_light()
            } else {
                HighlightTheme::default_dark()
            },
            is_dark: !theme.is_light,
            ..Default::default()
        };

        let readme: AnyElement = match &self.readme_state {
            LoadState::Loaded(document) => {
                if self.readme_preview {
                    TextView::markdown("project-readme", omit_markdown_images(document), window, cx)
                        .style(readme_style)
                        .selectable(true)
                        .scrollable(true)
                        .size_full()
                        .px_3()
                        .py_2()
                        .text_color(theme.text_muted)
                        .into_any_element()
                } else {
                    let editor =
                        window.use_keyed_state("readme-source-editor", cx, |window, cx| {
                            InputState::new(window, cx)
                                .code_editor("markdown")
                                .line_number(true)
                                .soft_wrap(true)
                                .default_value(document.clone())
                        });
                    if editor.read(cx).value().as_ref() != document {
                        editor.update(cx, |editor, cx| {
                            editor.set_highlighter("markdown", cx);
                            editor.set_value(document.clone(), window, cx);
                            editor.set_soft_wrap(true, window, cx);
                        });
                    }
                    Input::new(&editor)
                        .disabled(true)
                        .appearance(false)
                        .focus_bordered(false)
                        .p_0()
                        .bg(theme.panel_background)
                        .text_size(px(11.0))
                        .h_full()
                        .into_any_element()
                }
            }
            LoadState::Loading => workbench_message("loading README...", theme.text_disabled),
            LoadState::Error(error) => workbench_message(error.clone(), theme.error),
            LoadState::Idle | LoadState::Empty => {
                workbench_message("no README found", theme.text_disabled)
            }
        };
        let modified = project
            .last_modified
            .map(relative_modified)
            .unwrap_or_else(|| "unknown".into());
        let source_label = project.source.label().to_string();
        let app = cx.entity();

        div()
            .id("overview-panel")
            .size_full()
            .min_h_0()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex_shrink_0()
                    .flex()
                    .flex_col()
                    .gap_1()
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
                                Button::new("readme-mode-toggle")
                                    .label(if self.readme_preview {
                                        "Raw"
                                    } else {
                                        "Preview"
                                    })
                                    .tooltip("Toggle README rendering")
                                    .small()
                                    .compact()
                                    .ghost()
                                    .on_click(move |_, _, cx| {
                                        app.update(cx, |this, cx| {
                                            this.readme_preview = !this.readme_preview;
                                            cx.notify();
                                        });
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .h(px(20.0))
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .gap_1()
                            .font_family(MONO_FONT)
                            .text_size(px(10.0))
                            .text_color(theme.text_muted)
                            .child(
                                Icon::new(IconName::Folder)
                                    .size(px(12.0))
                                    .text_color(theme.text_disabled),
                            )
                            .child(
                                div()
                                    .min_w_0()
                                    .flex_1()
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .child(project.path.to_string_lossy().into_owned()),
                            ),
                    )
                    .child(
                        div()
                            .h(px(18.0))
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .gap_2()
                            .font_family(MONO_FONT)
                            .text_size(px(9.0))
                            .text_color(theme.text_disabled)
                            .child(project.project_type.label())
                            .child("·")
                            .child(source_label)
                            .when(project.has_git, |metadata| metadata.child("·").child("Git"))
                            .child("·")
                            .child(modified),
                    ),
            )
            .child(
                div()
                    .min_w_0()
                    .min_h_0()
                    .flex_1()
                    .overflow_hidden()
                    .child(readme),
            )
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
        let dragging = self.resize_target == Some(ResizeTarget::WorkspaceContext);
        let app = cx.entity();
        let tree_notice = match (listing.truncated, listing.warnings.len()) {
            (true, 0) => "500+ entries".to_string(),
            (false, 0) => String::new(),
            (true, warnings) => format!("500+ · {warnings} warning(s)"),
            (false, warnings) => format!("{warnings} warning(s)"),
        };

        let tree_panel =
            div()
                .id("tree-panel")
                .w(px(self.files_context_width_px))
                .h_full()
                .min_h_0()
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
                        .child(
                            Checkbox::new("hidden-toggle")
                                .label("Show hidden")
                                .checked(show_hidden)
                                .small()
                                .on_click(move |checked, _, cx| {
                                    app.update(cx, |this, cx| {
                                        this.show_hidden = *checked;
                                        this.expanded_dirs.clear();
                                        this.load_tree(cx);
                                    });
                                }),
                        )
                        .child(
                            div()
                                .ml_auto()
                                .font_family(MONO_FONT)
                                .text_size(px(9.0))
                                .text_color(theme.warning)
                                .child(tree_notice),
                        ),
                )
                .child(
                    div()
                        .id("files-scroll")
                        .min_h_0()
                        .flex_1()
                        .overflow_y_scroll()
                        .children(visible.iter().map(|&idx| {
                            let entry = &listing.entries[idx];
                            let indent = 6.0 + entry.depth as f32 * 14.0;
                            let is_selected = self.selected_file.as_ref() == Some(&entry.path);
                            let is_dir = entry.is_dir;
                            let is_expanded = is_dir && self.expanded_dirs.contains(&entry.path);
                            let name = entry.name.clone();
                            let disclosure = if is_expanded {
                                IconName::ChevronDown
                            } else {
                                IconName::ChevronRight
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
                                .child(div().w(px(14.0)).flex_shrink_0().child(if is_dir {
                                    Icon::new(disclosure)
                                        .size(px(13.0))
                                        .text_color(theme.text_muted)
                                } else {
                                    Icon::empty().size(px(13.0))
                                }))
                                .child(
                                    Icon::new(if is_dir {
                                        if is_expanded {
                                            IconName::FolderOpen
                                        } else {
                                            IconName::FolderClosed
                                        }
                                    } else {
                                        file_icon(&name)
                                    })
                                    .size(px(14.0))
                                    .text_color(if is_dir { theme.accent } else { text_col }),
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
                    this.resize_target = Some(ResizeTarget::WorkspaceContext);
                    cx.notify();
                }),
            );

        let code_panel_content = self.file_detail_content(theme, window, cx);

        let code_panel = div()
            .id("code-panel")
            .h_full()
            .min_h_0()
            .flex_1()
            .min_w_0()
            .child(code_panel_content);

        div()
            .id("split-pane")
            .size_full()
            .flex()
            .flex_row()
            .min_h_0()
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _event: &MouseUpEvent, _window, cx| {
                    if this.resize_target == Some(ResizeTarget::WorkspaceContext) {
                        this.resize_target = None;
                        cx.notify();
                    }
                }),
            )
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _window, cx| {
                if this.resize_target == Some(ResizeTarget::WorkspaceContext) {
                    let raw: f32 = event.position.x.into();
                    this.files_context_width_px = raw.clamp(130.0, 460.0);
                    cx.notify();
                }
            }))
            .when(self.context_pane_visible, |pane| {
                pane.child(tree_panel).child(handle)
            })
            .child(code_panel)
            .into_any_element()
    }

    fn file_detail_content(
        &self,
        theme: Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match (&self.selected_file, &self.file_state) {
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
        }
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
            .p_0()
            .bg(theme.panel_background)
            .text_size(px(11.0))
            .h_full();
        let editor_for_wrap = editor.clone();
        let app_for_copy = app.clone();
        let app_for_wrap = app.clone();

        div()
            .size_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .h(px(24.0))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap_2()
                    .pl_2()
                    .pr_1()
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
                        Button::new("document-copy")
                            .icon(IconName::Copy)
                            .tooltip("Copy file contents")
                            .small()
                            .compact()
                            .ghost()
                            .on_click(move |_, _, cx| {
                                app_for_copy.update(cx, |this, cx| this.copy_document(cx));
                            }),
                    )
                    .child(
                        Button::new("document-wrap")
                            .label("Wrap")
                            .tooltip("Toggle line wrapping")
                            .small()
                            .compact()
                            .ghost()
                            .selected(wrap)
                            .on_click(move |_, window, cx| {
                                let next = !app_for_wrap.read(cx).wrap_document;
                                app_for_wrap.update(cx, |this, cx| {
                                    this.wrap_document = next;
                                    cx.notify();
                                });
                                editor_for_wrap.update(cx, |editor, cx| {
                                    editor.set_soft_wrap(next, window, cx);
                                });
                            }),
                    ),
            )
            .child(
                div()
                    .id("code-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .bg(theme.panel_background)
                    .child(code),
            )
            .into_any_element()
    }

    fn search_content(
        &self,
        theme: Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let app = cx.entity();
        let dragging = self.resize_target == Some(ResizeTarget::WorkspaceContext);
        let results = match &self.search_state {
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
                        .min_h(px(34.0))
                        .flex_shrink_0()
                        .flex()
                        .flex_col()
                        .justify_center()
                        .px_2()
                        .cursor_pointer()
                        .hover(move |style| style.bg(theme.surface_hover))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
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
                                        .child(format!("L{line}")),
                                ),
                        )
                        .child(
                            div()
                                .font_family(MONO_FONT)
                                .text_size(px(9.0))
                                .text_color(theme.text_disabled)
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .child(path_str),
                        )
                        .on_click(cx.listener(move |this, _event, window, cx| {
                            this.selected_file = Some(hit_path.clone());
                            this.pending_document_line = Some(line.saturating_sub(1));
                            this.load_file_content(cx);
                            window.focus(&this.focus_handle);
                            cx.notify();
                        }))
                }))
                .into_any_element(),
        };

        let context = div()
            .w(px(self.search_context_width_px))
            .h_full()
            .min_h_0()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .border_r_1()
            .border_color(theme.border)
            .child(
                div()
                    .h(px(30.0))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .px_2()
                    .border_b_1()
                    .border_color(theme.border)
                    .gap_2()
                    .child(
                        Input::new(&self.search_input)
                            .flex_1()
                            .min_w_0()
                            .appearance(false),
                    )
                    .child(
                        Button::new("search-go")
                            .icon(IconName::Search)
                            .tooltip("Run search")
                            .small()
                            .compact()
                            .ghost()
                            .disabled(self.search_query.trim().is_empty())
                            .on_click(move |_, window, cx| {
                                app.update(cx, |this, cx| this.execute_search(window, cx));
                            }),
                    ),
            )
            .child(results);

        let handle = div()
            .id("search-split-handle")
            .w(px(4.0))
            .flex_shrink_0()
            .bg(if dragging { theme.focus } else { theme.border })
            .hover(move |style| style.bg(theme.focus))
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _: &MouseDownEvent, _window, cx| {
                    this.resize_target = Some(ResizeTarget::WorkspaceContext);
                    cx.notify();
                }),
            );

        div()
            .id("search-split-pane")
            .size_full()
            .flex()
            .min_h_0()
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _event: &MouseUpEvent, _window, cx| {
                    if this.resize_target == Some(ResizeTarget::WorkspaceContext) {
                        this.resize_target = None;
                        cx.notify();
                    }
                }),
            )
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _window, cx| {
                if this.resize_target == Some(ResizeTarget::WorkspaceContext) {
                    let raw: f32 = event.position.x.into();
                    this.search_context_width_px = raw.clamp(150.0, 460.0);
                    cx.notify();
                }
            }))
            .when(self.context_pane_visible, |search| {
                search.child(context).child(handle)
            })
            .child(
                div()
                    .min_w_0()
                    .min_h_0()
                    .h_full()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .child(self.file_detail_content(theme, window, cx)),
            )
            .into_any_element()
    }

    fn git_change_row(
        &self,
        selection: GitSelection,
        row_id: (&'static str, usize),
        label: String,
        depth: usize,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let change = &selection.change;
        let kind = selection.kind;
        let is_selected = self.git_selection.as_ref() == Some(&selection);
        let status = match kind {
            GitDiffKind::Unstaged => change.worktree_status,
            GitDiffKind::Staged => change.index_status,
        };
        let status = if status == '?' {
            "U".to_string()
        } else {
            status.to_string()
        };
        let status_color = if change.is_conflicted() {
            theme.error
        } else if change.is_untracked() || change.status_label() == "added" {
            theme.success
        } else if change.status_label() == "deleted" {
            theme.error
        } else {
            theme.warning
        };
        let app = cx.entity();
        let app_for_action = app.clone();
        let action_path = change.path.clone();
        let (action_icon, action_tooltip) = match kind {
            GitDiffKind::Unstaged => (IconName::Plus, "Stage change"),
            GitDiffKind::Staged => (IconName::Minus, "Unstage change"),
        };
        let line_stats = change.line_stats(kind);
        let additions = line_stats.and_then(|stats| stats.additions);
        let deletions = line_stats.and_then(|stats| stats.deletions);
        let operation_running = self.git_operation_running();
        let context_selection = selection.clone();
        let select_selection = selection.clone();

        div()
            .id(row_id)
            .h(px(25.0))
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap_2()
            .pl(px(6.0 + depth as f32 * 13.0))
            .pr_1()
            .cursor_pointer()
            .bg(if is_selected {
                theme.surface_selected
            } else {
                theme.sidebar_background
            })
            .hover(move |style| style.bg(theme.surface_hover))
            .child(
                div()
                    .w(px(12.0))
                    .flex_shrink_0()
                    .font_family(MONO_FONT)
                    .text_size(px(9.0))
                    .text_color(status_color)
                    .child(status),
            )
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .font_family(MONO_FONT)
                    .text_size(px(10.0))
                    .text_color(if is_selected {
                        theme.text
                    } else {
                        theme.text_muted
                    })
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .child(label),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap_1()
                    .font_family(MONO_FONT)
                    .text_size(px(9.0))
                    .child(
                        div()
                            .text_color(theme.success)
                            .child(format!("+{}", git_stat_value(additions))),
                    )
                    .child(
                        div()
                            .text_color(theme.error)
                            .child(format!("-{}", git_stat_value(deletions))),
                    ),
            )
            .child(
                Button::new(("git-row-action", row_id.1))
                    .icon(action_icon)
                    .tooltip(action_tooltip)
                    .xsmall()
                    .compact()
                    .ghost()
                    .disabled(operation_running)
                    .on_click(move |_, _, cx| {
                        let action = match kind {
                            GitDiffKind::Unstaged => GitAction::Stage(vec![action_path.clone()]),
                            GitDiffKind::Staged => GitAction::Unstage(vec![action_path.clone()]),
                        };
                        app_for_action.update(cx, |this, cx| {
                            this.run_git_action(action, cx);
                        });
                    }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, _window, cx| {
                    this.select_git_change(context_selection.clone(), cx);
                    this.context_menu = None;
                    this.git_context_menu = Some((
                        context_selection.clone(),
                        event.position.x.into(),
                        event.position.y.into(),
                    ));
                    cx.notify();
                }),
            )
            .on_click(move |_, _, cx| {
                app.update(cx, |this, cx| {
                    this.select_git_change(select_selection.clone(), cx);
                });
            })
            .into_any_element()
    }

    fn git_change_rows(
        &self,
        changes: &[&GitFileChange],
        kind: GitDiffKind,
        row_prefix: &'static str,
        row_index: &mut usize,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        if !self.git_tree_view {
            return changes
                .iter()
                .map(|change| {
                    let index = *row_index;
                    *row_index += 1;
                    self.git_change_row(
                        GitSelection {
                            change: (*change).clone(),
                            kind,
                        },
                        (row_prefix, index),
                        git_change_label(change),
                        0,
                        theme,
                        cx,
                    )
                })
                .collect();
        }

        let app = cx.entity();
        git_tree_entries(changes, kind)
            .into_iter()
            .filter(|entry| {
                !self
                    .git_collapsed_dirs
                    .iter()
                    .any(|(collapsed_kind, path)| {
                        *collapsed_kind == kind
                            && entry.path != *path
                            && entry.path.starts_with(path)
                    })
            })
            .map(|entry| {
                let index = *row_index;
                *row_index += 1;
                let app_for_entry = app.clone();
                if let Some(selection) = entry.selection {
                    return self.git_change_row(
                        selection,
                        (row_prefix, index),
                        entry.label,
                        entry.depth,
                        theme,
                        cx,
                    );
                }

                let path = entry.path;
                let collapsed = self.git_collapsed_dirs.contains(&(kind, path.clone()));
                let path_for_click = path.clone();
                div()
                    .id(("git-tree-directory", index))
                    .h(px(23.0))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap_1()
                    .pl(px(4.0 + entry.depth as f32 * 13.0))
                    .pr_1()
                    .font_family(MONO_FONT)
                    .text_size(px(10.0))
                    .text_color(theme.text_muted)
                    .cursor_pointer()
                    .hover(move |style| style.bg(theme.surface_hover))
                    .child(
                        Icon::new(if collapsed {
                            IconName::ChevronRight
                        } else {
                            IconName::ChevronDown
                        })
                        .size(px(11.0))
                        .text_color(theme.text_disabled),
                    )
                    .child(
                        Icon::new(if collapsed {
                            IconName::FolderClosed
                        } else {
                            IconName::FolderOpen
                        })
                        .size(px(12.0))
                        .text_color(theme.accent),
                    )
                    .child(entry.label)
                    .on_click(move |_, _, cx| {
                        app_for_entry.update(cx, |this, cx| {
                            let key = (kind, path_for_click.clone());
                            if !this.git_collapsed_dirs.remove(&key) {
                                this.git_collapsed_dirs.insert(key);
                            }
                            cx.notify();
                        });
                    })
                    .into_any_element()
            })
            .collect()
    }

    fn git_diff_panel(
        &self,
        theme: Theme,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(selection) = self.git_selection.clone() else {
            return workbench_message("working tree clean", theme.text_disabled);
        };
        let path = selection.change.path.to_string_lossy().into_owned();
        let app = cx.entity();
        let operation_running = self.git_operation_running();
        let actions = match selection.kind {
            GitDiffKind::Unstaged => {
                let app_for_stage = app.clone();
                let app_for_discard = app.clone();
                let change_for_stage = selection.change.clone();
                let change_for_discard = selection.change.clone();
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(
                        Button::new("git-stage-selected")
                            .icon(IconName::Plus)
                            .tooltip("Stage selected change")
                            .xsmall()
                            .compact()
                            .ghost()
                            .disabled(operation_running)
                            .on_click(move |_, _, cx| {
                                let change = change_for_stage.clone();
                                app_for_stage.update(cx, |this, cx| {
                                    this.run_git_action(GitAction::Stage(vec![change.path]), cx);
                                });
                            }),
                    )
                    .child(
                        Button::new("git-discard-selected")
                            .icon(IconName::Undo2)
                            .tooltip("Discard selected change")
                            .xsmall()
                            .compact()
                            .ghost()
                            .disabled(operation_running)
                            .on_click(move |_, _, cx| {
                                let change = change_for_discard.clone();
                                app_for_discard.update(cx, |this, cx| {
                                    this.git_pending_discard = Some(change);
                                    cx.notify();
                                });
                            }),
                    )
                    .into_any_element()
            }
            GitDiffKind::Staged => {
                let app_for_unstage = app.clone();
                div()
                    .child(
                        Button::new("git-unstage-selected")
                            .icon(IconName::Minus)
                            .tooltip("Unstage selected change")
                            .xsmall()
                            .compact()
                            .ghost()
                            .disabled(operation_running)
                            .on_click(move |_, _, cx| {
                                let change = selection.change.clone();
                                app_for_unstage.update(cx, |this, cx| {
                                    this.run_git_action(GitAction::Unstage(vec![change.path]), cx);
                                });
                            }),
                    )
                    .into_any_element()
            }
        };

        let content = Self::diff_body(&self.git_diff_state, theme);

        div()
            .size_full()
            .min_h_0()
            .flex()
            .flex_col()
            .child(
                div()
                    .h(px(26.0))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap_2()
                    .pl_2()
                    .pr_1()
                    .border_b_1()
                    .border_color(theme.border)
                    .font_family(MONO_FONT)
                    .text_size(px(10.0))
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_color(theme.text_muted)
                            .child(path),
                    )
                    .child(
                        div()
                            .text_color(theme.text_disabled)
                            .child(match selection.kind {
                                GitDiffKind::Unstaged => "Working Tree",
                                GitDiffKind::Staged => "Staged",
                            }),
                    )
                    .child(
                        Button::new("git-copy-diff")
                            .icon(IconName::Copy)
                            .tooltip("Copy diff")
                            .xsmall()
                            .compact()
                            .ghost()
                            .on_click({
                                let app = app.clone();
                                move |_, _, cx| {
                                    app.update(cx, |this, cx| {
                                        if let LoadState::Loaded(diff) = &this.git_diff_state {
                                            cx.write_to_clipboard(ClipboardItem::new_string(
                                                diff.clone(),
                                            ));
                                            this.show_copy_feedback("COPIED DIFF", cx);
                                        }
                                    });
                                }
                            }),
                    )
                    .child(actions),
            )
            .child(div().min_h_0().flex_1().overflow_hidden().child(content))
            .into_any_element()
    }

    fn git_content(&self, theme: Theme, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let app = cx.entity();
        let operation_running = self.git_operation_running();
        let dragging = self.resize_target == Some(ResizeTarget::WorkspaceContext);
        let mut row_index = 0usize;

        let changes: AnyElement = match &self.git_status_state {
            LoadState::Loaded(status) => {
                let conflicts = status
                    .changes
                    .iter()
                    .filter(|change| change.is_conflicted())
                    .collect::<Vec<_>>();
                let unstaged = status
                    .changes
                    .iter()
                    .filter(|change| change.is_unstaged() && !change.is_conflicted())
                    .collect::<Vec<_>>();
                let staged = status
                    .changes
                    .iter()
                    .filter(|change| change.is_staged() && !change.is_conflicted())
                    .collect::<Vec<_>>();
                let branch = status
                    .branch
                    .clone()
                    .unwrap_or_else(|| "detached HEAD".into());
                let tracking = match (status.ahead, status.behind) {
                    (0, 0) => String::new(),
                    (ahead, 0) => format!("{ahead} ahead"),
                    (0, behind) => format!("{behind} behind"),
                    (ahead, behind) => format!("{ahead} ahead · {behind} behind"),
                };
                let (total_additions, total_deletions) = status.line_totals();
                let fetch_enabled = self
                    .selected_project()
                    .is_some_and(|project| project.has_git);
                let set_upstream = status.upstream.is_none();
                let push_enabled =
                    fetch_enabled && branch != "detached" && branch != "detached HEAD";
                let app_for_fetch = app.clone();
                let app_for_push = app.clone();
                let app_for_stage_all = app.clone();
                let app_for_unstage_all = app.clone();
                let app_for_view = app.clone();
                let conflict_rows = self.git_change_rows(
                    &conflicts,
                    GitDiffKind::Unstaged,
                    "git-conflict",
                    &mut row_index,
                    theme,
                    cx,
                );
                let unstaged_rows = self.git_change_rows(
                    &unstaged,
                    GitDiffKind::Unstaged,
                    "git-unstaged",
                    &mut row_index,
                    theme,
                    cx,
                );
                let staged_rows = self.git_change_rows(
                    &staged,
                    GitDiffKind::Staged,
                    "git-staged",
                    &mut row_index,
                    theme,
                    cx,
                );

                let mut list = div()
                    .size_full()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .h(px(30.0))
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .gap_2()
                            .px_2()
                            .border_b_1()
                            .border_color(theme.border)
                            .child(
                                Icon::new(GitBranchIcon)
                                    .size(px(13.0))
                                    .text_color(theme.text_muted),
                            )
                            .child(
                                div()
                                    .min_w_0()
                                    .flex_1()
                                    .font_family(MONO_FONT)
                                    .text_size(px(10.0))
                                    .text_color(theme.text)
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .child(branch),
                            )
                            .when(!tracking.is_empty(), |header| {
                                header.child(
                                    div()
                                        .font_family(MONO_FONT)
                                        .text_size(px(9.0))
                                        .text_color(theme.text_disabled)
                                        .child(tracking),
                                )
                            })
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .font_family(MONO_FONT)
                                    .text_size(px(9.0))
                                    .child(
                                        div()
                                            .text_color(theme.success)
                                            .child(format!("+{total_additions}")),
                                    )
                                    .child(
                                        div()
                                            .text_color(theme.error)
                                            .child(format!("-{total_deletions}")),
                                    ),
                            )
                            .child(
                                Button::new("git-change-view")
                                    .icon(if self.git_tree_view {
                                        IconName::File
                                    } else {
                                        IconName::FolderClosed
                                    })
                                    .tooltip(if self.git_tree_view {
                                        "View changes as flat list"
                                    } else {
                                        "View changes as tree"
                                    })
                                    .xsmall()
                                    .compact()
                                    .ghost()
                                    .on_click(move |_, _, cx| {
                                        app_for_view.update(cx, |this, cx| {
                                            this.git_tree_view = !this.git_tree_view;
                                            cx.notify();
                                        });
                                    }),
                            )
                            .child(
                                Button::new("git-fetch")
                                    .icon(IconName::ArrowDown)
                                    .tooltip("Fetch remotes")
                                    .xsmall()
                                    .compact()
                                    .ghost()
                                    .disabled(operation_running || !fetch_enabled)
                                    .on_click(move |_, _, cx| {
                                        app_for_fetch.update(cx, |this, cx| {
                                            this.run_git_action(GitAction::Fetch, cx);
                                        });
                                    }),
                            )
                            .child(
                                Button::new("git-push")
                                    .icon(IconName::ArrowUp)
                                    .tooltip(if set_upstream {
                                        "Set upstream and push current branch"
                                    } else {
                                        "Push current branch"
                                    })
                                    .xsmall()
                                    .compact()
                                    .ghost()
                                    .disabled(operation_running || !push_enabled)
                                    .on_click(move |_, _, cx| {
                                        app_for_push.update(cx, |this, cx| {
                                            this.run_git_action(
                                                GitAction::Push { set_upstream },
                                                cx,
                                            );
                                        });
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .id("git-changes-scroll")
                            .track_scroll(&self.git_scroll)
                            .min_h_0()
                            .flex_1()
                            .overflow_y_scroll()
                            .when(!conflicts.is_empty(), |scroll| {
                                scroll
                                    .child(
                                        div()
                                            .h(px(23.0))
                                            .flex()
                                            .items_center()
                                            .px_2()
                                            .font_family(MONO_FONT)
                                            .text_size(px(9.0))
                                            .text_color(theme.error)
                                            .child(format!("CONFLICTS  {}", conflicts.len())),
                                    )
                                    .children(conflict_rows)
                            })
                            .when(!unstaged.is_empty(), |scroll| {
                                scroll
                                    .child(
                                        div()
                                            .h(px(23.0))
                                            .flex()
                                            .items_center()
                                            .px_2()
                                            .font_family(MONO_FONT)
                                            .text_size(px(9.0))
                                            .text_color(theme.text_disabled)
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .child(format!("CHANGES  {}", unstaged.len())),
                                            )
                                            .child(
                                                Button::new("git-stage-all")
                                                    .icon(IconName::Plus)
                                                    .tooltip("Stage all changes")
                                                    .xsmall()
                                                    .compact()
                                                    .ghost()
                                                    .disabled(operation_running)
                                                    .on_click(move |_, _, cx| {
                                                        app_for_stage_all.update(cx, |this, cx| {
                                                            this.run_git_action(
                                                                GitAction::StageAll,
                                                                cx,
                                                            );
                                                        });
                                                    }),
                                            ),
                                    )
                                    .children(unstaged_rows)
                            })
                            .when(!staged.is_empty(), |scroll| {
                                scroll
                                    .child(
                                        div()
                                            .h(px(23.0))
                                            .flex()
                                            .items_center()
                                            .px_2()
                                            .font_family(MONO_FONT)
                                            .text_size(px(9.0))
                                            .text_color(theme.text_disabled)
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .child(format!("STAGED  {}", staged.len())),
                                            )
                                            .child(
                                                Button::new("git-unstage-all")
                                                    .icon(IconName::Minus)
                                                    .tooltip("Unstage all changes")
                                                    .xsmall()
                                                    .compact()
                                                    .ghost()
                                                    .disabled(operation_running)
                                                    .on_click(move |_, _, cx| {
                                                        app_for_unstage_all.update(
                                                            cx,
                                                            |this, cx| {
                                                                this.run_git_action(
                                                                    GitAction::UnstageAll,
                                                                    cx,
                                                                );
                                                            },
                                                        );
                                                    }),
                                            ),
                                    )
                                    .children(staged_rows)
                            })
                            .when(status.changes.is_empty(), |scroll| {
                                scroll.child(workbench_message(
                                    "working tree clean",
                                    theme.text_disabled,
                                ))
                            }),
                    );

                let commit_enabled = (status.staged_count() > 0 || self.git_amend)
                    && !self.git_commit_message.trim().is_empty()
                    && !operation_running;
                let app_for_commit = app.clone();
                let app_for_commit_menu = app.clone();
                list = list.child(
                    div()
                        .flex_shrink_0()
                        .p_1()
                        .border_t_1()
                        .border_color(theme.border)
                        .bg(theme.panel_background)
                        .child(
                            div()
                                .relative()
                                .h(px(118.0))
                                .flex()
                                .flex_col()
                                .border_1()
                                .border_color(theme.border_strong)
                                .rounded(px(2.0))
                                .bg(theme.panel_background)
                                .child(
                                    Input::new(&self.git_commit_input)
                                        .min_h_0()
                                        .flex_1()
                                        .w_full()
                                        .appearance(false)
                                        .focus_bordered(false)
                                        .bg(theme.panel_background)
                                        .font_family(MONO_FONT)
                                        .text_size(px(10.0))
                                        .px_1()
                                        .pt_1(),
                                )
                                .child(
                                    div()
                                        .h(px(26.0))
                                        .flex_shrink_0()
                                        .flex()
                                        .items_center()
                                        .justify_end()
                                        .border_t_1()
                                        .border_color(theme.border.opacity(0.6))
                                        .px_1()
                                        .child(
                                            div()
                                                .h(px(20.0))
                                                .flex()
                                                .items_center()
                                                .border_1()
                                                .border_color(theme.border_strong)
                                                .rounded(px(2.0))
                                                .text_size(px(9.0))
                                                .text_color(if commit_enabled {
                                                    theme.text
                                                } else {
                                                    theme.text_disabled
                                                })
                                                .child(
                                                    div()
                                                        .id("git-commit-submit")
                                                        .h_full()
                                                        .flex()
                                                        .items_center()
                                                        .px_1()
                                                        .when(commit_enabled, |button| {
                                                            button
                                                                .cursor_pointer()
                                                                .hover(move |style| {
                                                                    style.bg(theme.surface_hover)
                                                                })
                                                                .on_click(move |_, _, cx| {
                                                                    app_for_commit.update(
                                                                        cx,
                                                                        |this, cx| {
                                                                            this.commit_git(cx);
                                                                        },
                                                                    );
                                                                })
                                                        })
                                                        .child(if self.git_amend {
                                                            "Amend Tracked"
                                                        } else {
                                                            "Commit Tracked"
                                                        }),
                                                )
                                                .child(
                                                    div()
                                                        .id("git-commit-mode")
                                                        .h_full()
                                                        .w(px(19.0))
                                                        .flex()
                                                        .items_center()
                                                        .justify_center()
                                                        .border_l_1()
                                                        .border_color(theme.border_strong)
                                                        .cursor_pointer()
                                                        .hover(move |style| {
                                                            style.bg(theme.surface_hover)
                                                        })
                                                        .child(
                                                            Icon::new(IconName::ChevronDown)
                                                                .size(px(10.0))
                                                                .text_color(theme.text_muted),
                                                        )
                                                        .on_click(move |_, _, cx| {
                                                            app_for_commit_menu.update(
                                                                cx,
                                                                |this, cx| {
                                                                    this.git_commit_menu_open =
                                                                        !this.git_commit_menu_open;
                                                                    cx.notify();
                                                                },
                                                            );
                                                        }),
                                                ),
                                        ),
                                )
                                .when(self.git_commit_menu_open, |composer| {
                                    let app_for_commit_mode = app.clone();
                                    let app_for_amend_mode = app.clone();
                                    composer.child(
                                        div()
                                            .absolute()
                                            .right(px(4.0))
                                            .bottom(px(27.0))
                                            .w(px(154.0))
                                            .border_1()
                                            .border_color(theme.border_strong)
                                            .rounded(px(2.0))
                                            .bg(theme.surface_background)
                                            .shadow_sm()
                                            .occlude()
                                            .child(commit_mode_row(
                                                "commit-mode-tracked",
                                                "Commit tracked",
                                                !self.git_amend,
                                                theme,
                                                move |_, _, cx| {
                                                    app_for_commit_mode.update(cx, |this, cx| {
                                                        this.git_amend = false;
                                                        this.git_commit_menu_open = false;
                                                        cx.notify();
                                                    });
                                                },
                                            ))
                                            .child(commit_mode_row(
                                                "commit-mode-amend",
                                                "Amend HEAD",
                                                self.git_amend,
                                                theme,
                                                move |_, _, cx| {
                                                    app_for_amend_mode.update(cx, |this, cx| {
                                                        this.git_amend = true;
                                                        this.git_commit_menu_open = false;
                                                        cx.notify();
                                                    });
                                                },
                                            )),
                                    )
                                }),
                        ),
                );
                list.into_any_element()
            }
            LoadState::Loading => workbench_message("loading Git status...", theme.text_disabled),
            LoadState::Error(error) => workbench_message(error.clone(), theme.error),
            LoadState::Idle => workbench_message("loading Git status...", theme.text_disabled),
            LoadState::Empty => workbench_message("working tree clean", theme.text_disabled),
        };

        let context = div()
            .w(px(self.git_context_width_px))
            .h_full()
            .min_h_0()
            .flex_shrink_0()
            .bg(theme.sidebar_background)
            .border_r_1()
            .border_color(theme.border)
            .child(changes);
        let handle = div()
            .id("git-split-handle")
            .w(px(4.0))
            .flex_shrink_0()
            .bg(if dragging { theme.focus } else { theme.border })
            .hover(move |style| style.bg(theme.focus))
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _: &MouseDownEvent, _window, cx| {
                    this.resize_target = Some(ResizeTarget::WorkspaceContext);
                    cx.notify();
                }),
            );

        let content = div()
            .relative()
            .size_full()
            .min_h_0()
            .flex()
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _event: &MouseUpEvent, _window, cx| {
                    if this.resize_target == Some(ResizeTarget::WorkspaceContext) {
                        this.resize_target = None;
                        cx.notify();
                    }
                }),
            )
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _window, cx| {
                if this.resize_target == Some(ResizeTarget::WorkspaceContext) {
                    let raw: f32 = event.position.x.into();
                    this.git_context_width_px = raw.clamp(220.0, 520.0);
                    cx.notify();
                }
            }))
            .when(self.context_pane_visible, |workspace| {
                workspace.child(context).child(handle)
            })
            .child(
                div()
                    .min_w_0()
                    .min_h_0()
                    .flex_1()
                    .child(self.git_diff_panel(theme, window, cx)),
            );

        let Some(pending) = self.git_pending_discard.clone() else {
            return content.into_any_element();
        };
        let app_for_cancel = app.clone();
        let app_for_confirm = app.clone();
        let path = pending.path.to_string_lossy().into_owned();
        content
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(theme.app_background.opacity(0.72))
                    .occlude()
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        app_for_cancel.update(cx, |this, cx| {
                            this.git_pending_discard = None;
                            cx.notify();
                        });
                    })
                    .child(
                        div()
                            .w(px(360.0))
                            .max_w_full()
                            .flex()
                            .flex_col()
                            .gap_3()
                            .p_3()
                            .bg(theme.surface_background)
                            .border_1()
                            .border_color(theme.border_strong)
                            .rounded_sm()
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .child(div().text_size(px(13.0)).text_color(theme.text).child(
                                if pending.is_untracked() {
                                    "Delete untracked file?"
                                } else {
                                    "Discard working-tree changes?"
                                },
                            ))
                            .child(
                                div()
                                    .font_family(MONO_FONT)
                                    .text_size(px(10.0))
                                    .text_color(theme.text_muted)
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .child(path),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_end()
                                    .gap_2()
                                    .child(
                                        Button::new("git-discard-cancel")
                                            .label("Cancel")
                                            .small()
                                            .compact()
                                            .on_click(move |_, _, cx| {
                                                app.update(cx, |this, cx| {
                                                    this.git_pending_discard = None;
                                                    cx.notify();
                                                });
                                            }),
                                    )
                                    .child(
                                        Button::new("git-discard-confirm")
                                            .label("Discard")
                                            .small()
                                            .compact()
                                            .danger()
                                            .on_click(move |_, _, cx| {
                                                let change = pending.clone();
                                                app_for_confirm.update(cx, |this, cx| {
                                                    this.run_git_action(
                                                        GitAction::Discard(change),
                                                        cx,
                                                    );
                                                });
                                            }),
                                    ),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn commit_history_content(
        &self,
        theme: Theme,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if matches!(self.history_state, LoadState::Idle) {
            return workbench_message("select a project to view history", theme.text_disabled);
        }
        if matches!(self.history_state, LoadState::Loading) && self.history_commits.is_empty() {
            return workbench_message("loading commit history...", theme.text_disabled);
        }
        if let LoadState::Error(ref error) = self.history_state {
            return workbench_message(error.clone(), theme.error);
        }
        if self.history_commits.is_empty() {
            return workbench_message("no commits", theme.text_muted);
        }

        let main = if self.history_selected_file.is_some() {
            self.history_commit_diff_content(theme, cx)
        } else {
            self.history_table(theme, cx)
        };
        let selected = self
            .history_selected
            .and_then(|index| self.history_commits.get(index));

        div()
            .size_full()
            .min_h_0()
            .flex()
            .bg(theme.panel_background)
            .child(div().min_w_0().min_h_0().flex_1().child(main))
            .when_some(selected, |layout, commit| {
                layout.child(
                    div()
                        .w(px(292.0))
                        .min_w(px(250.0))
                        .max_w(px(340.0))
                        .h_full()
                        .flex_shrink_0()
                        .border_l_1()
                        .border_color(theme.border)
                        .child(self.commit_detail_panel(commit, theme, cx)),
                )
            })
            .into_any_element()
    }

    fn diff_body(state: &LoadState<String>, theme: Theme) -> AnyElement {
        match state {
            LoadState::Loaded(diff) => {
                let (lines, truncated) = parse_unified_diff(diff, MAX_RENDERED_DIFF_LINES);
                div()
                    .id("git-diff-scroll")
                    .size_full()
                    .overflow_scroll()
                    .bg(theme.panel_background)
                    .font_family(MONO_FONT)
                    .text_size(px(10.0))
                    .children(lines.into_iter().enumerate().filter_map(|(index, line)| {
                        let text = diff_line_text(&line)?;
                        let (background, foreground) = match line.kind {
                            DiffLineKind::Addition => (theme.success.opacity(0.14), theme.success),
                            DiffLineKind::Deletion => (theme.error.opacity(0.14), theme.error),
                            DiffLineKind::HunkHeader => {
                                (theme.accent.opacity(0.08), theme.text_disabled)
                            }
                            DiffLineKind::Context => (theme.panel_background, theme.text_muted),
                            DiffLineKind::Metadata => {
                                (theme.surface_background, theme.text_disabled)
                            }
                            DiffLineKind::FileHeader => return None,
                        };
                        Some(
                            div()
                                .id(("git-diff-line", index))
                                .min_h(px(19.0))
                                .w_full()
                                .flex()
                                .items_center()
                                .bg(background)
                                .text_color(foreground)
                                .child(diff_gutter_number(line.old_line, theme))
                                .child(diff_gutter_number(line.new_line, theme))
                                .child(
                                    div()
                                        .flex_shrink_0()
                                        .pl_2()
                                        .pr_4()
                                        .whitespace_nowrap()
                                        .child(text),
                                )
                                .into_any_element(),
                        )
                    }))
                    .when(truncated, |diff| {
                        diff.child(
                            div()
                                .h(px(24.0))
                                .flex()
                                .items_center()
                                .px_2()
                                .bg(theme.warning.opacity(0.10))
                                .text_color(theme.warning)
                                .child("Diff truncated at 5,000 lines."),
                        )
                    })
                    .into_any_element()
            }
            LoadState::Loading => workbench_message("loading diff...", theme.text_disabled),
            LoadState::Empty => workbench_message("no textual diff", theme.text_muted),
            LoadState::Error(error) => workbench_message(error.clone(), theme.error),
            LoadState::Idle => workbench_message("select a change", theme.text_disabled),
        }
    }

    fn history_table(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let graph_rows = history_graph_rows(&self.history_commits);
        let commit_count = self.history_commits.len();
        let app = cx.entity();
        let rows = uniform_list(
            "history-rows",
            commit_count,
            cx.processor(
                move |this, visible_range: std::ops::Range<usize>, _window, cx| {
                    if visible_range.end.saturating_add(5) >= this.history_commits.len()
                        && this.history_has_more
                        && this.history_more_cancellation.is_none()
                    {
                        let app = cx.entity();
                        cx.defer(move |cx| {
                            app.update(cx, |this, cx| this.load_more_history(cx));
                        });
                    }

                    visible_range
                        .filter_map(|index| {
                            let commit = this.history_commits.get(index)?.clone();
                            let graph = graph_rows.get(index).cloned().unwrap_or_default();
                            let is_selected = this.history_selected == Some(index);
                            let app = app.clone();
                            Some(history_commit_row(
                                index,
                                commit,
                                graph,
                                is_selected,
                                theme,
                                move |_, _, cx| {
                                    app.update(cx, |this, cx| {
                                        this.select_history_commit(index, cx);
                                    });
                                },
                            ))
                        })
                        .collect()
                },
            ),
        )
        .size_full()
        .track_scroll(self.history_scroll.clone());

        div()
            .size_full()
            .min_h_0()
            .flex()
            .flex_col()
            .child(history_table_header(theme))
            .child(div().min_h_0().flex_1().child(rows))
            .when_some(self.history_more_error.as_ref(), |table, error| {
                table.child(
                    div()
                        .h(px(22.0))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .px_2()
                        .border_t_1()
                        .border_color(theme.border)
                        .font_family(MONO_FONT)
                        .text_size(px(9.0))
                        .text_color(theme.error)
                        .child(error.clone()),
                )
            })
            .into_any_element()
    }

    fn history_commit_diff_content(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let path = self.history_selected_file.clone().unwrap_or_default();
        let commit_hash = self
            .history_selected
            .and_then(|index| self.history_commits.get(index))
            .map(|commit| commit.hash.chars().take(7).collect::<String>())
            .unwrap_or_default();
        let app = cx.entity();
        let app_for_copy = app.clone();

        div()
            .size_full()
            .min_h_0()
            .flex()
            .flex_col()
            .child(
                div()
                    .h(px(28.0))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap_1()
                    .px_1()
                    .border_b_1()
                    .border_color(theme.border)
                    .child(
                        Button::new("history-diff-back")
                            .icon(IconName::ArrowLeft)
                            .tooltip("Back to commit history")
                            .xsmall()
                            .compact()
                            .ghost()
                            .on_click(move |_, _, cx| {
                                app.update(cx, |this, cx| this.close_history_file_diff(cx));
                            }),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .font_family(MONO_FONT)
                            .text_size(px(9.0))
                            .text_color(theme.text_muted)
                            .child(path),
                    )
                    .child(
                        div()
                            .font_family(MONO_FONT)
                            .text_size(px(9.0))
                            .text_color(theme.text_disabled)
                            .child(commit_hash),
                    )
                    .child(
                        Button::new("history-copy-diff")
                            .icon(IconName::Copy)
                            .tooltip("Copy patch")
                            .xsmall()
                            .compact()
                            .ghost()
                            .on_click(move |_, _, cx| {
                                app_for_copy.update(cx, |this, cx| {
                                    if let LoadState::Loaded(diff) = &this.history_diff_state {
                                        cx.write_to_clipboard(ClipboardItem::new_string(
                                            diff.clone(),
                                        ));
                                        this.show_copy_feedback("COPIED PATCH", cx);
                                    }
                                });
                            }),
                    ),
            )
            .child(
                div()
                    .id("history-diff-body")
                    .min_h_0()
                    .flex_1()
                    .child(Self::diff_body(&self.history_diff_state, theme)),
            )
            .into_any_element()
    }

    fn commit_detail_panel(
        &self,
        commit: &CommitEntry,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let hash_long = commit.hash.clone();
        let initials = author_initials(&commit.author);
        let (additions, deletions, binary_files) = commit_totals(commit);
        let github_url = self.selected_project().and_then(|project| {
            project
                .git_remote
                .as_ref()
                .and_then(|remote| git_remote_to_github_url(remote, &hash_long))
        });
        let tree = history_file_tree_entries(&commit.files);
        let selected_file = self.history_selected_file.as_deref();
        let app = cx.entity();
        let app_for_close = app.clone();
        let app_for_copy = app.clone();

        div()
            .id("commit-detail")
            .size_full()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(theme.sidebar_background)
            .child(
                div()
                    .h(px(26.0))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_end()
                    .px_1()
                    .child(
                        Button::new("close-commit-detail")
                            .icon(IconName::Close)
                            .tooltip("Close commit details")
                            .xsmall()
                            .compact()
                            .ghost()
                            .on_click(move |_, _, cx| {
                                app_for_close.update(cx, |this, cx| this.close_history_sidebar(cx));
                            }),
                    ),
            )
            .child(
                div()
                    .id("commit-detail-scroll")
                    .min_h_0()
                    .flex_1()
                    .overflow_y_scroll()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap_1()
                            .px_2()
                            .pb_2()
                            .child(
                                div()
                                    .size(px(34.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(17.0))
                                    .bg(theme.surface_selected)
                                    .font_family(MONO_FONT)
                                    .text_size(px(11.0))
                                    .text_color(theme.text)
                                    .child(initials),
                            )
                            .child(
                                div()
                                    .font_family(MONO_FONT)
                                    .text_size(px(11.0))
                                    .text_color(theme.text)
                                    .child(commit.author.clone()),
                            )
                            .child(
                                div()
                                    .font_family(MONO_FONT)
                                    .text_size(px(8.0))
                                    .text_color(theme.text_muted)
                                    .child(commit.date.clone()),
                            )
                            .child(
                                div().flex().flex_wrap().justify_center().gap_1().children(
                                    commit.refs.iter().map(|reference| {
                                        history_ref_badge(reference.clone(), theme)
                                    }),
                                ),
                            ),
                    )
                    .child(
                        div()
                            .mx_2()
                            .my_1()
                            .border_1()
                            .border_color(theme.border)
                            .rounded(px(2.0))
                            .bg(theme.surface_background)
                            .child(
                                div()
                                    .min_h(px(24.0))
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .px_1()
                                    .border_b_1()
                                    .border_color(theme.border.opacity(0.5))
                                    .child(
                                        Icon::new(IconName::Inbox)
                                            .size(px(11.0))
                                            .text_color(theme.text_disabled),
                                    )
                                    .child(
                                        div()
                                            .min_w_0()
                                            .flex_1()
                                            .whitespace_nowrap()
                                            .overflow_hidden()
                                            .font_family(MONO_FONT)
                                            .text_size(px(8.0))
                                            .text_color(theme.text_muted)
                                            .child(commit.author_email.clone()),
                                    ),
                            )
                            .child(
                                div()
                                    .min_h(px(24.0))
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .px_1()
                                    .child(
                                        div()
                                            .min_w_0()
                                            .flex_1()
                                            .whitespace_nowrap()
                                            .overflow_hidden()
                                            .font_family(MONO_FONT)
                                            .text_size(px(8.0))
                                            .text_color(theme.text_muted)
                                            .child(hash_long.clone()),
                                    )
                                    .child(
                                        Button::new("copy-commit-hash")
                                            .icon(IconName::Copy)
                                            .tooltip("Copy commit hash")
                                            .xsmall()
                                            .compact()
                                            .ghost()
                                            .on_click(move |_, _, cx| {
                                                let hash = hash_long.clone();
                                                app_for_copy.update(cx, |this, cx| {
                                                    cx.write_to_clipboard(
                                                        ClipboardItem::new_string(hash),
                                                    );
                                                    this.show_copy_feedback("COPIED HASH", cx);
                                                });
                                            }),
                                    ),
                            )
                            .when_some(github_url, |card, url| {
                                let app = app.clone();
                                card.child(
                                    Button::new("view-commit-github")
                                        .icon(IconName::GitHub)
                                        .label("View on GitHub")
                                        .xsmall()
                                        .compact()
                                        .ghost()
                                        .on_click(move |_, _, cx| {
                                            let command = if cfg!(target_os = "windows") {
                                                "explorer"
                                            } else if cfg!(target_os = "macos") {
                                                "open"
                                            } else {
                                                "xdg-open"
                                            };
                                            if let Err(error) = std::process::Command::new(command)
                                                .arg(&url)
                                                .spawn()
                                            {
                                                app.update(cx, |this, cx| {
                                                    this.history_open_error =
                                                        Some(error.to_string());
                                                    cx.notify();
                                                });
                                            }
                                        }),
                                )
                            }),
                    )
                    .child(
                        div()
                            .mt_2()
                            .px_2()
                            .py_1()
                            .border_t_1()
                            .border_b_1()
                            .border_color(theme.border)
                            .flex()
                            .items_center()
                            .child(
                                div()
                                    .flex_1()
                                    .font_family(MONO_FONT)
                                    .text_size(px(9.0))
                                    .text_color(theme.text_muted)
                                    .child(format!("{} changed files", commit.files.len())),
                            )
                            .child(
                                div()
                                    .font_family(MONO_FONT)
                                    .text_size(px(9.0))
                                    .text_color(theme.success)
                                    .child(format!("+{additions}")),
                            )
                            .child(
                                div()
                                    .ml_1()
                                    .font_family(MONO_FONT)
                                    .text_size(px(9.0))
                                    .text_color(theme.error)
                                    .child(format!("-{deletions}")),
                            )
                            .when(binary_files > 0, |header| {
                                header.child(
                                    div()
                                        .ml_1()
                                        .font_family(MONO_FONT)
                                        .text_size(px(8.0))
                                        .text_color(theme.text_disabled)
                                        .child(format!("{binary_files} binary")),
                                )
                            }),
                    )
                    .child(div().py_1().children(tree.into_iter().map(|entry| {
                        let app = app.clone();
                        let tree_guides = history_tree_guides(&entry, theme);
                        match entry.file_index {
                            Some(file_index) => {
                                let file = commit.files[file_index].clone();
                                let hash = commit.hash.clone();
                                let is_selected = selected_file == Some(file.path.as_str());
                                div()
                                    .id(("commit-file", file_index))
                                    .h(px(22.0))
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .pl_1()
                                    .pr_1()
                                    .cursor_pointer()
                                    .bg(if is_selected {
                                        theme.surface_selected
                                    } else {
                                        theme.sidebar_background
                                    })
                                    .hover(move |style| style.bg(theme.surface_hover))
                                    .child(tree_guides)
                                    .child(
                                        Icon::new(IconName::File)
                                            .size(px(11.0))
                                            .text_color(theme.accent),
                                    )
                                    .child(
                                        div()
                                            .min_w_0()
                                            .flex_1()
                                            .whitespace_nowrap()
                                            .overflow_hidden()
                                            .font_family(MONO_FONT)
                                            .text_size(px(9.0))
                                            .text_color(theme.text_muted)
                                            .child(entry.label),
                                    )
                                    .on_click(move |_, _, cx| {
                                        app.update(cx, |this, cx| {
                                            this.load_history_file_diff(
                                                hash.clone(),
                                                file.path.clone(),
                                                cx,
                                            );
                                        });
                                    })
                                    .into_any_element()
                            }
                            None => div()
                                .h(px(22.0))
                                .flex()
                                .items_center()
                                .gap_1()
                                .pl_1()
                                .pr_1()
                                .child(tree_guides)
                                .child(
                                    Icon::new(IconName::FolderOpen)
                                        .size(px(11.0))
                                        .text_color(theme.text_disabled),
                                )
                                .child(
                                    div()
                                        .min_w_0()
                                        .flex_1()
                                        .whitespace_nowrap()
                                        .overflow_hidden()
                                        .font_family(MONO_FONT)
                                        .text_size(px(9.0))
                                        .text_color(theme.text_disabled)
                                        .child(entry.label),
                                )
                                .into_any_element(),
                        }
                    })))
                    .when_some(self.history_open_error.as_ref(), |content, error| {
                        content.child(
                            div()
                                .px_2()
                                .py_1()
                                .font_family(MONO_FONT)
                                .text_size(px(8.0))
                                .text_color(theme.error)
                                .child(error.clone()),
                        )
                    }),
            )
            .into_any_element()
    }

    fn project_catalog_panel(
        &self,
        theme: Theme,
        filter_active: bool,
        visible_count: usize,
        total_count: usize,
        project_list: AnyElement,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let app = cx.entity();
        let app_for_close = app.clone();
        let dragging = self.resize_target == Some(ResizeTarget::ProjectCatalog);

        div()
            .absolute()
            .top(px(TITLEBAR_HEIGHT))
            .bottom(px(STATUSBAR_HEIGHT))
            .left_0()
            .right_0()
            .flex()
            .occlude()
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _event: &MouseUpEvent, _window, cx| {
                    if this.resize_target == Some(ResizeTarget::ProjectCatalog) {
                        this.resize_target = None;
                        cx.notify();
                    }
                }),
            )
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _window, cx| {
                if this.resize_target == Some(ResizeTarget::ProjectCatalog) {
                    let raw: f32 = event.position.x.into();
                    this.project_catalog_width_px = raw.clamp(180.0, 520.0);
                    cx.notify();
                }
            }))
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                app.update(cx, |this, cx| {
                    this.project_catalog_open = false;
                    this.resize_target = None;
                    window.focus(&this.focus_handle);
                    cx.notify();
                });
            })
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .h_full()
                    .w(px(self.project_catalog_width_px))
                    .flex()
                    .flex_col()
                    .border_r_1()
                    .border_color(theme.border_strong)
                    .bg(theme.sidebar_background)
                    .occlude()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .h(px(32.0))
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .px_1()
                            .gap_1()
                            .border_b_1()
                            .border_color(theme.border)
                            .key_context("ProjectFilter")
                            .child(
                                Input::new(&self.filter_input)
                                    .min_w_0()
                                    .flex_1()
                                    .appearance(false),
                            )
                            .child(
                                div()
                                    .font_family(MONO_FONT)
                                    .text_size(px(9.0))
                                    .text_color(theme.text_disabled)
                                    .child(if filter_active {
                                        format!("{visible_count}/{total_count}")
                                    } else {
                                        total_count.to_string()
                                    }),
                            )
                            .child(
                                Button::new("close-project-catalog")
                                    .icon(IconName::Close)
                                    .tooltip("Close project catalog")
                                    .xsmall()
                                    .compact()
                                    .ghost()
                                    .on_click(move |_, window, cx| {
                                        app_for_close.update(cx, |this, cx| {
                                            this.project_catalog_open = false;
                                            this.resize_target = None;
                                            window.focus(&this.focus_handle);
                                            cx.notify();
                                        });
                                    }),
                            ),
                    )
                    .child(project_list),
            )
            .child(
                div()
                    .id("project-catalog-resize-handle")
                    .h_full()
                    .w(px(4.0))
                    .flex_shrink_0()
                    .bg(if dragging { theme.focus } else { theme.border })
                    .hover(move |style| style.bg(theme.focus))
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _: &MouseDownEvent, _window, cx| {
                            this.resize_target = Some(ResizeTarget::ProjectCatalog);
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ),
            )
            .when(dragging, |catalog| {
                catalog.child(
                    div()
                        .absolute()
                        .inset_0()
                        .cursor_pointer()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _window, cx| {
                            let raw: f32 = event.position.x.into();
                            this.project_catalog_width_px = raw.clamp(180.0, 520.0);
                            cx.notify();
                        }))
                        .on_mouse_up(
                            MouseButton::Left,
                            cx.listener(|this, _event: &MouseUpEvent, _window, cx| {
                                this.resize_target = None;
                                cx.notify();
                            }),
                        ),
                )
            })
            .into_any_element()
    }

    fn workspace_navigation(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let app = cx.entity();
        let button_style = |active: bool| {
            ButtonCustomVariant::new(cx)
                .color(if active {
                    theme.surface_selected
                } else {
                    theme.titlebar_background
                })
                .foreground(if active {
                    theme.accent
                } else {
                    theme.text_disabled
                })
                .hover(theme.surface_hover)
                .active(theme.surface_selected)
        };

        let app_for_catalog = app.clone();
        let mut navigation = div().h_full().flex().items_center().gap_1().child(
            Button::new("project-catalog")
                .icon(IconName::LayoutDashboard)
                .tooltip("Projects (Ctrl+1)")
                .xsmall()
                .compact()
                .custom(button_style(self.project_catalog_open))
                .w(px(22.0))
                .h(px(20.0))
                .on_click(move |_, window, cx| {
                    app_for_catalog.update(cx, |this, cx| {
                        this.toggle_project_catalog_open(window, cx);
                    });
                }),
        );

        for (index, activity) in Activity::ALL.into_iter().enumerate() {
            let icon = match activity {
                Activity::Overview => Icon::new(IconName::BookOpen),
                Activity::Files => Icon::new(IconName::FolderOpen),
                Activity::Search => Icon::new(IconName::Search),
                Activity::Git => Icon::new(GitBranchIcon),
                Activity::History => Icon::new(HistoryIcon),
            };
            let app = app.clone();
            navigation = navigation.child(
                Button::new(("workspace", index))
                    .icon(icon)
                    .tooltip(activity.label())
                    .xsmall()
                    .compact()
                    .custom(button_style(
                        self.activity == activity && !self.show_settings,
                    ))
                    .w(px(22.0))
                    .h(px(20.0))
                    .on_click(move |_, window, cx| {
                        app.update(cx, |this, cx| {
                            this.set_activity(activity, window, cx);
                        });
                    }),
            );
        }

        let app = cx.entity();
        navigation = navigation.child(
            Button::new("bottom-command-palette")
                .icon(IconName::SquareTerminal)
                .tooltip("Command palette (Ctrl+Shift+P)")
                .xsmall()
                .compact()
                .custom(button_style(matches!(
                    self.launcher,
                    Some(LauncherMode::Commands | LauncherMode::Themes)
                )))
                .w(px(22.0))
                .h(px(20.0))
                .on_click(move |_, window, cx| {
                    app.update(cx, |this, cx| {
                        this.open_launcher(LauncherMode::Commands, window, cx);
                    });
                }),
        );

        navigation.into_any_element()
    }

    fn launcher_panel(
        &self,
        theme: Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(mode) = self.launcher else {
            return div().into_any_element();
        };
        let app = cx.entity();
        let title = match mode {
            LauncherMode::Branches => "Branches",
            LauncherMode::Commands => "Commands",
            LauncherMode::Editors => "Open in",
            LauncherMode::Projects => "Projects",
            LauncherMode::Themes => "Themes",
        };
        let panel_width = match mode {
            LauncherMode::Branches => 280.0,
            LauncherMode::Editors => 300.0,
            LauncherMode::Commands | LauncherMode::Projects | LauncherMode::Themes => 350.0,
        };
        let window_width: f32 = window.bounds().size.width.into();
        let panel_left = ((window_width - panel_width) / 2.0).max(4.0);
        let app_for_close = app.clone();
        let app_for_backdrop = app.clone();

        let (empty_message, empty_color) = match mode {
            LauncherMode::Branches => match &self.git_branches_state {
                LoadState::Loading | LoadState::Idle => {
                    ("loading branches...".to_string(), theme.text_disabled)
                }
                LoadState::Error(error) => (error.clone(), theme.error),
                LoadState::Empty => ("no local branches".to_string(), theme.text_muted),
                LoadState::Loaded(_) => ("no matches".to_string(), theme.text_muted),
            },
            LauncherMode::Editors if self.launcher_query.trim().is_empty() => {
                let message = if self
                    .selected_project()
                    .is_some_and(|project| project.source.is_remote())
                {
                    "no detected editor supports SSH projects"
                } else {
                    "no compatible editors detected"
                };
                (message.to_string(), theme.text_muted)
            }
            _ => ("no matches".to_string(), theme.text_muted),
        };

        let rows = match mode {
            LauncherMode::Branches => self
                .launcher_branches()
                .into_iter()
                .enumerate()
                .map(|(index, branch)| {
                    let app = app.clone();
                    div()
                        .id(("branch", index))
                        .h(px(26.0))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .gap_2()
                        .px_2()
                        .border_b_1()
                        .border_color(theme.border.opacity(0.35))
                        .cursor_pointer()
                        .bg(if self.launcher_selected == index {
                            theme.surface_selected
                        } else {
                            theme.surface_background
                        })
                        .hover(move |style| style.bg(theme.surface_hover))
                        .child(if branch.current {
                            Icon::new(IconName::Check)
                                .size(px(12.0))
                                .text_color(theme.success)
                                .into_any_element()
                        } else {
                            Icon::new(GitBranchIcon)
                                .size(px(12.0))
                                .text_color(theme.text_disabled)
                                .into_any_element()
                        })
                        .child(
                            div()
                                .min_w_0()
                                .flex_1()
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .font_family(MONO_FONT)
                                .text_size(px(11.0))
                                .text_color(theme.text)
                                .child(branch.name),
                        )
                        .on_click(move |_, window, cx| {
                            app.update(cx, |this, cx| {
                                this.launcher_selected = index;
                                this.accept_launcher_selection(window, cx);
                            });
                        })
                        .into_any_element()
                })
                .collect::<Vec<_>>(),
            LauncherMode::Commands => self
                .launcher_commands()
                .into_iter()
                .enumerate()
                .map(|(index, command)| {
                    let app = app.clone();
                    div()
                        .id(("command", index))
                        .h(px(26.0))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .gap_2()
                        .px_2()
                        .border_b_1()
                        .border_color(theme.border.opacity(0.35))
                        .cursor_pointer()
                        .bg(if self.launcher_selected == index {
                            theme.surface_selected
                        } else {
                            theme.surface_background
                        })
                        .hover(move |style| style.bg(theme.surface_hover))
                        .child(
                            div()
                                .min_w_0()
                                .flex_1()
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .text_size(px(12.0))
                                .text_color(theme.text)
                                .child(command.title),
                        )
                        .child(
                            div()
                                .font_family(MONO_FONT)
                                .text_size(px(9.0))
                                .text_color(theme.text_disabled)
                                .child(command.category),
                        )
                        .when_some(command.shortcut, |row, shortcut| {
                            row.child(
                                div()
                                    .font_family(MONO_FONT)
                                    .text_size(px(9.0))
                                    .text_color(theme.text_muted)
                                    .child(shortcut),
                            )
                        })
                        .on_click(move |_, window, cx| {
                            app.update(cx, |this, cx| {
                                this.launcher_selected = index;
                                this.accept_launcher_selection(window, cx);
                            });
                        })
                        .into_any_element()
                })
                .collect::<Vec<_>>(),
            LauncherMode::Editors => self
                .launcher_editors()
                .into_iter()
                .enumerate()
                .map(|(index, editor)| {
                    let app = app.clone();
                    div()
                        .id(("editor", index))
                        .h(px(26.0))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .px_2()
                        .border_b_1()
                        .border_color(theme.border.opacity(0.35))
                        .cursor_pointer()
                        .bg(if self.launcher_selected == index {
                            theme.surface_selected
                        } else {
                            theme.surface_background
                        })
                        .hover(move |style| style.bg(theme.surface_hover))
                        .child(
                            div()
                                .min_w_0()
                                .flex_1()
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .text_size(px(11.0))
                                .text_color(theme.text)
                                .child(editor.label().to_string()),
                        )
                        .on_click(move |_, window, cx| {
                            app.update(cx, |this, cx| {
                                this.launcher_selected = index;
                                this.accept_launcher_selection(window, cx);
                            });
                        })
                        .into_any_element()
                })
                .collect::<Vec<_>>(),
            LauncherMode::Projects => self
                .launcher_project_indices()
                .into_iter()
                .enumerate()
                .filter_map(|(index, project_index)| {
                    let project = self.scan.projects.get(project_index)?;
                    let app = app.clone();
                    Some(
                        div()
                            .id(("project-switch", index))
                            .h(px(26.0))
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .gap_2()
                            .px_2()
                            .border_b_1()
                            .border_color(theme.border.opacity(0.35))
                            .cursor_pointer()
                            .bg(if self.launcher_selected == index {
                                theme.surface_selected
                            } else {
                                theme.surface_background
                            })
                            .hover(move |style| style.bg(theme.surface_hover))
                            .child(
                                Icon::new(IconName::Folder)
                                    .size(px(13.0))
                                    .text_color(theme.accent),
                            )
                            .child(
                                div()
                                    .min_w_0()
                                    .max_w(px(132.0))
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .text_size(px(11.0))
                                    .text_color(theme.text)
                                    .child(project.name.clone()),
                            )
                            .child(
                                div()
                                    .min_w_0()
                                    .flex_1()
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .font_family(MONO_FONT)
                                    .text_size(px(8.0))
                                    .text_color(theme.text_disabled)
                                    .child(project.path.to_string_lossy().into_owned()),
                            )
                            .on_click(move |_, window, cx| {
                                app.update(cx, |this, cx| {
                                    this.launcher_selected = index;
                                    this.accept_launcher_selection(window, cx);
                                });
                            })
                            .into_any_element(),
                    )
                })
                .collect::<Vec<_>>(),
            LauncherMode::Themes => filtered_themes(&self.launcher_query)
                .into_iter()
                .enumerate()
                .map(|(index, selection)| {
                    let app = app.clone();
                    div()
                        .id(("theme", index))
                        .h(px(24.0))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .gap_2()
                        .px_2()
                        .border_b_1()
                        .border_color(theme.border.opacity(0.35))
                        .cursor_pointer()
                        .bg(if self.launcher_selected == index {
                            theme.surface_selected
                        } else {
                            theme.surface_background
                        })
                        .hover(move |style| style.bg(theme.surface_hover))
                        .child(
                            Icon::new(
                                if selection.is_active(self.config.theme, self.config.appearance) {
                                    IconName::Check
                                } else {
                                    IconName::Palette
                                },
                            )
                            .size(px(12.0))
                            .text_color(
                                if self.launcher_selected == index {
                                    theme.accent
                                } else {
                                    theme.text_disabled
                                },
                            ),
                        )
                        .child(
                            div()
                                .min_w_0()
                                .flex_1()
                                .text_size(px(12.0))
                                .text_color(theme.text)
                                .child(selection.label()),
                        )
                        .on_click(move |_, window, cx| {
                            app.update(cx, |this, cx| {
                                this.launcher_selected = index;
                                (this.pending_theme, this.pending_appearance) =
                                    selection.preferences(this.config.theme);
                                this.accept_launcher_selection(window, cx);
                            });
                        })
                        .into_any_element()
                })
                .collect::<Vec<_>>(),
        };

        div()
            .absolute()
            .inset_0()
            .occlude()
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                        app_for_backdrop.update(cx, |this, cx| {
                            this.close_launcher(window, cx);
                        });
                    })
                    .on_scroll_wheel(|_, _, cx| cx.stop_propagation()),
            )
            .child(
                div()
                    .absolute()
                    .key_context("DevHubLauncher")
                    .top(px(TITLEBAR_HEIGHT + 4.0))
                    .left(px(panel_left))
                    .w(px(panel_width))
                    .max_h(px(310.0))
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .border_1()
                    .border_color(theme.border_strong)
                    .bg(theme.surface_background)
                    .occlude()
                    .children(vec![
                        div()
                            .h(px(30.0))
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .gap_1()
                            .px_1()
                            .border_b_1()
                            .border_color(theme.border)
                            .child(
                                Input::new(&self.launcher_input)
                                    .min_w_0()
                                    .flex_1()
                                    .appearance(false),
                            )
                            .child(
                                div()
                                    .font_family(MONO_FONT)
                                    .text_size(px(9.0))
                                    .text_color(theme.text_disabled)
                                    .child(title),
                            )
                            .child(
                                Button::new("close-launcher")
                                    .icon(IconName::Close)
                                    .tooltip("Close")
                                    .xsmall()
                                    .compact()
                                    .ghost()
                                    .on_click(move |_, window, cx| {
                                        app_for_close.update(cx, |this, cx| {
                                            this.close_launcher(window, cx);
                                        });
                                    }),
                            )
                            .into_any_element(),
                        div()
                            .id("launcher-list")
                            .min_h_0()
                            .flex_1()
                            .overflow_y_scroll()
                            .track_scroll(&self.launcher_scroll)
                            .when(rows.is_empty(), |list| {
                                list.child(workbench_message(empty_message, empty_color))
                            })
                            .children(rows)
                            .into_any_element(),
                    ]),
            )
            .into_any_element()
    }

    fn terminal_panel(&self, theme: Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.terminal_visible {
            return None;
        }
        let dragging = self.resize_target == Some(ResizeTarget::Terminal);
        let entity = self.terminal_entity.as_ref()?;
        let shell = entity.read(cx).shell.clone();
        let cwd = entity.read(cx).cwd_label.clone();

        Some(
            div()
                .h(px(self.terminal_height_px))
                .flex_shrink_0()
                .flex()
                .flex_col()
                .child(
                    div()
                        .id("terminal-resize-handle")
                        .h(px(4.0))
                        .w_full()
                        .flex_shrink_0()
                        .bg(if dragging { theme.focus } else { theme.border })
                        .hover(move |style| style.bg(theme.focus))
                        .cursor(CursorStyle::ResizeUpDown)
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _: &MouseDownEvent, _window, cx| {
                                this.resize_target = Some(ResizeTarget::Terminal);
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        ),
                )
                .child({
                    let shell_clone = shell.clone();
                    let cwd_clone = cwd.clone();
                    div()
                        .h(px(24.0))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .px_2()
                        .gap_2()
                        .border_t_1()
                        .border_color(theme.border)
                        .bg(theme.titlebar_background)
                        .child(
                            div()
                                .font_family(TERMINAL_FONT)
                                .text_size(px(10.0))
                                .text_color(theme.text_muted)
                                .child(shell_clone),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .font_family(TERMINAL_FONT)
                                .text_size(px(10.0))
                                .text_color(theme.text_muted)
                                .child(cwd_clone),
                        )
                        .child({
                            let app = cx.entity();
                            Button::new("collapse-terminal")
                                .icon(IconName::ChevronDown)
                                .tooltip("Collapse terminal")
                                .xsmall()
                                .compact()
                                .ghost()
                                .on_click(move |_, window, cx| {
                                    app.update(cx, |this, cx| {
                                        this.collapse_terminal(window, cx);
                                    });
                                })
                        })
                })
                .child(
                    div()
                        .flex_1()
                        .min_h_0()
                        .w_full()
                        .flex()
                        .flex_col()
                        .bg(theme.sidebar_background)
                        .child(entity.clone()),
                )
                .into_any_element(),
        )
    }
}

impl Focusable for DevHubLite {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for DevHubLite {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let previewing_appearance = self.launcher == Some(LauncherMode::Themes);
        let theme = Theme::for_preferences(
            if previewing_appearance {
                self.pending_theme
            } else {
                self.config.theme
            },
            if previewing_appearance {
                self.pending_appearance
            } else {
                self.config.appearance
            },
            window.appearance(),
        );
        if let Some(entity) = self.terminal_entity.as_ref() {
            let (cols, rows) = terminal_grid_size(window, self.terminal_height_px);
            entity.update(cx, |terminal, _| terminal.resize(cols, rows));
        }
        if gpui_component::Theme::global(cx).is_dark() == theme.is_light {
            let mode = if theme.is_light {
                gpui_component::ThemeMode::Light
            } else {
                gpui_component::ThemeMode::Dark
            };
            gpui_component::Theme::change(mode, None, cx);
        }
        let component_theme = gpui_component::Theme::global_mut(cx);
        component_theme.radius = px(2.0);
        component_theme.radius_lg = px(3.0);
        component_theme.font_family = UI_FONT.into();
        component_theme.mono_font_family = MONO_FONT.into();
        component_theme.background = theme.panel_background;
        component_theme.foreground = theme.text;
        component_theme.border = theme.border;
        component_theme.input = theme.border;
        component_theme.muted = theme.surface_background;
        component_theme.muted_foreground = theme.text_muted;
        component_theme.accent = theme.surface_hover;
        component_theme.accent_foreground = theme.text;
        component_theme.primary = theme.accent;
        component_theme.primary_hover = theme.accent_hover;
        component_theme.primary_active = theme.focus;
        component_theme.primary_foreground = theme.app_background;
        component_theme.secondary = theme.surface_background;
        component_theme.secondary_hover = theme.surface_hover;
        component_theme.secondary_active = theme.surface_selected;
        component_theme.secondary_foreground = theme.text;
        component_theme.selection = theme.surface_selected;
        component_theme.ring = theme.focus;
        component_theme.popover = theme.surface_background;
        component_theme.popover_foreground = theme.text;
        component_theme.list = theme.panel_background;
        component_theme.list_hover = theme.surface_hover;
        component_theme.list_active = theme.surface_selected;
        component_theme.list_active_border = theme.border_strong;
        component_theme.scrollbar = theme.panel_background;
        component_theme.scrollbar_thumb = theme.panel_background;
        component_theme.scrollbar_thumb_hover = theme.panel_background;
        component_theme.danger = theme.error;
        component_theme.danger_foreground = theme.text;
        let highlight_theme = Arc::make_mut(&mut component_theme.highlight_theme);
        highlight_theme.style.editor_background = Some(theme.panel_background);
        highlight_theme.style.editor_foreground = Some(theme.text);
        highlight_theme.style.editor_active_line = Some(theme.surface_background);
        highlight_theme.style.editor_line_number = Some(theme.text_disabled);
        highlight_theme.style.editor_active_line_number = Some(theme.text_muted);
        let chrome_button_style = ButtonCustomVariant::new(cx)
            .foreground(theme.text_muted)
            .hover(theme.surface_hover)
            .active(theme.surface_selected);
        let active_chrome_button_style = ButtonCustomVariant::new(cx)
            .color(theme.surface_selected)
            .foreground(theme.accent)
            .hover(theme.surface_hover)
            .active(theme.surface_selected);
        let stop_button_style = ButtonCustomVariant::new(cx)
            .foreground(theme.error)
            .hover(theme.error.opacity(0.15))
            .active(theme.error.opacity(0.25));
        let project_pill_style = ButtonCustomVariant::new(cx)
            .color(theme.surface_selected)
            .foreground(theme.text)
            .hover(theme.surface_hover)
            .active(theme.surface_selected);
        let branch_pill_style = ButtonCustomVariant::new(cx)
            .color(theme.surface_selected)
            .foreground(theme.text_muted)
            .hover(theme.surface_hover)
            .active(theme.surface_selected);
        let window_active = window.is_window_active();
        let window_maximized = window.is_maximized();
        let project_list_focused = self.focus_handle.is_focused(window);
        let titlebar_background = if window_active {
            theme.titlebar_background
        } else {
            theme.titlebar_inactive_background
        };
        let filtered_indices = self.filtered_indices();
        let scan_status = scan_status(&self.scan.state, &self.scan_root);
        let status_color = scan_state_color(theme, &self.scan.state);
        let persistence_status_message = self
            .persistence_history
            .latest()
            .map(persistence_status_text);
        let persistence_status_color = self.persistence_history.latest().map(|event| match event {
            PersistenceEvent::Recovered { .. } => theme.warning,
            PersistenceEvent::Conflict { .. } => theme.error,
        });
        let scan_in_progress = self.scan_cancellation.is_some();
        let has_selected_project = self.selected_project().is_some();
        let selected_project_name = self
            .selected_project()
            .map(|project| project.name.clone())
            .unwrap_or_else(|| "devhub".to_string());
        let titlebar_git_branch = (!self.show_settings
            && self
                .selected_project()
                .is_some_and(|project| project.has_git))
        .then(|| {
            if let LoadState::Loaded(status) = &self.git_status_state {
                if let Some(branch) = &status.branch {
                    return branch.clone();
                }
            }
            if let LoadState::Loaded(branches) = &self.git_branches_state {
                if let Some(branch) = branches.iter().find(|branch| branch.current) {
                    return branch.name.clone();
                }
            }
            "Git".to_string()
        });
        let total_count = self.scan.projects.len();
        let visible_count = filtered_indices.len();
        let filter_active = !self.filter_query.is_empty();
        let show_filter_status = self.project_catalog_open && filter_active;
        let git_notice = (self.activity == Activity::Git)
            .then_some(self.git_notice.as_ref())
            .flatten();
        let git_notice_color = git_notice.map(|notice| match notice.tone {
            GitNoticeTone::Working => theme.warning,
            GitNoticeTone::Success => theme.success,
            GitNoticeTone::Error => theme.error,
        });

        let project_row_indices = filtered_indices.clone();
        let project_list_content: AnyElement = match &self.scan.state {
            ScanState::Scanning => message_panel(
                "SCANNING",
                "Filesystem discovery is running in the background.",
                theme.warning,
                theme,
            )
            .into_any_element(),
            ScanState::Error(error) => {
                message_panel("SCAN FAILED", error.clone(), theme.error, theme).into_any_element()
            }
            ScanState::Empty => message_panel(
                "NO PROJECTS",
                "No projects were found in this folder.",
                theme.text_muted,
                theme,
            )
            .into_any_element(),
            _ if self.scan.projects.is_empty() => message_panel(
                "NO PROJECTS",
                "Open the menu to add a local folder or SSH source.",
                theme.text_muted,
                theme,
            )
            .into_any_element(),
            _ if visible_count == 0 => message_panel(
                "NO MATCHES",
                format!("No projects match \"{}\".", self.filter_query),
                theme.text_muted,
                theme,
            )
            .into_any_element(),
            _ => uniform_list(
                "project-list-rows",
                visible_count,
                cx.processor(
                    move |this, visible_range: std::ops::Range<usize>, _window, cx| {
                        let menu_project_index = this.context_menu.map(|(idx, _, _)| idx);
                        visible_range
                            .map(|filtered_index| {
                                let project_index = project_row_indices[filtered_index];
                                let project = this.scan.projects[project_index].clone();
                                let is_selected = this.selected == Some(project_index);
                                let background = if is_selected {
                                    theme.surface_selected
                                } else {
                                    theme.sidebar_background
                                };
                                let type_color = project_type_color(theme, project.project_type);
                                let show_hover = menu_project_index
                                    .is_none_or(|menu_idx| menu_idx == project_index);

                                let is_pinned = this.is_pinned(&project);
                                div()
                                    .id(("project-row", filtered_index))
                                    .w_full()
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
                                    .when(show_hover, move |this| {
                                        this.hover(move |style| style.bg(theme.surface_hover))
                                    })
                                    .active(move |style| style.bg(theme.surface_background))
                                    .child(if is_selected {
                                        div()
                                            .w(px(17.0))
                                            .flex_shrink_0()
                                            .text_center()
                                            .text_size(px(13.0))
                                            .text_color(theme.accent)
                                            .child("›")
                                            .into_any_element()
                                    } else if is_pinned {
                                        div()
                                            .w(px(17.0))
                                            .flex_shrink_0()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .child(
                                                Icon::new(IconName::Star)
                                                    .size(px(11.0))
                                                    .text_color(theme.accent),
                                            )
                                            .into_any_element()
                                    } else {
                                        div().w(px(17.0)).flex_shrink_0().into_any_element()
                                    })
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
                                                    .w(px(118.0))
                                                    .max_w(px(118.0))
                                                    .whitespace_nowrap()
                                                    .overflow_hidden()
                                                    .text_size(px(13.0))
                                                    .text_color(theme.text)
                                                    .child(project.name.clone()),
                                            )
                                            .child(
                                                div()
                                                    .min_w_0()
                                                    .flex_1()
                                                    .text_right()
                                                    .font_family(MONO_FONT)
                                                    .text_size(px(10.0))
                                                    .text_color(theme.text_disabled)
                                                    .whitespace_nowrap()
                                                    .overflow_hidden()
                                                    .child(
                                                        project.path.to_string_lossy().into_owned(),
                                                    ),
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
                                    .on_mouse_down(
                                        MouseButton::Right,
                                        cx.listener(
                                            move |this: &mut DevHubLite,
                                                  event: &gpui::MouseDownEvent,
                                                  _window,
                                                  cx| {
                                                this.selected = Some(project_index);
                                                this.context_menu = Some((
                                                    project_index,
                                                    event.position.x.into(),
                                                    event.position.y.into(),
                                                ));
                                                cx.notify();
                                            },
                                        ),
                                    )
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.select_project(project_index, window, cx);
                                    }))
                            })
                            .collect::<Vec<_>>()
                    },
                ),
            )
            .track_scroll(self.project_scroll.clone())
            .w_full()
            .size_full()
            .into_any_element(),
        };

        let project_list = div()
            .id("project-list")
            .w_full()
            .min_h_0()
            .flex_1()
            .overflow_hidden()
            .child(project_list_content);
        let project_catalog_panel = self.project_catalog_open.then(|| {
            self.project_catalog_panel(
                theme,
                filter_active,
                visible_count,
                total_count,
                project_list.into_any_element(),
                cx,
            )
        });
        let launcher_panel = self
            .launcher
            .is_some()
            .then(|| self.launcher_panel(theme, window, cx));

        let app = div()
            .id("app")
            .track_focus(&self.focus_handle)
            .key_context("DevHub")
            .on_action(cx.listener(Self::select_previous_project))
            .on_action(cx.listener(Self::select_next_project))
            .on_action(cx.listener(Self::select_first_project))
            .on_action(cx.listener(Self::select_last_project))
            .on_action(cx.listener(Self::scan_current_folder))
            .on_action(cx.listener(Self::open_selected_project))
            .on_action(cx.listener(Self::focus_project_filter))
            .on_action(cx.listener(Self::show_command_palette))
            .on_action(cx.listener(Self::show_project_switcher))
            .on_action(cx.listener(Self::toggle_project_catalog))
            .on_action(cx.listener(Self::show_overview))
            .on_action(cx.listener(Self::show_files))
            .on_action(cx.listener(Self::show_search))
            .on_action(cx.listener(Self::show_git))
            .on_action(cx.listener(Self::toggle_context_pane))
            .on_action(cx.listener(Self::dismiss_launcher))
            .on_action(cx.listener(Self::accept_launcher))
            .on_action(cx.listener(Self::note_component_copy))
            .on_key_down(cx.listener(Self::handle_filter_keydown))
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .border_1()
            .border_color(theme.border)
            .bg(theme.app_background)
            .font_family(UI_FONT)
            .font_weight(FontWeight::MEDIUM)
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
                    .child({
                        let app = cx.entity();
                        Button::new("settings-button")
                            .icon(IconName::Menu)
                            .tooltip("Settings")
                            .xsmall()
                            .compact()
                            .custom(if self.show_settings {
                                active_chrome_button_style
                            } else {
                                chrome_button_style
                            })
                            .w(px(24.0))
                            .h(px(20.0))
                            .ml_1()
                            .on_click(move |event, window, cx| {
                                app.update(cx, |this, cx| {
                                    this.toggle_settings(event, window, cx);
                                });
                            })
                    })
                    .child({
                        let app = cx.entity();
                        Button::new("project-switcher")
                            .label(selected_project_name)
                            .tooltip("Switch project")
                            .xsmall()
                            .compact()
                            .custom(project_pill_style)
                            .h(px(20.0))
                            .max_w(px(200.0))
                            .overflow_hidden()
                            .ml_1()
                            .on_click(move |_, window, cx| {
                                app.update(cx, |this, cx| {
                                    this.open_launcher(LauncherMode::Projects, window, cx);
                                });
                            })
                    })
                    .when_some(titlebar_git_branch, |titlebar, branch| {
                        let app = cx.entity();
                        titlebar.child(
                            Button::new("branch-switcher")
                                .icon(GitBranchIcon)
                                .label(branch)
                                .tooltip("Switch branch")
                                .xsmall()
                                .compact()
                                .custom(branch_pill_style)
                                .h(px(20.0))
                                .max_w(px(170.0))
                                .ml_1()
                                .disabled(self.git_operation_running())
                                .on_click(move |_, window, cx| {
                                    app.update(cx, |this, cx| {
                                        this.open_branch_launcher(window, cx);
                                    });
                                }),
                        )
                    })
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
                                    toggle_window_zoom(window);
                                } else {
                                    begin_window_drag(window);
                                }
                            })
                            .child(
                                div()
                                    .min_w_0()
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .text_size(px(11.0))
                                    .text_color(theme.text_muted)
                                    .child(if self.show_settings {
                                        "Settings"
                                    } else {
                                        self.activity.label()
                                    }),
                            ),
                    )
                    .when(
                        !self.show_settings
                            && matches!(
                                self.activity,
                                Activity::Files | Activity::Search | Activity::Git
                            ),
                        |titlebar| {
                            let app = cx.entity();
                            titlebar.child(
                                Button::new("toggle-context-pane")
                                    .icon(if self.context_pane_visible {
                                        IconName::PanelLeftClose
                                    } else {
                                        IconName::PanelLeftOpen
                                    })
                                    .tooltip("Toggle context pane (Ctrl+B)")
                                    .xsmall()
                                    .compact()
                                    .custom(chrome_button_style)
                                    .on_click(move |_, _, cx| {
                                        app.update(cx, |this, cx| {
                                            this.context_pane_visible = !this.context_pane_visible;
                                            this.resize_target = None;
                                            cx.notify();
                                        });
                                    }),
                            )
                        },
                    )
                    .child({
                        let app = cx.entity();
                        Button::new("open-project-in-zed")
                            .icon(ZedIcon)
                            .tooltip("Open project in Zed")
                            .xsmall()
                            .compact()
                            .custom(chrome_button_style)
                            .w(px(24.0))
                            .h(px(20.0))
                            .disabled(!has_selected_project)
                            .on_click(move |_, _, cx| {
                                app.update(cx, |this, cx| {
                                    this.launch_selected_in_zed(cx);
                                });
                            })
                    })
                    .child({
                        let app = cx.entity();
                        Button::new("open-project-with-picker")
                            .icon(IconName::ExternalLink)
                            .tooltip("Open project in another editor")
                            .xsmall()
                            .compact()
                            .custom(chrome_button_style)
                            .disabled(!has_selected_project)
                            .on_click(move |_, window, cx| {
                                app.update(cx, |this, cx| {
                                    this.open_editor_launcher(window, cx);
                                });
                            })
                    })
                    .child({
                        let app = cx.entity();
                        Button::new("scan-current-folder")
                            .icon(ScanSearchIcon)
                            .tooltip("Refresh projects (Ctrl+R)")
                            .xsmall()
                            .compact()
                            .custom(chrome_button_style)
                            .on_click(move |_, window, cx| {
                                app.update(cx, |this, cx| this.begin_scan(window, cx));
                            })
                    })
                    .when(scan_in_progress, |titlebar| {
                        let app = cx.entity();
                        titlebar.child(
                            Button::new("stop-active-operations")
                                .icon(IconName::CircleX)
                                .tooltip("Stop project scan")
                                .xsmall()
                                .compact()
                                .custom(stop_button_style)
                                .on_click(move |event, window, cx| {
                                    app.update(cx, |this, cx| {
                                        this.stop_scan(event, window, cx);
                                    });
                                }),
                        )
                    })
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
                    .min_w_0()
                    .min_h_0()
                    .flex_1()
                    .flex()
                    .child(if self.show_settings {
                        self.settings_panel(theme, cx).into_any_element()
                    } else {
                        div()
                            .size_full()
                            .min_h_0()
                            .flex()
                            .child(
                                div()
                                    .size_full()
                                    .min_w_0()
                                    .min_h_0()
                                    .flex()
                                    .flex_col()
                                    .bg(theme.panel_background)
                                    .child(self.render_details_panel(theme, window, cx)),
                            )
                            .into_any()
                    }),
            )
            .when_some(self.terminal_panel(theme, cx), |app, terminal| {
                app.child(terminal)
            })
            .child(
                div()
                    .h(px(STATUSBAR_HEIGHT))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .px_2()
                    .border_t_1()
                    .border_color(theme.border)
                    .bg(theme.titlebar_background)
                    .text_size(px(10.0))
                    .child(self.workspace_navigation(theme, cx))
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .flex()
                            .items_center()
                            .justify_end()
                            .gap_2()
                            .ml_2()
                            .child(div().size(px(5.0)).rounded_full().bg(
                                if self.launch_error.is_some() {
                                    theme.error
                                } else if self.copy_feedback.is_some() {
                                    theme.success
                                } else if show_filter_status {
                                    theme.accent
                                } else if git_notice.is_some() {
                                    git_notice_color.unwrap_or(theme.text_muted)
                                } else if persistence_status_message.is_some() {
                                    persistence_status_color.unwrap_or(theme.warning)
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
                                } else if show_filter_status {
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
                                } else if let Some(notice) = git_notice {
                                    div()
                                        .font_family(MONO_FONT)
                                        .text_color(git_notice_color.unwrap_or(theme.text_muted))
                                        .whitespace_nowrap()
                                        .overflow_hidden()
                                        .child(notice.text.clone())
                                } else if let Some(message) = &persistence_status_message {
                                    div()
                                        .font_family(MONO_FONT)
                                        .text_color(
                                            persistence_status_color.unwrap_or(theme.warning),
                                        )
                                        .whitespace_nowrap()
                                        .overflow_hidden()
                                        .child(message.clone())
                                } else {
                                    div()
                                        .whitespace_nowrap()
                                        .overflow_hidden()
                                        .text_color(theme.text_muted)
                                        .child(scan_status)
                                },
                            )),
                    )
                    .child({
                        let app = cx.entity();
                        let style = if self.terminal_entity.is_some() {
                            active_chrome_button_style
                        } else {
                            chrome_button_style
                        };
                        Button::new("toggle-terminal")
                            .icon(IconName::PanelBottom)
                            .tooltip("Toggle Terminal (Ctrl+`)")
                            .xsmall()
                            .compact()
                            .custom(style)
                            .on_click(move |_, window, cx| {
                                app.update(cx, |this, cx| {
                                    this.toggle_terminal(window, cx);
                                });
                            })
                    }),
            )
            .when_some(project_catalog_panel, |app, catalog| app.child(catalog))
            .when_some(launcher_panel, |app, launcher| app.child(launcher));

        if let Some((project_index, raw_x, raw_y)) = self.context_menu {
            let bounds = window.bounds();
            let win_w: f32 = bounds.size.width.into();
            let win_h: f32 = bounds.size.height.into();
            let menu_w = 160.0;
            let menu_h = 84.0;
            let context_x = (raw_x + menu_w).min(win_w) - menu_w;
            let context_x = context_x.max(0.0);
            let context_y = (raw_y + menu_h).min(win_h) - menu_h;
            let context_y = context_y.max(0.0);

            let is_pinned = self
                .scan
                .projects
                .get(project_index)
                .is_some_and(|p| self.is_pinned(p));

            let pin_listener = cx.listener(
                move |this: &mut DevHubLite, _: &gpui::MouseDownEvent, _window, cx| {
                    this.toggle_pin(project_index, cx);
                },
            );

            let hide_listener = cx.listener(
                move |this: &mut DevHubLite, _: &gpui::MouseDownEvent, _window, cx| {
                    this.toggle_hide(project_index, cx);
                },
            );

            let entity = cx.entity();

            let backdrop_close =
                move |_: &gpui::MouseDownEvent, _: &mut gpui::Window, cx: &mut gpui::App| {
                    entity.update(cx, |this, cx| {
                        this.context_menu = None;
                        cx.notify();
                    });
                };

            div()
                .relative()
                .size_full()
                .child(app)
                .child(
                    div()
                        .absolute()
                        .inset_0()
                        .on_mouse_down(MouseButton::Left, backdrop_close)
                        .child(
                            div()
                                .absolute()
                                .left(px(context_x))
                                .top(px(context_y))
                                .min_w(px(140.0))
                                .bg(theme.surface_background)
                                .border_1()
                                .border_color(theme.border)
                                .rounded_sm()
                                .py_1()
                                .child(
                                    div()
                                        .px_2()
                                        .py_1()
                                        .text_size(px(12.0))
                                        .text_color(theme.text)
                                        .cursor_pointer()
                                        .hover(move |style| style.bg(theme.surface_hover))
                                        .child(if is_pinned { "Unpin" } else { "Pin" })
                                        .on_mouse_down(MouseButton::Left, pin_listener),
                                )
                                .child(
                                    div()
                                        .px_2()
                                        .py_1()
                                        .text_size(px(12.0))
                                        .text_color(theme.text)
                                        .cursor_pointer()
                                        .hover(move |style| style.bg(theme.surface_hover))
                                        .child("Hide")
                                        .on_mouse_down(MouseButton::Left, hide_listener),
                                ),
                        ),
                )
                .into_any()
        } else if let Some((selection, raw_x, raw_y)) = self.git_context_menu.clone() {
            let bounds = window.bounds();
            let win_w: f32 = bounds.size.width.into();
            let win_h: f32 = bounds.size.height.into();
            let menu_w = 176.0;
            let menu_h = if selection.kind == GitDiffKind::Unstaged {
                82.0
            } else {
                58.0
            };
            let context_x = ((raw_x + menu_w).min(win_w) - menu_w).max(0.0);
            let context_y = ((raw_y + menu_h).min(win_h) - menu_h).max(0.0);
            let entity = cx.entity();
            let close_entity = entity.clone();
            let action_entity = entity.clone();
            let copy_entity = entity.clone();
            let discard_entity = entity.clone();
            let discard_change = selection.change.clone();
            let path = selection.change.path.to_string_lossy().into_owned();
            let action_label = match selection.kind {
                GitDiffKind::Unstaged => "Stage",
                GitDiffKind::Staged => "Unstage",
            };
            let action_kind = selection.kind;
            let action_path = selection.change.path.clone();

            div()
                .relative()
                .size_full()
                .child(app)
                .child(
                    div()
                        .absolute()
                        .inset_0()
                        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                            close_entity.update(cx, |this, cx| {
                                this.git_context_menu = None;
                                cx.notify();
                            });
                        })
                        .child(
                            div()
                                .absolute()
                                .left(px(context_x))
                                .top(px(context_y))
                                .w(px(menu_w))
                                .bg(theme.surface_background)
                                .border_1()
                                .border_color(theme.border)
                                .rounded_sm()
                                .py_1()
                                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                .child(git_context_menu_item(action_label, theme).on_mouse_down(
                                    MouseButton::Left,
                                    move |_, _, cx| {
                                        let action = match action_kind {
                                            GitDiffKind::Unstaged => {
                                                GitAction::Stage(vec![action_path.clone()])
                                            }
                                            GitDiffKind::Staged => {
                                                GitAction::Unstage(vec![action_path.clone()])
                                            }
                                        };
                                        action_entity.update(cx, |this, cx| {
                                            this.run_git_action(action, cx);
                                        });
                                    },
                                ))
                                .when(selection.kind == GitDiffKind::Unstaged, |menu| {
                                    menu.child(
                                        git_context_menu_item("Discard changes", theme)
                                            .text_color(theme.error)
                                            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                                                let change = discard_change.clone();
                                                discard_entity.update(cx, |this, cx| {
                                                    this.git_context_menu = None;
                                                    this.git_pending_discard = Some(change);
                                                    cx.notify();
                                                });
                                            }),
                                    )
                                })
                                .child(git_context_menu_item("Copy path", theme).on_mouse_down(
                                    MouseButton::Left,
                                    move |_, _, cx| {
                                        cx.write_to_clipboard(ClipboardItem::new_string(
                                            path.clone(),
                                        ));
                                        copy_entity.update(cx, |this, cx| {
                                            this.git_context_menu = None;
                                            this.show_copy_feedback("COPIED PATH", cx);
                                        });
                                    },
                                )),
                        ),
                )
                .into_any()
        } else if self.resize_target == Some(ResizeTarget::Terminal) {
            div()
                .relative()
                .size_full()
                .child(app)
                .child(
                    div()
                        .absolute()
                        .inset_0()
                        .cursor(CursorStyle::ResizeUpDown)
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                            let bounds = window.bounds();
                            let win_h: f32 = bounds.size.height.into();
                            let raw: f32 = event.position.y.into();
                            let max_h = win_h - (TITLEBAR_HEIGHT + STATUSBAR_HEIGHT);
                            this.terminal_height_px =
                                (win_h - STATUSBAR_HEIGHT - raw).clamp(MIN_TERMINAL_HEIGHT, max_h);
                            if let Some(entity) = this.terminal_entity.as_ref() {
                                let (cols, rows) =
                                    terminal_grid_size(window, this.terminal_height_px);
                                entity.update(cx, |terminal, _| terminal.resize(cols, rows));
                            }
                            cx.notify();
                        }))
                        .on_mouse_up(
                            MouseButton::Left,
                            cx.listener(|this, _event: &MouseUpEvent, _window, cx| {
                                this.resize_target = None;
                                cx.notify();
                            }),
                        ),
                )
                .into_any()
        } else {
            app.into_any()
        }
    }
}

#[derive(Clone)]
struct HistoryFileTreeEntry {
    label: String,
    guides: Vec<bool>,
    is_last: bool,
    file_index: Option<usize>,
}

#[derive(Clone, Default)]
struct HistoryGraphRow {
    node_lane: usize,
    top_lanes: usize,
    bottom_lanes: usize,
    connections: Vec<(usize, usize)>,
}

fn terminal_grid_size(window: &Window, panel_height: f32) -> (usize, usize) {
    let width: f32 = window.bounds().size.width.into();
    let cols = ((width - 16.0) / 7.0).floor().max(20.0) as usize;
    let rows = ((panel_height - 28.0) / 16.0).floor().max(4.0) as usize;
    (cols, rows)
}

fn project_locator(project: &Project) -> ProjectLocator {
    ProjectLocator {
        path: project.path.clone(),
        remote_host: project.source.host().map(str::to_string),
    }
}

fn project_locator_matches(project: &Project, locator: &ProjectLocator) -> bool {
    project.path == locator.path && project.source.host() == locator.remote_host.as_deref()
}

fn git_change_label(change: &GitFileChange) -> String {
    change.original_path.as_ref().map_or_else(
        || change.path.to_string_lossy().into_owned(),
        |original| {
            format!(
                "{} -> {}",
                original.to_string_lossy(),
                change.path.to_string_lossy()
            )
        },
    )
}

fn git_tree_entries(changes: &[&GitFileChange], kind: GitDiffKind) -> Vec<GitTreeEntry> {
    let mut directories = HashSet::new();
    for change in changes {
        let mut current = change.path.parent();
        while let Some(directory) = current.filter(|directory| !directory.as_os_str().is_empty()) {
            directories.insert(directory.to_path_buf());
            current = directory.parent();
        }
    }

    let mut entries = Vec::new();
    append_git_tree_entries(Path::new(""), 0, &directories, changes, kind, &mut entries);
    entries
}

fn append_git_tree_entries(
    parent: &Path,
    depth: usize,
    directories: &HashSet<PathBuf>,
    changes: &[&GitFileChange],
    kind: GitDiffKind,
    entries: &mut Vec<GitTreeEntry>,
) {
    let mut child_directories = directories
        .iter()
        .filter(|directory| directory.parent().unwrap_or(Path::new("")) == parent)
        .collect::<Vec<_>>();
    child_directories.sort();
    for directory in child_directories {
        let label = directory.file_name().map_or_else(
            || directory.to_string_lossy().into_owned(),
            |name| name.to_string_lossy().into_owned(),
        );
        entries.push(GitTreeEntry {
            path: directory.clone(),
            label,
            depth,
            selection: None,
        });
        append_git_tree_entries(directory, depth + 1, directories, changes, kind, entries);
    }

    let mut child_changes = changes
        .iter()
        .filter(|change| change.path.parent().unwrap_or(Path::new("")) == parent)
        .copied()
        .collect::<Vec<_>>();
    child_changes.sort_by(|left, right| left.path.cmp(&right.path));
    for change in child_changes {
        let label = change.path.file_name().map_or_else(
            || change.path.to_string_lossy().into_owned(),
            |name| name.to_string_lossy().into_owned(),
        );
        let label = change
            .original_path
            .as_ref()
            .map_or(label.clone(), |original| {
                let original = original.file_name().map_or_else(
                    || original.to_string_lossy().into_owned(),
                    |name| name.to_string_lossy().into_owned(),
                );
                format!("{original} -> {label}")
            });
        entries.push(GitTreeEntry {
            path: change.path.clone(),
            label,
            depth,
            selection: Some(GitSelection {
                change: change.clone(),
                kind,
            }),
        });
    }
}

fn git_stat_value(value: Option<usize>) -> String {
    value.map_or_else(|| "?".into(), |value| value.to_string())
}

fn history_table_header(theme: Theme) -> Div {
    div()
        .h(px(27.0))
        .w_full()
        .min_w(px(620.0))
        .flex_shrink_0()
        .flex()
        .items_center()
        .border_b_1()
        .border_color(theme.border)
        .bg(theme.surface_background)
        .font_family(MONO_FONT)
        .text_size(px(9.0))
        .text_color(theme.text_muted)
        .child(div().w(px(112.0)).flex_shrink_0().px_2().child("Graph"))
        .child(div().min_w(px(150.0)).flex_1().px_1().child("Description"))
        .child(
            div()
                .w(px(122.0))
                .flex_shrink_0()
                .px_1()
                .border_l_1()
                .border_color(theme.border)
                .child("Date"),
        )
        .child(
            div()
                .w(px(104.0))
                .flex_shrink_0()
                .px_1()
                .border_l_1()
                .border_color(theme.border)
                .child("Author"),
        )
        .child(
            div()
                .w(px(66.0))
                .flex_shrink_0()
                .px_1()
                .border_l_1()
                .border_color(theme.border)
                .child("Commit"),
        )
}

fn history_commit_row(
    index: usize,
    commit: CommitEntry,
    graph: HistoryGraphRow,
    selected: bool,
    theme: Theme,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let subject = commit
        .message
        .lines()
        .next()
        .unwrap_or_default()
        .to_string();
    let short_hash = commit.hash.chars().take(7).collect::<String>();
    div()
        .id(("history-commit", index))
        .h(px(26.0))
        .w_full()
        .min_w(px(620.0))
        .flex_shrink_0()
        .flex()
        .items_center()
        .cursor_pointer()
        .bg(if selected {
            theme.surface_selected
        } else {
            theme.panel_background
        })
        .hover(move |style| {
            if selected {
                style
            } else {
                style.bg(theme.surface_hover)
            }
        })
        .font_family(MONO_FONT)
        .text_size(px(9.0))
        .child(history_graph_view(graph, theme))
        .child(
            div()
                .min_w(px(150.0))
                .flex_1()
                .flex()
                .items_center()
                .gap_1()
                .px_1()
                .whitespace_nowrap()
                .overflow_hidden()
                .children(
                    commit
                        .refs
                        .into_iter()
                        .map(|reference| history_ref_badge(reference, theme)),
                )
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .text_color(theme.text_muted)
                        .child(subject),
                ),
        )
        .child(
            div()
                .w(px(122.0))
                .h_full()
                .flex_shrink_0()
                .flex()
                .items_center()
                .px_1()
                .border_l_1()
                .border_color(theme.border.opacity(0.65))
                .whitespace_nowrap()
                .overflow_hidden()
                .text_color(theme.text_muted)
                .child(commit.date),
        )
        .child(
            div()
                .w(px(104.0))
                .h_full()
                .flex_shrink_0()
                .flex()
                .items_center()
                .px_1()
                .border_l_1()
                .border_color(theme.border.opacity(0.65))
                .whitespace_nowrap()
                .overflow_hidden()
                .text_color(theme.text_muted)
                .child(commit.author),
        )
        .child(
            div()
                .w(px(66.0))
                .h_full()
                .flex_shrink_0()
                .flex()
                .items_center()
                .px_1()
                .border_l_1()
                .border_color(theme.border.opacity(0.65))
                .text_color(theme.text_disabled)
                .child(short_hash),
        )
        .on_click(on_click)
        .into_any_element()
}

fn history_ref_badge(reference: String, theme: Theme) -> AnyElement {
    div()
        .flex_shrink_0()
        .h(px(16.0))
        .flex()
        .items_center()
        .px_1()
        .border_1()
        .border_color(theme.accent.opacity(0.8))
        .rounded(px(2.0))
        .bg(theme.accent.opacity(0.08))
        .font_family(MONO_FONT)
        .text_size(px(8.0))
        .text_color(theme.text)
        .child(reference)
        .into_any_element()
}

fn history_graph_rows(commits: &[CommitEntry]) -> Vec<HistoryGraphRow> {
    let mut lanes = Vec::<String>::new();
    let mut rows = Vec::with_capacity(commits.len());
    for (row_index, commit) in commits.iter().enumerate() {
        let node_lane = lanes
            .iter()
            .position(|hash| hash == &commit.hash)
            .unwrap_or_else(|| {
                lanes.insert(0, commit.hash.clone());
                0
            });

        let before = lanes.clone();
        let mut after = before.clone();
        after.remove(node_lane);
        let mut insertion_lane = node_lane.min(after.len());
        for parent in &commit.parents {
            if !after.contains(parent) {
                after.insert(insertion_lane, parent.clone());
                insertion_lane += 1;
            }
        }

        let mut connections = Vec::new();
        for (from_lane, hash) in before.iter().enumerate() {
            if from_lane == node_lane {
                continue;
            }
            if let Some(to_lane) = after.iter().position(|candidate| candidate == hash) {
                connections.push((from_lane, to_lane));
            }
        }
        for parent in &commit.parents {
            if let Some(to_lane) = after.iter().position(|candidate| candidate == parent) {
                connections.push((node_lane, to_lane));
            }
        }

        rows.push(HistoryGraphRow {
            node_lane,
            top_lanes: if row_index == 0 { 0 } else { before.len() },
            bottom_lanes: after.len(),
            connections,
        });
        lanes = after;
    }
    rows
}

fn history_graph_view(graph: HistoryGraphRow, theme: Theme) -> AnyElement {
    const MAX_LANES: usize = 8;
    const LANE_WIDTH: f32 = 13.0;
    const LANE_ORIGIN: f32 = 10.0;
    const ROW_MIDDLE: f32 = 13.0;

    let line_color = theme.accent.opacity(0.82);
    let lane_x = |lane: usize| LANE_ORIGIN + lane as f32 * LANE_WIDTH;
    let mut segments = Vec::new();

    for lane in 0..graph.top_lanes.min(MAX_LANES) {
        segments.push(
            div()
                .absolute()
                .left(px(lane_x(lane)))
                .top(px(0.0))
                .w(px(2.0))
                .h(px(ROW_MIDDLE + 1.0))
                .bg(line_color)
                .into_any_element(),
        );
    }
    for lane in 0..graph.bottom_lanes.min(MAX_LANES) {
        segments.push(
            div()
                .absolute()
                .left(px(lane_x(lane)))
                .top(px(ROW_MIDDLE))
                .w(px(2.0))
                .bottom(px(0.0))
                .bg(line_color)
                .into_any_element(),
        );
    }
    for (from_lane, to_lane) in graph.connections {
        if from_lane == to_lane || from_lane >= MAX_LANES || to_lane >= MAX_LANES {
            continue;
        }
        let from_x = lane_x(from_lane);
        let to_x = lane_x(to_lane);
        segments.push(
            div()
                .absolute()
                .left(px(from_x.min(to_x)))
                .top(px(ROW_MIDDLE))
                .w(px((from_x - to_x).abs() + 2.0))
                .h(px(2.0))
                .bg(line_color)
                .into_any_element(),
        );
    }
    if graph.node_lane < MAX_LANES {
        segments.push(
            div()
                .absolute()
                .left(px(lane_x(graph.node_lane) - 2.0))
                .top(px(ROW_MIDDLE - 2.0))
                .size(px(6.0))
                .rounded_full()
                .bg(theme.accent)
                .into_any_element(),
        );
    }

    div()
        .relative()
        .w(px(112.0))
        .h_full()
        .flex_shrink_0()
        .overflow_hidden()
        .children(segments)
        .into_any_element()
}

fn commit_totals(commit: &CommitEntry) -> (usize, usize, usize) {
    commit.files.iter().fold((0, 0, 0), |totals, file| {
        (
            totals.0 + file.additions.unwrap_or_default(),
            totals.1 + file.deletions.unwrap_or_default(),
            totals.2 + usize::from(file.additions.is_none() || file.deletions.is_none()),
        )
    })
}

fn author_initials(author: &str) -> String {
    let initials = author
        .split_whitespace()
        .take(2)
        .filter_map(|part| part.chars().next())
        .flat_map(char::to_uppercase)
        .collect::<String>();
    if initials.is_empty() {
        "?".into()
    } else {
        initials
    }
}

fn history_file_tree_entries(files: &[CommitFileChange]) -> Vec<HistoryFileTreeEntry> {
    let mut directories = BTreeSet::new();
    for file in files {
        let mut parent = Path::new(&file.path).parent();
        while let Some(directory) = parent.filter(|directory| !directory.as_os_str().is_empty()) {
            directories.insert(directory.to_path_buf());
            parent = directory.parent();
        }
    }
    let mut entries = Vec::new();
    append_history_file_tree_entries(Path::new(""), &[], &directories, files, &mut entries);
    entries
}

fn append_history_file_tree_entries(
    parent: &Path,
    guides: &[bool],
    directories: &BTreeSet<PathBuf>,
    files: &[CommitFileChange],
    entries: &mut Vec<HistoryFileTreeEntry>,
) {
    let child_directories = directories
        .iter()
        .filter(|directory| directory.parent().unwrap_or(Path::new("")) == parent)
        .collect::<Vec<_>>();
    let mut child_files = files
        .iter()
        .enumerate()
        .filter(|(_, file)| Path::new(&file.path).parent().unwrap_or(Path::new("")) == parent)
        .collect::<Vec<_>>();
    child_files.sort_by(|(_, left), (_, right)| left.path.cmp(&right.path));
    let child_count = child_directories.len() + child_files.len();
    let mut child_index = 0;

    for directory in child_directories {
        let is_last = child_index + 1 == child_count;
        child_index += 1;
        entries.push(HistoryFileTreeEntry {
            label: display_history_path(
                &directory
                    .file_name()
                    .unwrap_or(directory.as_os_str())
                    .to_string_lossy(),
            ),
            guides: guides.to_vec(),
            is_last,
            file_index: None,
        });
        let mut child_guides = guides.to_vec();
        child_guides.push(!is_last);
        append_history_file_tree_entries(directory, &child_guides, directories, files, entries);
    }

    for (file_index, file) in child_files {
        let is_last = child_index + 1 == child_count;
        child_index += 1;
        let label = Path::new(&file.path).file_name().map_or_else(
            || file.path.clone(),
            |name| name.to_string_lossy().into_owned(),
        );
        let label = file
            .original_path
            .as_ref()
            .map_or(label.clone(), |original| {
                let original = Path::new(original).file_name().map_or_else(
                    || original.clone(),
                    |name| name.to_string_lossy().into_owned(),
                );
                format!("{original} -> {label}")
            });
        entries.push(HistoryFileTreeEntry {
            label: display_history_path(&label),
            guides: guides.to_vec(),
            is_last,
            file_index: Some(file_index),
        });
    }
}

fn history_tree_guides(entry: &HistoryFileTreeEntry, theme: Theme) -> AnyElement {
    const GUIDE_WIDTH: f32 = 12.0;
    const GUIDE_X: f32 = 5.0;
    const ROW_MIDDLE: f32 = 11.0;

    let line_color = theme.text_disabled.opacity(0.48);
    let mut segments = Vec::new();
    for (depth, continues) in entry.guides.iter().enumerate() {
        if *continues {
            segments.push(
                div()
                    .absolute()
                    .left(px(GUIDE_X + depth as f32 * GUIDE_WIDTH))
                    .top(px(0.0))
                    .bottom(px(0.0))
                    .w(px(1.0))
                    .bg(line_color)
                    .into_any_element(),
            );
        }
    }

    let branch_x = GUIDE_X + entry.guides.len() as f32 * GUIDE_WIDTH;
    segments.push(
        div()
            .absolute()
            .left(px(branch_x))
            .top(px(0.0))
            .w(px(1.0))
            .h(px(if entry.is_last { ROW_MIDDLE } else { 22.0 }))
            .bg(line_color)
            .into_any_element(),
    );
    segments.push(
        div()
            .absolute()
            .left(px(branch_x))
            .top(px(ROW_MIDDLE))
            .w(px(GUIDE_WIDTH - GUIDE_X + 1.0))
            .h(px(1.0))
            .bg(line_color)
            .into_any_element(),
    );

    div()
        .relative()
        .h_full()
        .w(px((entry.guides.len() as f32 + 1.0) * GUIDE_WIDTH))
        .flex_shrink_0()
        .children(segments)
        .into_any_element()
}

fn display_history_path(path: &str) -> String {
    path.chars()
        .flat_map(|character| character.escape_default())
        .collect()
}

fn diff_line_text(line: &DiffLine) -> Option<String> {
    match line.kind {
        DiffLineKind::FileHeader => None,
        DiffLineKind::Addition => Some(line.text.strip_prefix('+').unwrap_or(&line.text).into()),
        DiffLineKind::Deletion => Some(line.text.strip_prefix('-').unwrap_or(&line.text).into()),
        DiffLineKind::Context => Some(line.text.strip_prefix(' ').unwrap_or(&line.text).into()),
        DiffLineKind::HunkHeader => Some(humanize_hunk_header(&line.text)),
        DiffLineKind::Metadata => humanize_diff_metadata(&line.text),
    }
}

fn humanize_hunk_header(header: &str) -> String {
    let Some(ranges) = header.strip_prefix("@@ -") else {
        return "Changed lines".into();
    };
    let Some((ranges, context)) = ranges.split_once(" @@") else {
        return "Changed lines".into();
    };
    let Some((old, new)) = ranges.split_once(" +") else {
        return "Changed lines".into();
    };
    let context = context.trim();
    if context.is_empty() {
        format!("Changed lines {old} -> {new}")
    } else {
        format!("Changed lines {old} -> {new}  {context}")
    }
}

fn humanize_diff_metadata(metadata: &str) -> Option<String> {
    if metadata == "\\ No newline at end of file" {
        Some("No newline at end of file".into())
    } else if metadata.starts_with("Binary files ") || metadata == "GIT binary patch" {
        Some("Binary file changed".into())
    } else if let Some(mode) = metadata.strip_prefix("new file mode ") {
        Some(format!("New file, mode {mode}"))
    } else if let Some(mode) = metadata.strip_prefix("deleted file mode ") {
        Some(format!("Deleted file, mode {mode}"))
    } else if let Some(path) = metadata.strip_prefix("rename from ") {
        Some(format!("Renamed from {}", display_history_path(path)))
    } else if let Some(path) = metadata.strip_prefix("rename to ") {
        Some(format!("Renamed to {}", display_history_path(path)))
    } else {
        metadata
            .strip_prefix("similarity index ")
            .map(|similarity| format!("Similarity {similarity}"))
    }
}

fn commit_mode_row(
    id: &'static str,
    label: &'static str,
    selected: bool,
    theme: Theme,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(24.0))
        .flex()
        .items_center()
        .gap_1()
        .px_1()
        .border_b_1()
        .border_color(theme.border.opacity(0.35))
        .cursor_pointer()
        .hover(move |style| style.bg(theme.surface_hover))
        .child(
            div().w(px(14.0)).flex().justify_center().child(
                Icon::new(IconName::Check)
                    .size(px(10.0))
                    .text_color(if selected {
                        theme.accent
                    } else {
                        theme.surface_background
                    }),
            ),
        )
        .child(
            div()
                .font_family(MONO_FONT)
                .text_size(px(9.0))
                .text_color(theme.text)
                .child(label),
        )
        .on_click(on_click)
}

fn git_context_menu_item(label: &'static str, theme: Theme) -> Div {
    div()
        .h(px(24.0))
        .px_2()
        .flex()
        .items_center()
        .text_size(px(11.0))
        .text_color(theme.text)
        .cursor_pointer()
        .hover(move |style| style.bg(theme.surface_hover))
        .child(label)
}

fn local_git_directory(project_root: &Path) -> PathBuf {
    let dot_git = project_root.join(".git");
    if dot_git.is_dir() {
        return dot_git;
    }
    let Ok(contents) = std::fs::read_to_string(&dot_git) else {
        return dot_git;
    };
    let Some(path) = contents.trim().strip_prefix("gitdir:").map(str::trim) else {
        return dot_git;
    };
    let path = PathBuf::from(path);
    if path.is_absolute() {
        path
    } else {
        project_root.join(path)
    }
}

fn diff_gutter_number(number: Option<usize>, theme: Theme) -> Div {
    div()
        .w(px(38.0))
        .h_full()
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_end()
        .pr_2()
        .border_r_1()
        .border_color(theme.border.opacity(0.72))
        .text_color(theme.text_disabled)
        .child(number.map(|number| number.to_string()).unwrap_or_default())
}

fn git_watch_event_is_relevant(
    project_root: &Path,
    git_directory: &Path,
    event: &notify::Event,
) -> bool {
    event.paths.iter().any(|path| {
        if let Ok(relative) = path.strip_prefix(project_root) {
            let mut components = relative.components();
            let Some(first) = components.next() else {
                return true;
            };
            if first.as_os_str() != std::ffi::OsStr::new(".git") {
                return true;
            }
            return git_metadata_path_is_relevant(components.as_path());
        }
        path.strip_prefix(git_directory)
            .is_ok_and(git_metadata_path_is_relevant)
    })
}

fn git_metadata_path_is_relevant(path: &Path) -> bool {
    let Some(name) = path.components().next() else {
        return true;
    };
    let name = name.as_os_str().to_string_lossy();
    name.starts_with("index")
        || matches!(
            name.as_ref(),
            "HEAD"
                | "ORIG_HEAD"
                | "MERGE_HEAD"
                | "REBASE_HEAD"
                | "CHERRY_PICK_HEAD"
                | "REVERT_HEAD"
                | "packed-refs"
                | "refs"
                | "config"
                | "commondir"
        )
}

fn git_error_notice(error: &GitError) -> String {
    if !matches!(
        error.kind,
        GitErrorKind::CommandFailed | GitErrorKind::Validation
    ) {
        return error.status_text().into();
    }
    let Some(detail) = error
        .detail
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty())
    else {
        return error.status_text().into();
    };
    let detail = detail
        .strip_prefix("fatal: ")
        .or_else(|| detail.strip_prefix("error: "))
        .unwrap_or(detail);
    let mut compact = detail.chars().take(140).collect::<String>();
    if detail.chars().count() > 140 {
        compact.push_str("...");
    }
    format!("Git: {compact}")
}

fn relative_modified(timestamp: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(timestamp);
    let elapsed = now.saturating_sub(timestamp);
    match elapsed {
        0..=59 => "just now".into(),
        60..=3_599 => format!("{}m ago", elapsed / 60),
        3_600..=86_399 => format!("{}h ago", elapsed / 3_600),
        _ => format!("{}d ago", elapsed / 86_400),
    }
}

fn remote_parent_path(path: &str) -> Option<String> {
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        return None;
    }
    let parent = trimmed.rsplit_once('/').map(|(parent, _)| parent)?;
    Some(if parent.is_empty() { "/" } else { parent }.to_string())
}

pub(crate) fn run() {
    Application::new().with_assets(Assets).run(|cx: &mut App| {
        Assets::register_fonts(cx).expect("failed to register the bundled terminal font");
        gpui_component::init(cx);
        gpui_component::Theme::change(gpui_component::ThemeMode::Dark, None, cx);
        let component_theme = gpui_component::Theme::global_mut(cx);
        component_theme.radius = px(2.0);
        component_theme.radius_lg = px(2.0);
        component_theme.font_family = UI_FONT.into();
        component_theme.mono_font_family = MONO_FONT.into();
        cx.bind_keys([
            KeyBinding::new("up", SelectPreviousProject, Some("DevHub")),
            KeyBinding::new("down", SelectNextProject, Some("DevHub")),
            KeyBinding::new("home", SelectFirstProject, Some("DevHub")),
            KeyBinding::new("end", SelectLastProject, Some("DevHub")),
            KeyBinding::new("ctrl-r", ScanCurrentFolder, Some("DevHub")),
            KeyBinding::new("ctrl-f", FocusProjectFilter, Some("DevHub")),
            KeyBinding::new("ctrl-shift-p", ShowCommandPalette, Some("DevHub")),
            KeyBinding::new("ctrl-p", ShowProjectSwitcher, Some("DevHub")),
            KeyBinding::new("ctrl-1", ToggleProjectCatalog, Some("DevHub")),
            KeyBinding::new("ctrl-2", ShowOverview, Some("DevHub")),
            KeyBinding::new("ctrl-3", ShowFiles, Some("DevHub")),
            KeyBinding::new("ctrl-4", ShowSearch, Some("DevHub")),
            KeyBinding::new("ctrl-5", ShowGit, Some("DevHub")),
            KeyBinding::new("ctrl-6", ShowHistory, Some("DevHub")),
            KeyBinding::new("ctrl-b", ToggleContextPane, Some("DevHub")),
            KeyBinding::new("up", SelectPreviousProject, Some("Input && DevHubLauncher")),
            KeyBinding::new("down", SelectNextProject, Some("Input && DevHubLauncher")),
            KeyBinding::new("home", SelectFirstProject, Some("Input && DevHubLauncher")),
            KeyBinding::new("end", SelectLastProject, Some("Input && DevHubLauncher")),
            KeyBinding::new("escape", DismissLauncher, Some("Input && DevHubLauncher")),
            KeyBinding::new("enter", AcceptLauncher, Some("Input && DevHubLauncher")),
            KeyBinding::new("up", SelectPreviousProject, Some("Input && ProjectFilter")),
            KeyBinding::new("down", SelectNextProject, Some("Input && ProjectFilter")),
            KeyBinding::new("home", SelectFirstProject, Some("Input && ProjectFilter")),
            KeyBinding::new("end", SelectLastProject, Some("Input && ProjectFilter")),
            KeyBinding::new("escape", DismissLauncher, Some("Input && ProjectFilter")),
            KeyBinding::new("enter", OpenSelectedProject, Some("Input && ProjectFilter")),
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
                app.update(cx, |this, cx| this.restore_last_project(cx));
                cx.new(|cx| gpui_component::Root::new(app, window, cx))
            },
        )
        .expect("failed to open the main window");

        cx.activate(true);
    });
}
