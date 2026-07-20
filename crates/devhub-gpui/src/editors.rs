use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use devhub_core::{Project, ProjectSource, ProjectType};
use serde::Deserialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EditorProjectKind {
    Rust,
    Web,
    Go,
    Python,
    Native,
    DotNet,
    Java,
    Php,
    Ruby,
    Swift,
    Dart,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum EditorKind {
    Local,
    Code {
        remote: bool,
    },
    JetBrains {
        product_code: String,
        remote: bool,
        transport: Option<PathBuf>,
        project_kinds: Vec<EditorProjectKind>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DetectedEditor {
    id: String,
    label: String,
    executable: PathBuf,
    kind: EditorKind,
}

impl DetectedEditor {
    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn supports_remote(&self) -> bool {
        match &self.kind {
            EditorKind::Code { remote } => *remote,
            EditorKind::JetBrains {
                remote, transport, ..
            } => *remote && transport.is_some(),
            EditorKind::Local => false,
        }
    }

    pub fn supports_project(&self, project: &Project) -> bool {
        let EditorKind::JetBrains { project_kinds, .. } = &self.kind else {
            return true;
        };
        !project_kinds.is_empty()
            && effective_project_kinds(project)
                .iter()
                .any(|project_kind| project_kinds.contains(project_kind))
    }

    pub fn launch(&self, project: &Project) -> Result<(), String> {
        let request = self.launch_request(project)?;
        let mut command = Command::new(&request.program);
        command
            .args(request.args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        configure_editor_process(&mut command);
        command
            .spawn()
            .map(|_| ())
            .map_err(|error| format!("Unable to start {}: {error}", self.label))
    }

    fn launch_request(&self, project: &Project) -> Result<EditorLaunchRequest, String> {
        match (&self.kind, &project.source) {
            (_, ProjectSource::Local) => Ok(EditorLaunchRequest {
                program: self.executable.clone(),
                args: vec![project.path.to_string_lossy().into_owned()],
            }),
            (EditorKind::Code { remote: true }, ProjectSource::Remote { host, .. }) => {
                Ok(EditorLaunchRequest {
                    program: self.executable.clone(),
                    args: vec![
                        "--remote".into(),
                        format!("ssh-remote+{host}"),
                        normalized_remote_path(&project.path),
                    ],
                })
            }
            (
                EditorKind::JetBrains {
                    product_code,
                    remote: true,
                    transport: Some(transport),
                    ..
                },
                ProjectSource::Remote { host, .. },
            ) => Ok(EditorLaunchRequest {
                program: transport.clone(),
                args: vec![jetbrains_remote_uri(host, &project.path, product_code)],
            }),
            _ => Err(format!(
                "{} does not expose an SSH project launcher",
                self.label
            )),
        }
    }
}

#[cfg(target_os = "windows")]
fn configure_editor_process(command: &mut Command) {
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(target_os = "windows"))]
fn configure_editor_process(_command: &mut Command) {}

#[derive(Debug, PartialEq, Eq)]
struct EditorLaunchRequest {
    program: PathBuf,
    args: Vec<String>,
}

#[derive(Debug)]
struct LauncherCandidate {
    executable: PathBuf,
    label: Option<String>,
    declared_editor: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CodeProduct {
    application_name: Option<String>,
    name_long: Option<String>,
    name_short: Option<String>,
    remote_name: Option<String>,
    server_application_name: Option<String>,
    tunnel_application_name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JetBrainsProduct {
    name: String,
    product_code: String,
    #[serde(default)]
    launch: Vec<JetBrainsLaunch>,
    #[serde(default)]
    modules: Vec<JetBrainsModule>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JetBrainsLaunch {
    os: String,
    launcher_path: String,
    #[serde(default)]
    custom_commands: Vec<JetBrainsCommand>,
}

#[derive(Debug, Deserialize)]
struct JetBrainsCommand {
    #[serde(default)]
    commands: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum JetBrainsModule {
    Name(String),
    Entry { name: String },
}

impl JetBrainsModule {
    fn name(&self) -> &str {
        match self {
            Self::Name(name) | Self::Entry { name } => name,
        }
    }
}

pub fn detect_editors() -> Vec<DetectedEditor> {
    let candidates = discover_launcher_candidates();
    let transport = candidates
        .iter()
        .find(|candidate| is_remote_transport(&candidate.executable))
        .map(|candidate| candidate.executable.clone());
    let mut seen = HashSet::new();
    let mut editors = candidates
        .iter()
        .filter_map(|candidate| inspect_editor(candidate, transport.clone()))
        .filter(|editor| seen.insert(editor.id.clone()))
        .collect::<Vec<_>>();
    editors.sort_by_cached_key(|editor| editor.label.to_ascii_lowercase());
    editors
}

pub fn filtered_editors(
    editors: &[DetectedEditor],
    query: &str,
    project: &Project,
) -> Vec<DetectedEditor> {
    let query = query.trim().to_ascii_lowercase();
    editors
        .iter()
        .filter(|editor| !project.source.is_remote() || editor.supports_remote())
        .filter(|editor| editor.supports_project(project))
        .filter(|editor| {
            query.is_empty()
                || editor.label.to_ascii_lowercase().contains(&query)
                || editor.id.contains(&query)
        })
        .cloned()
        .collect()
}

fn inspect_editor(
    candidate: &LauncherCandidate,
    transport: Option<PathBuf>,
) -> Option<DetectedEditor> {
    if is_first_party_zed(&candidate.executable) || is_remote_transport(&candidate.executable) {
        return None;
    }

    if let Some((root, product)) = find_jetbrains_product(&candidate.executable) {
        let launch = product
            .launch
            .iter()
            .find(|launch| launch.os.eq_ignore_ascii_case(platform_name()))?;
        let executable = root.join(&launch.launcher_path);
        if !executable.is_file() {
            return None;
        }
        let remote = launch.custom_commands.iter().any(|command| {
            command
                .commands
                .iter()
                .any(|name| name.eq_ignore_ascii_case("thinClient"))
        });
        let project_kinds = jetbrains_project_kinds(&product.modules);
        return Some(DetectedEditor {
            id: editor_id(&executable),
            label: product.name,
            executable,
            kind: EditorKind::JetBrains {
                product_code: product.product_code,
                remote,
                transport,
                project_kinds,
            },
        });
    }

    if let Some(product) = find_code_product(&candidate.executable) {
        let remote = code_supports_remote(&product);
        let application_name = product.application_name.unwrap_or_default();
        if application_name.eq_ignore_ascii_case("zed") {
            return None;
        }
        let label = product
            .name_long
            .or(product.name_short)
            .or(candidate.label.clone())
            .unwrap_or_else(|| application_name.clone());
        if label.trim().is_empty() {
            return None;
        }
        return Some(DetectedEditor {
            id: editor_id(&candidate.executable),
            label,
            executable: candidate.executable.clone(),
            kind: EditorKind::Code { remote },
        });
    }

    candidate.declared_editor.then(|| DetectedEditor {
        id: editor_id(&candidate.executable),
        label: candidate.label.clone().unwrap_or_else(|| {
            candidate
                .executable
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        }),
        executable: candidate.executable.clone(),
        kind: EditorKind::Local,
    })
}

fn code_supports_remote(product: &CodeProduct) -> bool {
    product.remote_name.is_some()
        || product.server_application_name.is_some()
        || product.tunnel_application_name.is_some()
}

fn jetbrains_project_kinds(modules: &[JetBrainsModule]) -> Vec<EditorProjectKind> {
    let has = |module: &str| modules.iter().any(|candidate| candidate.name() == module);

    // Product-specific modules take precedence over optional bundled language support.
    if has("com.intellij.modules.rustrover") {
        return vec![EditorProjectKind::Rust];
    }
    if has("com.intellij.modules.webstorm") {
        return vec![EditorProjectKind::Web];
    }
    if has("com.intellij.modules.clion") {
        return vec![EditorProjectKind::Native];
    }
    if has("com.intellij.modules.rider") {
        return vec![EditorProjectKind::DotNet];
    }
    if has("com.intellij.modules.goland") || has("com.intellij.modules.go") {
        return vec![EditorProjectKind::Go];
    }
    if has("com.intellij.modules.pycharm") || has("com.intellij.modules.python") {
        return vec![EditorProjectKind::Python];
    }
    if has("com.intellij.modules.phpstorm") || has("com.intellij.modules.php") {
        return vec![EditorProjectKind::Php];
    }
    if has("com.intellij.modules.rubymine") || has("com.intellij.modules.ruby") {
        return vec![EditorProjectKind::Ruby];
    }
    if has("com.intellij.modules.appcode") || has("com.intellij.modules.swift") {
        return vec![EditorProjectKind::Swift];
    }

    let mut project_kinds = Vec::new();
    if has("com.intellij.modules.java") {
        project_kinds.push(EditorProjectKind::Java);
    }
    if has("com.intellij.modules.javascript") {
        project_kinds.push(EditorProjectKind::Web);
    }
    if has("com.intellij.modules.dart") {
        project_kinds.push(EditorProjectKind::Dart);
    }
    project_kinds
}

fn effective_project_kinds(project: &Project) -> Vec<EditorProjectKind> {
    let mut kinds = Vec::new();
    let primary = match project.project_type {
        ProjectType::Rust => Some(EditorProjectKind::Rust),
        ProjectType::Node => Some(EditorProjectKind::Web),
        ProjectType::Go => Some(EditorProjectKind::Go),
        ProjectType::Python => Some(EditorProjectKind::Python),
        ProjectType::Make | ProjectType::CMake | ProjectType::Assembly => {
            Some(EditorProjectKind::Native)
        }
        ProjectType::DotNet => Some(EditorProjectKind::DotNet),
        ProjectType::Java => Some(EditorProjectKind::Java),
        ProjectType::Unknown => None,
    };
    if let Some(primary) = primary {
        kinds.push(primary);
    }

    for marker in &project.markers_found {
        let project_kind = match marker.as_str() {
            "Cargo.toml" => Some(EditorProjectKind::Rust),
            "package.json" | "*.html" | "*.js" | "*.css" | "*.ts" | "*.tsx" | "*.jsx" | "*.vue" => {
                Some(EditorProjectKind::Web)
            }
            "go.mod" => Some(EditorProjectKind::Go),
            "pyproject.toml" | "requirements.txt" | "*.py" => Some(EditorProjectKind::Python),
            "Makefile" | "CMakeLists.txt" | "*.asm" => Some(EditorProjectKind::Native),
            "*.sln" => Some(EditorProjectKind::DotNet),
            "build.gradle" | "pom.xml" | "*.kt" => Some(EditorProjectKind::Java),
            "*.php" => Some(EditorProjectKind::Php),
            "*.rb" => Some(EditorProjectKind::Ruby),
            "*.swift" => Some(EditorProjectKind::Swift),
            "*.dart" => Some(EditorProjectKind::Dart),
            _ => None,
        };
        if let Some(project_kind) = project_kind {
            if !kinds.contains(&project_kind) {
                kinds.push(project_kind);
            }
        }
    }
    kinds
}

fn find_code_product(executable: &Path) -> Option<CodeProduct> {
    executable.ancestors().take(5).find_map(|directory| {
        read_json::<CodeProduct>(&directory.join("resources/app/product.json"))
            .or_else(|| read_json::<CodeProduct>(&directory.join("Resources/app/product.json")))
    })
}

fn find_jetbrains_product(executable: &Path) -> Option<(PathBuf, JetBrainsProduct)> {
    executable.ancestors().take(5).find_map(|directory| {
        read_json::<JetBrainsProduct>(&directory.join("product-info.json"))
            .map(|product| (directory.to_path_buf(), product))
            .or_else(|| {
                read_json::<JetBrainsProduct>(&directory.join("Resources/product-info.json"))
                    .map(|product| (directory.to_path_buf(), product))
            })
    })
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Option<T> {
    serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
}

fn editor_id(path: &Path) -> String {
    let id = path.to_string_lossy().replace('\\', "/");
    if cfg!(windows) {
        id.to_ascii_lowercase()
    } else {
        id
    }
}

fn is_first_party_zed(path: &Path) -> bool {
    path.file_stem()
        .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case("zed"))
}

fn is_remote_transport(path: &Path) -> bool {
    path.ancestors().take(4).any(|directory| {
        directory.join(".appState.json").is_file() && directory.join("plugins").join("ssh").is_dir()
    })
}

fn normalized_remote_path(path: &Path) -> String {
    let path = path.to_string_lossy().replace('\\', "/");
    if path.starts_with('/') {
        path
    } else {
        format!("/{path}")
    }
}

fn jetbrains_remote_uri(host: &str, path: &Path, product_code: &str) -> String {
    let (user, address) = host
        .rsplit_once('@')
        .map_or((None, host), |(user, address)| (Some(user), address));
    let (hostname, port) = split_host_port(address);
    let mut uri = format!(
        "jetbrains://gateway/ssh/environment?h={}&p={}&launchIde=true&projectHint={}&ideHint={}",
        encode_query_value(hostname),
        port,
        encode_query_value(&normalized_remote_path(path)),
        encode_query_value(product_code),
    );
    if let Some(user) = user {
        uri.push_str("&u=");
        uri.push_str(&encode_query_value(user));
    }
    uri
}

fn split_host_port(address: &str) -> (&str, u16) {
    if let Some(bracketed) = address.strip_prefix('[') {
        if let Some((host, port)) = bracketed.split_once("]:") {
            return (host, port.parse().unwrap_or(22));
        }
        return (bracketed.strip_suffix(']').unwrap_or(bracketed), 22);
    }
    address
        .rsplit_once(':')
        .filter(|(_, port)| port.chars().all(|character| character.is_ascii_digit()))
        .map_or((address, 22), |(host, port)| {
            (host, port.parse().unwrap_or(22))
        })
}

fn encode_query_value(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (byte as char).to_string()
            }
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

fn platform_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "Windows"
    } else if cfg!(target_os = "macos") {
        "macOS"
    } else {
        "Linux"
    }
}

fn path_candidates() -> Vec<LauncherCandidate> {
    let mut candidates = Vec::new();
    for directory in env::var_os("PATH").iter().flat_map(env::split_paths) {
        let Ok(entries) = fs::read_dir(directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let extension = path
                .extension()
                .map(|extension| extension.to_string_lossy().to_ascii_lowercase());
            let executable = if cfg!(windows) {
                matches!(extension.as_deref(), Some("exe" | "cmd" | "bat"))
            } else {
                path.is_file()
            };
            if executable {
                candidates.push(LauncherCandidate {
                    executable: path,
                    label: None,
                    declared_editor: false,
                });
            }
        }
    }
    candidates
}

#[cfg(target_os = "windows")]
fn discover_launcher_candidates() -> Vec<LauncherCandidate> {
    use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};

    let initialized = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok() };
    let mut candidates = path_candidates();
    for root in windows_shortcut_roots() {
        collect_windows_shortcuts(&root, 0, &mut candidates);
    }
    if initialized {
        unsafe { CoUninitialize() };
    }
    candidates
}

#[cfg(target_os = "windows")]
fn windows_shortcut_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(app_data) = env::var_os("APPDATA") {
        roots.push(
            PathBuf::from(app_data)
                .join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs"),
        );
    }
    if let Some(program_data) = env::var_os("PROGRAMDATA") {
        roots.push(
            PathBuf::from(program_data)
                .join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs"),
        );
    }
    roots
}

#[cfg(target_os = "windows")]
fn collect_windows_shortcuts(
    directory: &Path,
    depth: usize,
    candidates: &mut Vec<LauncherCandidate>,
) {
    if depth > 8 {
        return;
    }
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_windows_shortcuts(&path, depth + 1, candidates);
        } else if path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("lnk"))
        {
            if let Some(executable) = windows_shortcut_target(&path) {
                candidates.push(LauncherCandidate {
                    executable,
                    label: path
                        .file_stem()
                        .map(|name| name.to_string_lossy().into_owned()),
                    declared_editor: false,
                });
            }
        }
    }
}

