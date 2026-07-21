use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use crate::config::{normalize_ssh_host, RemoteHostConfig};
use crate::discovery::{sort_projects, Project, ProjectSource, ProjectType};
use crate::ssh::SshRunner;
use crate::workspace::{FileEntry, SearchHit, TreeListing};
use crate::CancellationToken;

const SSH_CONNECT_TIMEOUT_SECONDS: u64 = 8;
const SSH_OPERATION_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_REMOTE_OUTPUT_BYTES: usize = 2 * 1024 * 1024;
const MAX_FILE_BYTES: usize = 512 * 1024;
const MAX_TREE_ENTRIES: usize = 500;
const MAX_SEARCH_HITS: usize = 200;
const MAX_PREVIEW_CHARS: usize = 240;
const MAX_REMOTE_PROJECTS: usize = 1_000;
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

const REMOTE_IGNORE_FUNCTION: &str = r#"is_git_ignored() {
    candidate="$1"
    command -v git >/dev/null 2>&1 || return 1
    cursor="$(dirname "$candidate")"
    while [ "$cursor" != "/" ] && [ -n "$cursor" ]; do
        if [ -e "$cursor/.git" ]; then
            case "$candidate" in
                "$cursor"/*) relative=${candidate#"$cursor"/} ;;
                *) return 1 ;;
            esac
            git -C "$cursor" check-ignore -q -- "$relative" 2>/dev/null
            return $?
        fi
        parent="$(dirname "$cursor")"
        [ "$parent" = "$cursor" ] && break
        cursor="$parent"
    done
    return 1
}
"#;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryEntry {
    pub name: String,
    pub path: String,
}

pub fn validate_ssh_host(raw: &str) -> Result<String, String> {
    let host = normalize_ssh_host(raw);
    if host.is_empty() {
        return Err("SSH host cannot be empty".into());
    }
    if host.starts_with('-') {
        return Err("SSH host cannot begin with '-'".into());
    }
    if !host.chars().all(|character| {
        character.is_ascii_alphanumeric()
            || matches!(character, '.' | '-' | '_' | '@' | ':' | '[' | ']' | '%')
    }) {
        return Err(
            "SSH host may contain only letters, digits, '.', '-', '_', '@', ':', '[', ']', and '%'"
                .into(),
        );
    }
    Ok(host)
}

pub fn validate_remote_path(raw: &str) -> Result<String, String> {
    let path = raw.trim().replace('\\', "/");
    if path.is_empty() {
        return Err("remote path cannot be empty".into());
    }
    if path
        .bytes()
        .any(|byte| matches!(byte, 0 | b'\n' | b'\r' | b'\t'))
    {
        return Err("remote path cannot contain control characters".into());
    }
    Ok(path)
}

pub fn check_ssh_connection(host: &str) -> Result<(), String> {
    check_ssh_connection_cancellable(host, &CancellationToken::new())
}

pub fn check_ssh_connection_cancellable(
    host: &str,
    cancellation: &CancellationToken,
) -> Result<(), String> {
    let output = run_ssh_command(
        host,
        &["echo", "devhub-ssh-ok"],
        &[],
        Duration::from_secs(12),
        cancellation,
    )?;
    if output.trim() == "devhub-ssh-ok" {
        Ok(())
    } else {
        Err("SSH connection returned an unexpected response".into())
    }
}

pub fn open_project_in_zed(project: &Project) -> Result<(), String> {
    let target = match &project.source {
        ProjectSource::Local => project.path.as_os_str().to_os_string(),
        ProjectSource::Remote { .. } => zed_ssh_uri(project)?.into(),
    };

    if spawn_zed("zed", &target) {
        return Ok(());
    }

    #[cfg(windows)]
    for candidate in zed_windows_candidates() {
        if candidate.is_file() && spawn_zed(&candidate, &target) {
            return Ok(());
        }
    }

    Err("Zed was not found. Install Zed or use Open in... to choose another compatible application.".into())
}

fn spawn_zed(program: impl AsRef<std::ffi::OsStr>, target: &std::ffi::OsStr) -> bool {
    let mut command = Command::new(program);
    command
        .arg(target)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command.spawn().is_ok()
}

pub fn zed_ssh_uri(project: &Project) -> Result<String, String> {
    let ProjectSource::Remote { host, .. } = &project.source else {
        return Err("project is not remote".into());
    };
    let host = validate_ssh_host(host)?;
    Ok(format!("ssh://{host}{}", encode_remote_path(&project.path)))
}

fn encode_remote_path(path: &Path) -> String {
    let path = path.to_string_lossy().replace('\\', "/");
    let path = if path.starts_with('/') {
        path
    } else {
        format!("/{path}")
    };
    path.bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' => {
                (byte as char).to_string()
            }
            _ => format!("%{byte:02X}"),
        })
        .collect::<String>()
}

#[cfg(windows)]
fn zed_windows_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        let local = PathBuf::from(local);
        candidates.push(local.join("Programs").join("Zed").join("Zed.exe"));
        candidates.push(local.join("Zed").join("Zed.exe"));
    }
    if let Some(program_files) = std::env::var_os("ProgramFiles") {
        candidates.push(PathBuf::from(program_files).join("Zed").join("Zed.exe"));
    }
    candidates
}

pub fn list_remote_subdirs(host: &str, path: &str) -> Result<Vec<DirectoryEntry>, String> {
    list_remote_subdirs_cancellable(host, path, &CancellationToken::new())
}

pub fn list_remote_subdirs_cancellable(
    host: &str,
    path: &str,
    cancellation: &CancellationToken,
) -> Result<Vec<DirectoryEntry>, String> {
    let path = validate_remote_path(path)?;
    if path == "/" && remote_is_windows(host, cancellation) {
        return list_windows_remote_drives(host, cancellation);
    }
    let script = format!(
        "root={}\nfind \"$root\" -mindepth 1 -maxdepth 1 -type d -printf '%f\\t%p\\n' 2>/dev/null | head -n 500\n",
        shell_quote(&path)
    );
    let output =
        run_ssh_script_for_path(host, &path, &script, SSH_OPERATION_TIMEOUT, cancellation)?;
    let mut entries = output
        .lines()
        .filter_map(|line| {
            let (name, path) = line.split_once('\t')?;
            (!name.is_empty() && !path.is_empty()).then(|| DirectoryEntry {
                name: name.to_string(),
                path: path.to_string(),
            })
        })
        .collect::<Vec<_>>();
    entries.sort_by_cached_key(|entry| entry.name.to_lowercase());
    Ok(entries)
}

fn remote_is_windows(host: &str, cancellation: &CancellationToken) -> bool {
    run_powershell_command(
        host,
        "[Console]::Out.Write('devhub-windows')",
        Duration::from_secs(12),
        cancellation,
    )
    .is_ok_and(|output| output == "devhub-windows")
}

fn list_windows_remote_drives(
    host: &str,
    cancellation: &CancellationToken,
) -> Result<Vec<DirectoryEntry>, String> {
    let script = r#"Get-PSDrive -PSProvider FileSystem | Sort-Object Name | ForEach-Object {
    [Console]::Out.WriteLine("$($_.Name):`t$($_.Name):/")
}"#;
    let output = run_powershell_command(host, script, SSH_OPERATION_TIMEOUT, cancellation)?;
    Ok(output
        .lines()
        .filter_map(|line| {
            let (name, path) = line.split_once('\t')?;
            Some(DirectoryEntry {
                name: name.to_string(),
                path: path.to_string(),
            })
        })
        .collect())
}

pub fn scan_remote_host(config: &RemoteHostConfig) -> Result<Vec<Project>, String> {
    scan_remote_host_cancellable(config, &CancellationToken::new())
}

pub fn scan_remote_host_cancellable(
    config: &RemoteHostConfig,
    cancellation: &CancellationToken,
) -> Result<Vec<Project>, String> {
    let host = validate_ssh_host(&config.host)?;
    let roots = config
        .roots
        .iter()
        .map(|root| validate_remote_path(root))
        .collect::<Result<Vec<_>, _>>()?;
    if roots.is_empty() {
        return Ok(Vec::new());
    }

    let windows_remote = is_windows_remote_path(&roots[0]);
    if roots
        .iter()
        .any(|root| is_windows_remote_path(root) != windows_remote)
    {
        return Err("SSH roots for one host cannot mix Windows and POSIX paths".into());
    }
    let path_hint = roots[0].clone();

    let roots = roots
        .iter()
        .map(|root| shell_quote(root))
        .collect::<Vec<_>>()
        .join(" ");
    let script = format!(
        r#"{ignore}
emit_project() {{
    d="$1"
    markers=""
    ptype="Unknown"
    add_marker() {{
        [ -z "$markers" ] && ptype="$2"
        markers="${{markers}}$1,"
    }}
    [ -f "$d/Cargo.toml" ] && add_marker "Cargo.toml" "Rust"
    [ -f "$d/package.json" ] && add_marker "package.json" "Node"
    [ -f "$d/go.mod" ] && add_marker "go.mod" "Go"
    [ -f "$d/pyproject.toml" ] && add_marker "pyproject.toml" "Python"
    [ -f "$d/requirements.txt" ] && add_marker "requirements.txt" "Python"
    [ -f "$d/Makefile" ] && add_marker "Makefile" "Make"
    [ -f "$d/CMakeLists.txt" ] && add_marker "CMakeLists.txt" "CMake"
    [ -f "$d/build.gradle" ] && add_marker "build.gradle" "Java"
    [ -f "$d/pom.xml" ] && add_marker "pom.xml" "Java"
    set -- "$d"/*.asm; [ -e "$1" ] && add_marker "*.asm" "ASM"
    set -- "$d"/*.sln; [ -e "$1" ] && add_marker "*.sln" ".NET"
    has_git=false
    remote=""
    if [ -d "$d/.git" ]; then
        has_git=true
        [ -z "$markers" ] && markers=".git,"
        remote="$(git -C "$d" config --get remote.origin.url 2>/dev/null || true)"
    fi
    [ -z "$markers" ] && return
    modified="$(stat -c %Y "$d" 2>/dev/null || echo 0)"
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$(basename "$d")" "$d" "$ptype" "$has_git" "$remote" "$markers" "$modified"
}}

for root in {roots}; do
    [ -d "$root" ] || continue
    find "$root" -maxdepth {depth} \( -name .git -o -name node_modules -o -name target -o -name build -o -name dist -o -name .next -o -name vendor -o -name __pycache__ \) -prune -o -type d -print 2>/dev/null
done | head -n 5000 | while IFS= read -r dir; do
    is_git_ignored "$dir" || emit_project "$dir"
done | head -n {limit}
"#,
        ignore = REMOTE_IGNORE_FUNCTION,
        depth = config.max_depth.clamp(1, 20),
        limit = MAX_REMOTE_PROJECTS,
    );

    let output = run_ssh_script_for_path(
        &host,
        &path_hint,
        &script,
        SSH_OPERATION_TIMEOUT,
        cancellation,
    )?;
    let mut projects = output
        .lines()
        .filter_map(|line| parse_remote_project(line, config, &host))
        .collect::<Vec<_>>();
    sort_projects(&mut projects);
    Ok(projects)
}

pub fn list_remote_tree(
    host: &str,
    root: &Path,
    max_depth: usize,
    show_hidden: bool,
) -> Result<TreeListing, String> {
    list_remote_tree_cancellable(
        host,
        root,
        max_depth,
        show_hidden,
        &CancellationToken::new(),
    )
}

pub fn list_remote_tree_cancellable(
    host: &str,
    root: &Path,
    max_depth: usize,
    show_hidden: bool,
    cancellation: &CancellationToken,
) -> Result<TreeListing, String> {
    let root = validate_remote_path(&root.to_string_lossy())?;
    let hidden_prune = if show_hidden {
        String::new()
    } else {
        " -o -name '.*'".to_string()
    };
    let script = format!(
        r#"{ignore}
root={root}
find "$root" -mindepth 1 -maxdepth {depth} \( -name .git -o -name node_modules -o -name target -o -name build -o -name dist -o -name .next -o -name vendor -o -name __pycache__{hidden_prune} \) -prune -o \( -type d -printf 'd\t%p\n' -o -type f -printf 'f\t%p\n' \) 2>/dev/null | while IFS="$(printf '\t')" read -r kind path; do
    is_git_ignored "$path" || printf '%s\t%s\n' "$kind" "$path"
done | head -n {limit}
"#,
        ignore = REMOTE_IGNORE_FUNCTION,
        root = shell_quote(&root),
        depth = max_depth.clamp(1, 20),
        limit = MAX_TREE_ENTRIES + 1,
    );
    let output =
        run_ssh_script_for_path(host, &root, &script, SSH_OPERATION_TIMEOUT, cancellation)?;
    let entries = output
        .lines()
        .take(MAX_TREE_ENTRIES)
        .filter_map(|line| parse_tree_entry(line, &root))
        .collect::<Vec<_>>();
    Ok(TreeListing {
        truncated: output.lines().count() > MAX_TREE_ENTRIES,
        entries: order_tree_entries(Path::new(&root), entries),
        warnings: Vec::new(),
    })
}

pub fn read_remote_file(host: &str, root: &Path, path: &Path) -> Result<String, String> {
    read_remote_file_cancellable(host, root, path, &CancellationToken::new())
}

pub fn read_remote_file_cancellable(
    host: &str,
    root: &Path,
    path: &Path,
    cancellation: &CancellationToken,
) -> Result<String, String> {
    let root = validate_remote_path(&root.to_string_lossy())?;
    let path = validate_remote_path(&path.to_string_lossy())?;
    let script = format!(
        r#"root={root}
path={path}
[ -f "$path" ] || {{ printf 'not a file' >&2; exit 2; }}
canonical() {{
    if command -v realpath >/dev/null 2>&1; then realpath -- "$1"; else readlink -f -- "$1"; fi
}}
root_real="$(canonical "$root")" || exit 3
path_real="$(canonical "$path")" || exit 3
case "$path_real" in
    "$root_real"/*) ;;
    *) printf 'path resolves outside the project root' >&2; exit 5 ;;
esac
size="$(wc -c < "$path" 2>/dev/null)" || exit 3
[ "$size" -le {limit} ] || {{ printf 'file is larger than {kib} KiB' >&2; exit 4; }}
cat -- "$path"
"#,
        root = shell_quote(&root),
        path = shell_quote(&path),
        limit = MAX_FILE_BYTES,
        kib = MAX_FILE_BYTES / 1024,
    );
    decode_remote_text(
        run_ssh_script_for_path(host, &path, &script, SSH_OPERATION_TIMEOUT, cancellation)?
            .into_bytes(),
    )
}

pub fn read_remote_readme(host: &str, root: &Path) -> Result<Option<String>, String> {
    read_remote_readme_cancellable(host, root, &CancellationToken::new())
}

pub fn read_remote_readme_cancellable(
    host: &str,
    root: &Path,
    cancellation: &CancellationToken,
) -> Result<Option<String>, String> {
    let root = validate_remote_path(&root.to_string_lossy())?;
    let script = format!(
        r#"root={root}
for name in README.md README.txt README Readme.md readme.md; do
    path="$root/$name"
    if [ -f "$path" ]; then
        size="$(wc -c < "$path" 2>/dev/null)" || exit 3
        [ "$size" -le {limit} ] || exit 0
        cat -- "$path"
        exit 0
    fi
done
"#,
        root = shell_quote(&root),
        limit = MAX_FILE_BYTES,
    );
    let bytes = run_ssh_script_for_path(host, &root, &script, SSH_OPERATION_TIMEOUT, cancellation)?
        .into_bytes();
    if bytes.is_empty() {
        Ok(None)
    } else {
        decode_remote_text(bytes).map(Some)
    }
}

pub fn search_remote_content(
    host: &str,
    root: &Path,
    query: &str,
) -> Result<Vec<SearchHit>, String> {
    search_remote_content_cancellable(host, root, query, &CancellationToken::new())
}

pub fn search_remote_content_cancellable(
    host: &str,
    root: &Path,
    query: &str,
    cancellation: &CancellationToken,
) -> Result<Vec<SearchHit>, String> {
    let root = validate_remote_path(&root.to_string_lossy())?;
    let query = query.trim();
    if query.is_empty() {
        return Ok(Vec::new());
    }
    if query.bytes().any(|byte| matches!(byte, 0 | b'\n' | b'\r')) {
        return Err("search query cannot contain line breaks".into());
    }
    let script = format!(
        r#"{ignore}
root={root}
query={query}
grep -RInF --exclude-dir=.git --exclude-dir=node_modules --exclude-dir=target --exclude-dir=build --exclude-dir=dist --exclude-dir=.next --exclude-dir=vendor --exclude-dir=__pycache__ --binary-files=without-match -- "$query" "$root" 2>/dev/null | while IFS= read -r match; do
    path=${{match%%:*}}
    is_git_ignored "$path" || printf '%s\n' "$match"
done | head -n {limit} || true
"#,
        ignore = REMOTE_IGNORE_FUNCTION,
        root = shell_quote(&root),
        query = shell_quote(query),
        limit = MAX_SEARCH_HITS,
    );
    let output =
        run_ssh_script_for_path(host, &root, &script, SSH_OPERATION_TIMEOUT, cancellation)?;
    Ok(output.lines().filter_map(parse_grep_hit).collect())
}

fn run_ssh_script_for_path(
    host: &str,
    path: &str,
    script: &str,
    timeout: Duration,
    cancellation: &CancellationToken,
) -> Result<String, String> {
    let bytes = if is_windows_remote_path(path) {
        run_windows_ssh_script_bytes(host, script, timeout, cancellation)?
    } else {
        run_ssh_script_bytes(host, script, timeout, cancellation)?
    };
    String::from_utf8(bytes).map_err(|_| "SSH output is not valid UTF-8".into())
}

pub(crate) fn run_ssh_script_bytes(
    host: &str,
    script: &str,
    timeout: Duration,
    cancellation: &CancellationToken,
) -> Result<Vec<u8>, String> {
    run_ssh_command_bytes(
        host,
        &["sh", "-s"],
        script.as_bytes(),
        timeout,
        cancellation,
    )
}

pub(crate) fn run_windows_ssh_script_bytes(
    host: &str,
    script: &str,
    timeout: Duration,
    cancellation: &CancellationToken,
) -> Result<Vec<u8>, String> {
    let bootstrap = r#"$candidates = @(
    (Get-Command sh.exe -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source -First 1),
    "$env:ProgramFiles\Git\bin\sh.exe",
    "${env:ProgramFiles(x86)}\Git\bin\sh.exe"
) | Where-Object { $_ -and (Test-Path -LiteralPath $_) }
$sh = $candidates | Select-Object -First 1
if (-not $sh) {
    [Console]::Error.WriteLine('Git for Windows shell was not found on the SSH host')
    exit 127
}
$process = Start-Process -FilePath $sh -ArgumentList '-s' -NoNewWindow -Wait -PassThru
exit $process.ExitCode
"#;
    let encoded = encode_powershell_command(bootstrap);
    run_ssh_command_bytes(
        host,
        &[
            "powershell.exe",
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-EncodedCommand",
            &encoded,
        ],
        script.as_bytes(),
        timeout,
        cancellation,
    )
}

fn run_ssh_command(
    host: &str,
    remote_command: &[&str],
    input: &[u8],
    timeout: Duration,
    cancellation: &CancellationToken,
) -> Result<String, String> {
    let bytes = run_ssh_command_bytes(host, remote_command, input, timeout, cancellation)?;
    String::from_utf8(bytes).map_err(|_| "SSH output is not valid UTF-8".into())
}

fn run_powershell_command(
    host: &str,
    script: &str,
    timeout: Duration,
    cancellation: &CancellationToken,
) -> Result<String, String> {
    let encoded = encode_powershell_command(script);
    run_ssh_command(
        host,
        &[
            "powershell.exe",
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-EncodedCommand",
            &encoded,
        ],
        &[],
        timeout,
        cancellation,
    )
}

fn run_ssh_command_bytes(
    host: &str,
    remote_command: &[&str],
    input: &[u8],
    timeout: Duration,
    cancellation: &CancellationToken,
) -> Result<Vec<u8>, String> {
    cancellation.check()?;
    let host = validate_ssh_host(host)?;
    let mut cmd = Command::new("ssh");
    cmd.arg("-T")
        .arg("-o")
        .arg("BatchMode=yes")
        .arg("-o")
        .arg(format!("ConnectTimeout={SSH_CONNECT_TIMEOUT_SECONDS}"))
        .arg("-o")
        .arg("ConnectionAttempts=1")
        .arg(&host)
        .args(remote_command);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let output = SshRunner::new(&host, timeout, MAX_REMOTE_OUTPUT_BYTES, cancellation)
        .run(cmd, input)
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = stderr.trim();
        return Err(if detail.is_empty() {
            format!(
                "SSH operation failed for {host} with status {}",
                output.status
            )
        } else {
            format!("SSH operation failed for {host}: {detail}")
        });
    }
    Ok(output.stdout)
}

pub fn is_windows_remote_path(path: &str) -> bool {
    let path = path.as_bytes();
    (path.len() >= 3 && path[0].is_ascii_alphabetic() && path[1] == b':' && path[2] == b'/')
        || path.starts_with(b"//")
}

pub fn powershell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

pub fn encode_powershell_command(command: &str) -> String {
    let bytes = command
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>();
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut encoded = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let value = u32::from(chunk[0]) << 16
            | u32::from(*chunk.get(1).unwrap_or(&0)) << 8
            | u32::from(*chunk.get(2).unwrap_or(&0));
        encoded.push(ALPHABET[((value >> 18) & 0x3f) as usize] as char);
        encoded.push(ALPHABET[((value >> 12) & 0x3f) as usize] as char);
        encoded.push(if chunk.len() > 1 {
            ALPHABET[((value >> 6) & 0x3f) as usize] as char
        } else {
            '='
        });
        encoded.push(if chunk.len() > 2 {
            ALPHABET[(value & 0x3f) as usize] as char
        } else {
            '='
        });
    }
    encoded
}

fn parse_remote_project(line: &str, config: &RemoteHostConfig, host: &str) -> Option<Project> {
    let mut fields = line.split('\t');
    let name = fields.next()?.to_string();
    let path = PathBuf::from(fields.next()?);
    let project_type = project_type_from_label(fields.next()?);
    let has_git = fields.next()? == "true";
    let git_remote = fields
        .next()
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let markers_found = fields
        .next()?
        .split(',')
        .filter(|marker| !marker.is_empty())
        .map(str::to_string)
        .collect();
    let last_modified = fields
        .next()
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0);
    let mut project = Project {
        name,
        path,
        source: ProjectSource::Remote {
            name: config.label().to_string(),
            host: host.to_string(),
        },
        project_type,
        has_git,
        git_remote,
        markers_found,
        last_modified,
        search_key: String::new(),
    };
    project.refresh_search_key();
    Some(project)
}

fn parse_tree_entry(line: &str, root: &str) -> Option<FileEntry> {
    let (kind, raw_path) = line.split_once('\t')?;
    let relative = raw_path
        .strip_prefix(root)
        .unwrap_or(raw_path)
        .trim_start_matches('/');
    if relative.is_empty() {
        return None;
    }
    Some(FileEntry {
        name: relative.rsplit('/').next()?.to_string(),
        path: PathBuf::from(raw_path),
        depth: relative.split('/').count().saturating_sub(1),
        is_dir: kind == "d",
    })
}

fn order_tree_entries(root: &Path, entries: Vec<FileEntry>) -> Vec<FileEntry> {
    let mut children = HashMap::<PathBuf, Vec<FileEntry>>::new();
    for entry in entries {
        let parent = entry.path.parent().unwrap_or(root).to_path_buf();
        children.entry(parent).or_default().push(entry);
    }

    fn append(
        parent: &Path,
        children: &mut HashMap<PathBuf, Vec<FileEntry>>,
        output: &mut Vec<FileEntry>,
    ) {
        let Some(mut siblings) = children.remove(parent) else {
            return;
        };
        siblings.sort_by(|left, right| {
            right
                .is_dir
                .cmp(&left.is_dir)
                .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
                .then_with(|| left.name.cmp(&right.name))
        });
        for entry in siblings {
            let path = entry.path.clone();
            let is_dir = entry.is_dir;
            output.push(entry);
            if is_dir {
                append(&path, children, output);
            }
        }
    }

    let mut output = Vec::new();
    append(root, &mut children, &mut output);
    output
}

fn parse_grep_hit(line: &str) -> Option<SearchHit> {
    line.match_indices(':').find_map(|(separator, _)| {
        let (line_number, preview) = line[separator + 1..].split_once(':')?;
        let line_number = line_number.parse().ok()?;
        Some(SearchHit {
            path: PathBuf::from(&line[..separator]),
            line: line_number,
            preview: preview.trim().chars().take(MAX_PREVIEW_CHARS).collect(),
        })
    })
}

fn decode_remote_text(bytes: Vec<u8>) -> Result<String, String> {
    if bytes.contains(&0) {
        return Err("binary file preview is not supported".into());
    }
    String::from_utf8(bytes).map_err(|_| "file is not valid UTF-8 text".into())
}

fn project_type_from_label(label: &str) -> ProjectType {
    match label {
        "Rust" => ProjectType::Rust,
        "Node" => ProjectType::Node,
        "Go" => ProjectType::Go,
        "Python" => ProjectType::Python,
        "Make" => ProjectType::Make,
        "CMake" => ProjectType::CMake,
        "ASM" => ProjectType::Assembly,
        ".NET" => ProjectType::DotNet,
        "Java" => ProjectType::Java,
        _ => ProjectType::Unknown,
    }
}

pub fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ssh_host_validation_rejects_options_and_shell_fragments() {
        assert_eq!(
            validate_ssh_host(" ssh user@example.com ").unwrap(),
            "user@example.com"
        );
        assert!(validate_ssh_host("-Fbad").is_err());
        assert!(validate_ssh_host("host; command").is_err());
        assert!(validate_ssh_host("host name").is_err());
    }

    #[test]
    fn remote_paths_reject_record_separators() {
        assert_eq!(validate_remote_path(r" /srv\code ").unwrap(), "/srv/code");
        assert!(validate_remote_path("/srv\nbad").is_err());
        assert!(validate_remote_path("\t").is_err());
    }

    #[test]
    fn parsers_preserve_remote_project_and_hit_metadata() {
        let config = RemoteHostConfig {
            name: "build".into(),
            host: "dev@example.com".into(),
            roots: vec!["/srv".into()],
            max_depth: 3,
        };
        let project = parse_remote_project(
            "demo\t/srv/demo\tRust\ttrue\tgit@example.com:demo.git\tCargo.toml,\t42",
            &config,
            &config.host,
        )
        .unwrap();
        assert!(project.source.is_remote());
        assert_eq!(project.project_type, ProjectType::Rust);
        assert_eq!(project.last_modified, Some(42));

        let hit = parse_grep_hit("/srv/demo/src/main.rs:7:println!(\"hello\")").unwrap();
        assert_eq!(hit.line, 7);
        assert!(hit.path.ends_with("main.rs"));

        let windows_hit =
            parse_grep_hit("C:/dev/demo/src/main.rs:9:println!(\"windows\")").unwrap();
        assert_eq!(windows_hit.line, 9);
        assert_eq!(windows_hit.path, PathBuf::from("C:/dev/demo/src/main.rs"));
    }

    #[test]
    fn shell_quote_handles_single_quotes() {
        assert_eq!(shell_quote("/srv/user's code"), "'/srv/user'\\''s code'");
    }

    #[test]
    fn remote_path_style_distinguishes_windows_and_posix() {
        assert!(is_windows_remote_path("C:/Users/dev/project"));
        assert!(is_windows_remote_path("//server/share/project"));
        assert!(!is_windows_remote_path("/srv/project"));
        assert!(!is_windows_remote_path("relative/project"));
        assert_eq!(
            powershell_quote("C:/User's/project"),
            "'C:/User''s/project'"
        );
        assert_eq!(encode_powershell_command("hi"), "aABpAA==");
    }

    #[test]
    fn remote_tree_order_keeps_children_below_their_parent() {
        let root = Path::new("/srv/project");
        let entries = vec![
            FileEntry {
                name: "z.txt".into(),
                path: root.join("z.txt"),
                depth: 0,
                is_dir: false,
            },
            FileEntry {
                name: "main.rs".into(),
                path: root.join("src/main.rs"),
                depth: 1,
                is_dir: false,
            },
            FileEntry {
                name: "src".into(),
                path: root.join("src"),
                depth: 0,
                is_dir: true,
            },
        ];

        let ordered = order_tree_entries(root, entries);
        assert_eq!(
            ordered
                .iter()
                .map(|entry| entry.name.as_str())
                .collect::<Vec<_>>(),
            ["src", "main.rs", "z.txt"]
        );
    }

    #[test]
    fn zed_uri_encodes_remote_paths() {
        let project = Project {
            name: "fixture".into(),
            path: PathBuf::from("/srv/my project/#demo"),
            source: ProjectSource::Remote {
                name: "example".into(),
                host: "dev@example.com".into(),
            },
            project_type: ProjectType::Rust,
            has_git: true,
            git_remote: None,
            markers_found: vec!["Cargo.toml".into()],
            last_modified: None,
            search_key: String::new(),
        };
        assert_eq!(
            zed_ssh_uri(&project).unwrap(),
            "ssh://dev@example.com/srv/my%20project/%23demo"
        );

        let mut port_project = project;
        port_project.source = ProjectSource::Remote {
            name: "example".into(),
            host: "dev@example.com:2222".into(),
        };
        assert_eq!(
            zed_ssh_uri(&port_project).unwrap(),
            "ssh://dev@example.com:2222/srv/my%20project/%23demo"
        );
    }

    #[test]
    fn cancelled_ssh_operation_returns_before_spawning_a_process() {
        let token = CancellationToken::new();
        token.cancel();

        let error = run_ssh_script_bytes(
            "unreachable.invalid",
            "exit 0",
            Duration::from_secs(1),
            &token,
        )
        .unwrap_err();

        assert_eq!(error, crate::OPERATION_CANCELLED);
    }

    #[test]
    fn remote_ignore_filter_uses_git_check_ignore_and_handles_nested_paths() {
        assert!(REMOTE_IGNORE_FUNCTION.contains("git -C \"$cursor\" check-ignore"));
        assert!(REMOTE_IGNORE_FUNCTION.contains("relative=${candidate#\"$cursor\"/}"));
        assert!(REMOTE_IGNORE_FUNCTION.contains("command -v git"));
        assert!(REMOTE_IGNORE_FUNCTION.contains("cursor=\"$(dirname \"$candidate\")\""));
    }
}