#[cfg(target_os = "windows")]
fn windows_shortcut_target(shortcut: &Path) -> Option<PathBuf> {
    use std::ptr::null_mut;
    use windows::core::{Interface, HSTRING};
    use windows::Win32::System::Com::{
        CoCreateInstance, IPersistFile, CLSCTX_INPROC_SERVER, STGM_READ,
    };
    use windows::Win32::UI::Shell::{IShellLinkW, ShellLink, SLGP_RAWPATH};

    unsafe {
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).ok()?;
        let persist: IPersistFile = link.cast().ok()?;
        persist
            .Load(&HSTRING::from(shortcut.as_os_str()), STGM_READ)
            .ok()?;
        let mut target = vec![0_u16; 32_768];
        link.GetPath(&mut target, null_mut(), SLGP_RAWPATH.0 as u32)
            .ok()?;
        let length = target.iter().position(|character| *character == 0)?;
        let path = PathBuf::from(String::from_utf16_lossy(&target[..length]));
        path.is_file().then_some(path)
    }
}

#[cfg(target_os = "macos")]
fn discover_launcher_candidates() -> Vec<LauncherCandidate> {
    let mut candidates = path_candidates();
    let mut roots = vec![PathBuf::from("/Applications")];
    if let Some(home) = env::var_os("HOME") {
        roots.push(PathBuf::from(home).join("Applications"));
    }
    for root in roots {
        let Ok(apps) = fs::read_dir(root) else {
            continue;
        };
        for app in apps.flatten().map(|entry| entry.path()) {
            let executables = app.join("Contents").join("MacOS");
            let Ok(entries) = fs::read_dir(executables) else {
                continue;
            };
            candidates.extend(entries.flatten().filter_map(|entry| {
                let executable = entry.path();
                executable.is_file().then(|| LauncherCandidate {
                    label: app
                        .file_stem()
                        .map(|name| name.to_string_lossy().into_owned()),
                    executable,
                    declared_editor: false,
                })
            }));
        }
    }
    candidates
}

#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
fn discover_launcher_candidates() -> Vec<LauncherCandidate> {
    let mut candidates = path_candidates();
    let mut roots = vec![PathBuf::from("/usr/share/applications")];
    if let Some(home) = env::var_os("HOME") {
        roots.push(PathBuf::from(home).join(".local/share/applications"));
    }
    for root in roots {
        let Ok(entries) = fs::read_dir(root) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path
                .extension()
                .is_none_or(|extension| extension != "desktop")
            {
                continue;
            }
            if let Some(candidate) = parse_desktop_entry(&path) {
                candidates.push(candidate);
            }
        }
    }
    candidates
}

#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
fn parse_desktop_entry(path: &Path) -> Option<LauncherCandidate> {
    let content = fs::read_to_string(path).ok()?;
    let name = desktop_value(&content, "Name")?.to_string();
    let categories = desktop_value(&content, "Categories").unwrap_or_default();
    let mime_types = desktop_value(&content, "MimeType").unwrap_or_default();
    let declared_editor = categories
        .split(';')
        .any(|category| matches!(category, "Development" | "TextEditor"))
        || mime_types
            .split(';')
            .any(|mime| matches!(mime, "text/plain" | "inode/directory"));
    if !declared_editor || name.eq_ignore_ascii_case("zed") {
        return None;
    }
    let command = desktop_value(&content, "Exec")?;
    let executable = desktop_executable(command)?;
    Some(LauncherCandidate {
        executable,
        label: Some(name),
        declared_editor,
    })
}

#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
fn desktop_value<'a>(content: &'a str, key: &str) -> Option<&'a str> {
    content.lines().find_map(|line| {
        let (candidate, value) = line.split_once('=')?;
        (candidate == key).then_some(value.trim())
    })
}

#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
fn desktop_executable(command: &str) -> Option<PathBuf> {
    let executable = command
        .trim()
        .strip_prefix('"')
        .and_then(|value| value.split_once('"').map(|(path, _)| path))
        .or_else(|| command.split_whitespace().next())?;
    let executable = PathBuf::from(executable);
    if executable.is_absolute() && executable.is_file() {
        return Some(executable);
    }
    env::var_os("PATH")
        .iter()
        .flat_map(env::split_paths)
        .map(|directory| directory.join(&executable))
        .find(|candidate| candidate.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;
    use devhub_core::ProjectType;

    fn project(source: ProjectSource, path: &str) -> Project {
        Project {
            name: "sample".into(),
            path: PathBuf::from(path),
            source,
            project_type: ProjectType::Rust,
            has_git: true,
            git_remote: None,
            markers_found: Vec::new(),
            last_modified: None,
            search_key: String::new(),
        }
    }

    fn project_with_type(project_type: ProjectType, markers: &[&str]) -> Project {
        let mut project = project(ProjectSource::Local, "/srv/project");
        project.project_type = project_type;
        project.markers_found = markers.iter().map(|marker| (*marker).to_string()).collect();
        project
    }

    fn editor(kind: EditorKind) -> DetectedEditor {
        DetectedEditor {
            id: "test".into(),
            label: "Test Editor".into(),
            executable: PathBuf::from("editor"),
            kind,
        }
    }

    #[test]
    fn code_remote_uses_the_discovered_ssh_contract() {
        let request = editor(EditorKind::Code { remote: true })
            .launch_request(&project(
                ProjectSource::Remote {
                    name: "build".into(),
                    host: "dev@example.com".into(),
                },
                "/srv/project",
            ))
            .unwrap();

        assert_eq!(
            request,
            EditorLaunchRequest {
                program: PathBuf::from("editor"),
                args: vec![
                    "--remote".into(),
                    "ssh-remote+dev@example.com".into(),
                    "/srv/project".into(),
                ],
            }
        );
    }

    #[test]
    fn local_only_editors_are_removed_from_remote_results() {
        let editors = vec![
            editor(EditorKind::Local),
            editor(EditorKind::Code { remote: true }),
        ];

        let remote = project(
            ProjectSource::Remote {
                name: "build".into(),
                host: "dev@example.com".into(),
            },
            "/srv/project",
        );
        let filtered = filtered_editors(&editors, "", &remote);

        assert_eq!(filtered.len(), 1);
        assert!(filtered[0].supports_remote());
    }

    #[test]
    fn jetbrains_link_routes_to_the_discovered_product() {
        assert_eq!(
            jetbrains_remote_uri("dev@example.com:2222", Path::new("/srv/my project"), "RR"),
            "jetbrains://gateway/ssh/environment?h=example.com&p=2222&launchIde=true&projectHint=%2Fsrv%2Fmy%20project&ideHint=RR&u=dev"
        );
    }

    #[test]
    fn code_metadata_declares_remote_capability() {
        let product: CodeProduct = serde_json::from_str(
            r#"{"applicationName":"sample","nameLong":"Sample Editor","serverApplicationName":"sample-server","tunnelApplicationName":"sample-tunnel"}"#,
        )
        .unwrap();

        assert_eq!(product.name_long.as_deref(), Some("Sample Editor"));
        assert!(code_supports_remote(&product));
    }

    #[test]
    fn primary_jetbrains_product_module_wins_over_bundled_languages() {
        let modules = [
            JetBrainsModule::Name("com.intellij.modules.javascript".into()),
            JetBrainsModule::Name("com.intellij.modules.python-core-capable".into()),
            JetBrainsModule::Name("com.intellij.modules.rustrover".into()),
        ];

        assert_eq!(jetbrains_project_kinds(&modules), [EditorProjectKind::Rust]);
    }

    #[test]
    fn jetbrains_entries_are_filtered_by_project_compatibility() {
        let rustrover = editor(EditorKind::JetBrains {
            product_code: "RR".into(),
            remote: true,
            transport: Some(PathBuf::from("toolbox")),
            project_kinds: vec![EditorProjectKind::Rust],
        });

        assert_eq!(
            filtered_editors(
                std::slice::from_ref(&rustrover),
                "",
                &project_with_type(ProjectType::Rust, &[]),
            )
            .len(),
            1
        );
        assert!(filtered_editors(
            std::slice::from_ref(&rustrover),
            "",
            &project_with_type(ProjectType::Node, &[]),
        )
        .is_empty());
        assert!(filtered_editors(
            &[rustrover],
            "",
            &project_with_type(ProjectType::Unknown, &["*.js"]),
        )
        .is_empty());
    }

    #[test]
    fn jetbrains_entries_consider_every_language_marker() {
        let rustrover = editor(EditorKind::JetBrains {
            product_code: "RR".into(),
            remote: true,
            transport: Some(PathBuf::from("toolbox")),
            project_kinds: vec![EditorProjectKind::Rust],
        });
        let webstorm = editor(EditorKind::JetBrains {
            product_code: "WS".into(),
            remote: true,
            transport: Some(PathBuf::from("toolbox")),
            project_kinds: vec![EditorProjectKind::Web],
        });
        let project = project_with_type(ProjectType::Rust, &["Cargo.toml", "package.json"]);

        let filtered = filtered_editors(&[rustrover, webstorm], "", &project);

        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn unclassified_jetbrains_entries_are_not_offered() {
        let editor = editor(EditorKind::JetBrains {
            product_code: "UNKNOWN".into(),
            remote: true,
            transport: Some(PathBuf::from("toolbox")),
            project_kinds: Vec::new(),
        });

        assert!(filtered_editors(
            &[editor],
            "",
            &project_with_type(ProjectType::Rust, &["Cargo.toml"]),
        )
        .is_empty());
    }

    #[test]
    fn discovered_entries_are_actual_editors() {
        let editors = detect_editors();
        let mut ids = HashSet::new();
        for editor in editors {
            assert!(!editor.label().trim().is_empty());
            assert!(!is_first_party_zed(&editor.executable));
            assert!(!is_remote_transport(&editor.executable));
            if editor.supports_remote() {
                assert!(editor
                    .launch_request(&project(
                        ProjectSource::Remote {
                            name: "build".into(),
                            host: "dev@example.com".into(),
                        },
                        "/srv/project",
                    ))
                    .is_ok());
            }
            assert!(ids.insert(editor.id));
        }
    }
}
