use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use crate::discovery::{Project, ProjectSource};
use crate::remote::{run_ssh_script_bytes, shell_quote};
use crate::ssh::SshRunner;
use crate::workspace::read_project_file_cancellable;
use crate::{CancellationToken, OPERATION_CANCELLED};

const GIT_OPERATION_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_GIT_OUTPUT_BYTES: usize = 4 * 1024 * 1024;
const MAX_LINE_STAT_FILE_BYTES: u64 = 512 * 1024;
const MAX_LINE_STAT_TOTAL_BYTES: u64 = 4 * 1024 * 1024;
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitErrorKind {
    Cancelled,
    TimedOut,
    GitUnavailable,
    NotRepository,
    NetworkUnavailable,
    RemoteUnavailable,
    Validation,
    CommandFailed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitError {
    pub kind: GitErrorKind,
    pub detail: String,
}

impl GitError {
    fn new(kind: GitErrorKind, detail: impl Into<String>) -> Self {
        Self {
            kind,
            detail: detail.into(),
        }
    }

    pub fn status_text(&self) -> &str {
        match self.kind {
            GitErrorKind::Cancelled => "Git operation cancelled.",
            GitErrorKind::TimedOut => "Git operation timed out.",
            GitErrorKind::GitUnavailable => "Git is unavailable.",
            GitErrorKind::NotRepository => "This project is not a Git repository.",
            GitErrorKind::NetworkUnavailable => "Network unavailable.",
            GitErrorKind::RemoteUnavailable => "Remote project unavailable.",
            GitErrorKind::Validation => "Git command needs more information.",
            GitErrorKind::CommandFailed => "Git command failed.",
        }
    }
}

impl std::fmt::Display for GitError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.detail)
    }
}

impl std::error::Error for GitError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitStatus {
    pub branch: Option<String>,
    pub upstream: Option<String>,
    pub ahead: usize,
    pub behind: usize,
    pub changes: Vec<GitFileChange>,
}

impl GitStatus {
    pub fn staged_count(&self) -> usize {
        self.changes
            .iter()
            .filter(|change| change.is_staged())
            .count()
    }

    pub fn unstaged_count(&self) -> usize {
        self.changes
            .iter()
            .filter(|change| change.is_unstaged())
            .count()
    }

    pub fn conflict_count(&self) -> usize {
        self.changes
            .iter()
            .filter(|change| change.is_conflicted())
            .count()
    }

    pub fn line_totals(&self) -> (usize, usize) {
        self.changes
            .iter()
            .flat_map(|change| [change.staged_lines, change.unstaged_lines])
            .flatten()
            .fold((0, 0), |(additions, deletions), stats| {
                (
                    additions.saturating_add(stats.additions.unwrap_or_default()),
                    deletions.saturating_add(stats.deletions.unwrap_or_default()),
                )
            })
    }

    pub fn inherit_line_stats(&mut self, previous: &Self) {
        for change in &mut self.changes {
            let Some(old) = previous
                .changes
                .iter()
                .find(|old| old.path == change.path && old.original_path == change.original_path)
            else {
                continue;
            };
            if old.index_status == change.index_status {
                change.staged_lines = old.staged_lines;
            }
            if old.worktree_status == change.worktree_status {
                change.unstaged_lines = old.unstaged_lines;
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitFileChange {
    pub path: PathBuf,
    pub original_path: Option<PathBuf>,
    pub index_status: char,
    pub worktree_status: char,
    pub staged_lines: Option<GitLineStats>,
    pub unstaged_lines: Option<GitLineStats>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GitLineStats {
    pub additions: Option<usize>,
    pub deletions: Option<usize>,
}

impl GitFileChange {
    pub fn line_stats(&self, kind: GitDiffKind) -> Option<GitLineStats> {
        match kind {
            GitDiffKind::Unstaged => self.unstaged_lines,
            GitDiffKind::Staged => self.staged_lines,
        }
    }

    pub fn is_staged(&self) -> bool {
        self.index_status != ' ' && self.index_status != '?'
    }

    pub fn is_unstaged(&self) -> bool {
        self.worktree_status != ' ' || self.is_untracked()
    }

    pub fn is_untracked(&self) -> bool {
        self.index_status == '?' && self.worktree_status == '?'
    }

    pub fn is_conflicted(&self) -> bool {
        matches!(
            (self.index_status, self.worktree_status),
            ('D', 'D')
                | ('A', 'U')
                | ('U', 'D')
                | ('U', 'A')
                | ('D', 'U')
                | ('A', 'A')
                | ('U', 'U')
        )
    }

    pub fn status_label(&self) -> &'static str {
        if self.is_conflicted() {
            "conflict"
        } else if self.is_untracked() {
            "untracked"
        } else {
            match (self.index_status, self.worktree_status) {
                ('A', _) => "added",
                ('D', _) | (_, 'D') => "deleted",
                ('R', _) => "renamed",
                ('C', _) => "copied",
                ('T', _) | (_, 'T') => "type changed",
                _ => "modified",
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GitDiffKind {
    Unstaged,
    Staged,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitOperationResult {
    pub summary: String,
}

#[derive(Clone, Copy)]
enum CommandClass {
    Local,
    Network,
}

struct GitCommandOutput {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

pub fn git_status(project: &Project) -> Result<GitStatus, GitError> {
    git_status_cancellable(project, &CancellationToken::new())
}

pub fn git_status_cancellable(
    project: &Project,
    cancellation: &CancellationToken,
) -> Result<GitStatus, GitError> {
    let mut status = git_status_summary_cancellable(project, cancellation)?;
    let unstaged = run_git(
        project,
        &["diff", "--no-ext-diff", "--numstat", "-z"],
        CommandClass::Local,
        cancellation,
    )?;
    apply_numstat(&mut status, &unstaged.stdout, GitDiffKind::Unstaged)?;
    let staged = run_git(
        project,
        &["diff", "--no-ext-diff", "--cached", "--numstat", "-z"],
        CommandClass::Local,
        cancellation,
    )?;
    apply_numstat(&mut status, &staged.stdout, GitDiffKind::Staged)?;
    add_local_untracked_line_stats(project, &mut status, cancellation);
    Ok(status)
}

pub fn git_status_summary_cancellable(
    project: &Project,
    cancellation: &CancellationToken,
) -> Result<GitStatus, GitError> {
    let output = run_git(
        project,
        &["status", "--porcelain=v1", "-z", "--branch"],
        CommandClass::Local,
        cancellation,
    )?;
    parse_status(&output.stdout)
}

pub fn git_diff_cancellable(
    project: &Project,
    change: &GitFileChange,
    kind: GitDiffKind,
    cancellation: &CancellationToken,
) -> Result<String, GitError> {
    if kind == GitDiffKind::Unstaged && change.is_untracked() {
        let absolute_path = project.path.join(&change.path);
        let contents = read_project_file_cancellable(project, &absolute_path, cancellation)
            .map_err(|error| classify_error(error, project, CommandClass::Local))?;
        let path = change.path.to_string_lossy();
        let mut diff =
            format!("diff --git a/{path} b/{path}\nnew file\n--- /dev/null\n+++ b/{path}\n");
        for line in contents.lines() {
            diff.push('+');
            diff.push_str(line);
            diff.push('\n');
        }
        return Ok(diff);
    }

    let path = change.path.to_string_lossy().into_owned();
    let mut args = vec!["diff", "--no-ext-diff", "--no-color"];
    if kind == GitDiffKind::Staged {
        args.push("--cached");
    }
    args.extend(["--", &path]);
    let output = run_git(project, &args, CommandClass::Local, cancellation)?;
    String::from_utf8(output.stdout)
        .map_err(|_| GitError::new(GitErrorKind::CommandFailed, "Git diff is not valid UTF-8"))
}

pub fn git_stage_cancellable(
    project: &Project,
    paths: &[PathBuf],
    cancellation: &CancellationToken,
) -> Result<GitOperationResult, GitError> {
    run_path_operation(project, "add", &[], paths, cancellation)?;
    Ok(operation_result("Staged changes."))
}

pub fn git_stage_all_cancellable(
    project: &Project,
    cancellation: &CancellationToken,
) -> Result<GitOperationResult, GitError> {
    run_git(
        project,
        &["add", "--all"],
        CommandClass::Local,
        cancellation,
    )?;
    Ok(operation_result("Staged all changes."))
}

pub fn git_unstage_cancellable(
    project: &Project,
    paths: &[PathBuf],
    cancellation: &CancellationToken,
) -> Result<GitOperationResult, GitError> {
    let path_args = path_arguments(paths)?;
    let mut args = vec![
        "restore".to_string(),
        "--staged".to_string(),
        "--".to_string(),
    ];
    args.extend(path_args.iter().cloned());
    let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
    if let Err(error) = run_git(project, &refs, CommandClass::Local, cancellation) {
        if !head_is_unborn(&error) {
            return Err(error);
        }
        let mut fallback = vec![
            "rm".to_string(),
            "--cached".to_string(),
            "--quiet".to_string(),
            "--".to_string(),
        ];
        fallback.extend(path_args);
        let refs = fallback.iter().map(String::as_str).collect::<Vec<_>>();
        run_git(project, &refs, CommandClass::Local, cancellation)?;
    }
    Ok(operation_result("Unstaged changes."))
}

pub fn git_unstage_all_cancellable(
    project: &Project,
    cancellation: &CancellationToken,
) -> Result<GitOperationResult, GitError> {
    if let Err(error) = run_git(
        project,
        &["restore", "--staged", ":/"],
        CommandClass::Local,
        cancellation,
    ) {
        if !head_is_unborn(&error) {
            return Err(error);
        }
        run_git(
            project,
            &["rm", "--cached", "--quiet", "-r", ":/"],
            CommandClass::Local,
            cancellation,
        )?;
    }
    Ok(operation_result("Unstaged all changes."))
}

pub fn git_discard_cancellable(
    project: &Project,
    change: &GitFileChange,
    cancellation: &CancellationToken,
) -> Result<GitOperationResult, GitError> {
    let path = change.path.to_string_lossy().into_owned();
    if change.is_untracked() {
        run_git(
            project,
            &["clean", "-f", "--", &path],
            CommandClass::Local,
            cancellation,
        )?;
    } else {
        run_git(
            project,
            &["restore", "--worktree", "--", &path],
            CommandClass::Local,
            cancellation,
        )?;
    }
    Ok(operation_result("Discarded working-tree changes."))
}

pub fn git_commit_cancellable(
    project: &Project,
    message: &str,
    amend: bool,
    cancellation: &CancellationToken,
) -> Result<GitOperationResult, GitError> {
    let message = message.trim();
    if message.is_empty() {
        return Err(GitError::new(
            GitErrorKind::Validation,
            "Commit message cannot be empty",
        ));
    }
    let mut args = vec!["commit", "-m", message];
    if amend {
        args.insert(1, "--amend");
    }
    let output = run_git(project, &args, CommandClass::Local, cancellation)?;
    let summary = output_text(&output);
    Ok(operation_result(if summary.is_empty() {
        if amend {
            "Amended commit."
        } else {
            "Created commit."
        }
    } else {
        &summary
    }))
}

pub fn git_fetch_cancellable(
    project: &Project,
    cancellation: &CancellationToken,
) -> Result<GitOperationResult, GitError> {
    let output = run_git(
        project,
        &["fetch", "--prune"],
        CommandClass::Network,
        cancellation,
    )?;
    let summary = output_text(&output);
    Ok(operation_result(if summary.is_empty() {
        "Fetched remotes."
    } else {
        &summary
    }))
}

pub fn git_push_cancellable(
    project: &Project,
    set_upstream: bool,
    cancellation: &CancellationToken,
) -> Result<GitOperationResult, GitError> {
    let mut owned_arguments = Vec::new();
    let arguments = if set_upstream {
        let branch_output = run_git(
            project,
            &["symbolic-ref", "--quiet", "--short", "HEAD"],
            CommandClass::Local,
            cancellation,
        )
        .map_err(|error| {
            if error.kind == GitErrorKind::CommandFailed {
                GitError::new(
                    GitErrorKind::Validation,
                    "A detached HEAD cannot be pushed without choosing a branch",
                )
            } else {
                error
            }
        })?;
        let branch = String::from_utf8(branch_output.stdout)
            .map_err(|_| GitError::new(GitErrorKind::CommandFailed, "Git branch is not UTF-8"))?;
        let branch = branch.trim();
        if branch.is_empty() {
            return Err(GitError::new(
                GitErrorKind::Validation,
                "A branch is required before setting an upstream",
            ));
        }

        let remote_output = run_git(project, &["remote"], CommandClass::Local, cancellation)?;
        let remotes = String::from_utf8(remote_output.stdout)
            .map_err(|_| GitError::new(GitErrorKind::CommandFailed, "Git remotes are not UTF-8"))?;
        let remote = select_push_remote(&remotes)?;
        owned_arguments.extend([
            "push".to_string(),
            "--set-upstream".to_string(),
            remote,
            branch.to_string(),
        ]);
        owned_arguments
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
    } else {
        vec!["push"]
    };

    let output = run_git(project, &arguments, CommandClass::Network, cancellation)?;
    let summary = output_text(&output);
    Ok(operation_result(if summary.is_empty() {
        "Pushed current branch."
    } else {
        &summary
    }))
}

fn select_push_remote(remotes: &str) -> Result<String, GitError> {
    let remotes = remotes
        .lines()
        .map(str::trim)
        .filter(|remote| !remote.is_empty())
        .collect::<Vec<_>>();
    if remotes.contains(&"origin") {
        return Ok("origin".into());
    }
    if let [remote] = remotes.as_slice() {
        return Ok((*remote).to_string());
    }
    Err(GitError::new(
        GitErrorKind::Validation,
        if remotes.is_empty() {
            "No Git remote is configured"
        } else {
            "Choose an upstream in Git before pushing this branch"
        },
    ))
}

fn run_path_operation(
    project: &Project,
    command: &str,
    flags: &[&str],
    paths: &[PathBuf],
    cancellation: &CancellationToken,
) -> Result<GitCommandOutput, GitError> {
    let paths = path_arguments(paths)?;
    let mut args = vec![command.to_string()];
    args.extend(flags.iter().map(|flag| (*flag).to_string()));
    args.push("--".to_string());
    args.extend(paths);
    let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
    run_git(project, &refs, CommandClass::Local, cancellation)
}

fn path_arguments(paths: &[PathBuf]) -> Result<Vec<String>, GitError> {
    if paths.is_empty() {
        return Err(GitError::new(
            GitErrorKind::Validation,
            "Git file selection cannot be empty",
        ));
    }
    Ok(paths
        .iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect())
}

fn run_git(
    project: &Project,
    args: &[&str],
    class: CommandClass,
    cancellation: &CancellationToken,
) -> Result<GitCommandOutput, GitError> {
    cancellation
        .check()
        .map_err(|error| classify_error(error, project, class))?;
    match &project.source {
        ProjectSource::Local => run_local_git(project, args, class, cancellation),
        ProjectSource::Remote { host, .. } => {
            run_remote_git(project, host, args, class, cancellation)
        }
    }
}

fn run_local_git(
    project: &Project,
    args: &[&str],
    class: CommandClass,
    cancellation: &CancellationToken,
) -> Result<GitCommandOutput, GitError> {
    let mut command = Command::new("git");
    if is_read_only_git_command(args) {
        command.env("GIT_OPTIONAL_LOCKS", "0");
    }
    command
        .arg("-c")
        .arg("color.ui=false")
        .arg("-c")
        .arg("core.quotepath=false")
        .arg("-C")
        .arg(&project.path)
        .args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let output = SshRunner::new_process(
        "Git",
        GIT_OPERATION_TIMEOUT,
        MAX_GIT_OUTPUT_BYTES,
        cancellation,
    )
    .run(command, &[])
    .map_err(|error| classify_error(error.to_string(), project, class))?;
    let result = GitCommandOutput {
        stdout: output.stdout,
        stderr: output.stderr,
    };
    if output.status.success() {
        Ok(result)
    } else {
        Err(classify_command_failure(result, project, class))
    }
}

fn run_remote_git(
    project: &Project,
    host: &str,
    args: &[&str],
    class: CommandClass,
    cancellation: &CancellationToken,
) -> Result<GitCommandOutput, GitError> {
    let script = remote_git_script(project, args);
    let stdout = run_ssh_script_bytes(host, &script, GIT_OPERATION_TIMEOUT, cancellation)
        .map_err(|error| classify_error(error, project, class))?;
    Ok(GitCommandOutput {
        stdout,
        stderr: Vec::new(),
    })
}

fn remote_git_script(project: &Project, args: &[&str]) -> String {
    let path = project.path.to_string_lossy();
    let mut command = if is_read_only_git_command(args) {
        vec!["env".to_string(), "GIT_OPTIONAL_LOCKS=0".to_string()]
    } else {
        Vec::new()
    };
    command.extend([
        "git".to_string(),
        "-c".to_string(),
        "color.ui=false".to_string(),
        "-c".to_string(),
        "core.quotepath=false".to_string(),
        "-C".to_string(),
        path.into_owned(),
    ]);
    command.extend(args.iter().map(|argument| (*argument).to_string()));
    format!(
        "exec {}\n",
        command
            .iter()
            .map(|argument| shell_quote(argument))
            .collect::<Vec<_>>()
            .join(" ")
    )
}

fn is_read_only_git_command(args: &[&str]) -> bool {
    args.first()
        .is_some_and(|command| matches!(*command, "status" | "diff" | "log" | "show" | "ls-tree"))
}

fn head_is_unborn(error: &GitError) -> bool {
    let detail = error.detail.to_ascii_lowercase();
    detail.contains("could not resolve head")
        || detail.contains("could not resolve 'head'")
        || detail.contains("ambiguous argument 'head'")
        || detail.contains("bad revision 'head'")
}

fn classify_command_failure(
    output: GitCommandOutput,
    project: &Project,
    class: CommandClass,
) -> GitError {
    let detail = output_text(&output);
    classify_error(detail, project, class)
}

fn classify_error(detail: impl Into<String>, project: &Project, class: CommandClass) -> GitError {
    let detail = detail.into();
    let normalized = detail.to_ascii_lowercase();
    let kind = if normalized.contains(&OPERATION_CANCELLED.to_ascii_lowercase()) {
        GitErrorKind::Cancelled
    } else if normalized.contains("timed out") {
        GitErrorKind::TimedOut
    } else if matches!(class, CommandClass::Network) && is_network_failure(&normalized) {
        GitErrorKind::NetworkUnavailable
    } else if normalized.contains("not a git repository") {
        GitErrorKind::NotRepository
    } else if normalized.contains("starting git") {
        GitErrorKind::GitUnavailable
    } else if project.source.is_remote()
        && (normalized.contains("starting ssh")
            || normalized.contains("ssh operation")
            || normalized.contains("connection refused"))
    {
        GitErrorKind::RemoteUnavailable
    } else {
        GitErrorKind::CommandFailed
    };
    GitError::new(kind, detail)
}

fn is_network_failure(message: &str) -> bool {
    [
        "could not resolve host",
        "could not resolve hostname",
        "network is unreachable",
        "failed to connect",
        "connection timed out",
        "connection refused",
        "no route to host",
    ]
    .iter()
    .any(|needle| message.contains(needle))
}

fn output_text(output: &GitCommandOutput) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = stdout.trim();
    let stderr = stderr.trim();
    match (stdout.is_empty(), stderr.is_empty()) {
        (false, false) => format!("{stdout}\n{stderr}"),
        (false, true) => stdout.to_string(),
        (true, false) => stderr.to_string(),
        (true, true) => String::new(),
    }
}

fn operation_result(summary: impl Into<String>) -> GitOperationResult {
    GitOperationResult {
        summary: summary.into(),
    }
}

fn parse_status(output: &[u8]) -> Result<GitStatus, GitError> {
    let mut records = output
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty());
    let mut branch = None;
    let mut upstream = None;
    let mut ahead = 0;
    let mut behind = 0;
    let mut changes = Vec::new();

    if let Some(first) = records.next() {
        if first.starts_with(b"## ") {
            (branch, upstream, ahead, behind) = parse_branch_header(&first[3..]);
        } else {
            parse_change_record(first, &mut records, &mut changes)?;
        }
    }
    while let Some(record) = records.next() {
        parse_change_record(record, &mut records, &mut changes)?;
    }

    Ok(GitStatus {
        branch,
        upstream,
        ahead,
        behind,
        changes,
    })
}

fn parse_branch_header(header: &[u8]) -> (Option<String>, Option<String>, usize, usize) {
    let header = String::from_utf8_lossy(header);
    let header = header
        .strip_prefix("No commits yet on ")
        .or_else(|| header.strip_prefix("Initial commit on "))
        .unwrap_or(&header);
    if header == "HEAD (no branch)" {
        return (Some("detached".into()), None, 0, 0);
    }

    let (tracking, counts) = header
        .split_once(" [")
        .map_or((header, ""), |(tracking, counts)| {
            (tracking, counts.trim_end_matches(']'))
        });
    let (branch, upstream) = tracking
        .split_once("...")
        .map_or((tracking, None), |(branch, upstream)| {
            (branch, Some(upstream.to_string()))
        });
    let mut ahead = 0;
    let mut behind = 0;
    for count in counts.split(',').map(str::trim) {
        if let Some(value) = count.strip_prefix("ahead ") {
            ahead = value.parse().unwrap_or_default();
        } else if let Some(value) = count.strip_prefix("behind ") {
            behind = value.parse().unwrap_or_default();
        }
    }
    (Some(branch.to_string()), upstream, ahead, behind)
}

fn parse_change_record<'a>(
    record: &[u8],
    records: &mut impl Iterator<Item = &'a [u8]>,
    changes: &mut Vec<GitFileChange>,
) -> Result<(), GitError> {
    if record.len() < 3 {
        return Err(GitError::new(
            GitErrorKind::CommandFailed,
            "Git status returned a malformed record",
        ));
    }
    let index_status = record[0] as char;
    let worktree_status = record[1] as char;
    let path = PathBuf::from(String::from_utf8_lossy(&record[3..]).into_owned());
    let original_path = if matches!(index_status, 'R' | 'C') {
        records
            .next()
            .map(|path| PathBuf::from(String::from_utf8_lossy(path).into_owned()))
    } else {
        None
    };
    changes.push(GitFileChange {
        path,
        original_path,
        index_status,
        worktree_status,
        staged_lines: None,
        unstaged_lines: None,
    });
    Ok(())
}

fn apply_numstat(status: &mut GitStatus, output: &[u8], kind: GitDiffKind) -> Result<(), GitError> {
    let mut records = output
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty());
    while let Some(record) = records.next() {
        let mut fields = record.splitn(3, |byte| *byte == b'\t');
        let additions = parse_numstat_count(fields.next())?;
        let deletions = parse_numstat_count(fields.next())?;
        let Some(path) = fields.next() else {
            return Err(GitError::new(
                GitErrorKind::CommandFailed,
                "Git numstat returned a malformed record",
            ));
        };
        let path = if path.is_empty() {
            let _original_path = records.next();
            records.next().ok_or_else(|| {
                GitError::new(
                    GitErrorKind::CommandFailed,
                    "Git numstat returned a malformed rename",
                )
            })?
        } else {
            path
        };
        let path = PathBuf::from(String::from_utf8_lossy(path).into_owned());
        let Some(change) = status.changes.iter_mut().find(|change| change.path == path) else {
            continue;
        };
        let stats = Some(GitLineStats {
            additions,
            deletions,
        });
        match kind {
            GitDiffKind::Unstaged => change.unstaged_lines = stats,
            GitDiffKind::Staged => change.staged_lines = stats,
        }
    }
    Ok(())
}

fn parse_numstat_count(field: Option<&[u8]>) -> Result<Option<usize>, GitError> {
    let Some(field) = field else {
        return Err(GitError::new(
            GitErrorKind::CommandFailed,
            "Git numstat returned a malformed record",
        ));
    };
    if field == b"-" {
        return Ok(None);
    }
    let value = std::str::from_utf8(field)
        .ok()
        .and_then(|value| value.parse().ok())
        .ok_or_else(|| {
            GitError::new(
                GitErrorKind::CommandFailed,
                "Git numstat returned an invalid line count",
            )
        })?;
    Ok(Some(value))
}

fn add_local_untracked_line_stats(
    project: &Project,
    status: &mut GitStatus,
    cancellation: &CancellationToken,
) {
    if project.source.is_remote() {
        return;
    }
    let mut remaining_bytes = MAX_LINE_STAT_TOTAL_BYTES;
    for change in status
        .changes
        .iter_mut()
        .filter(|change| change.is_untracked())
    {
        if cancellation.is_cancelled() || remaining_bytes == 0 {
            break;
        }
        let path = project.path.join(&change.path);
        let Ok(metadata) = std::fs::metadata(&path) else {
            continue;
        };
        if metadata.len() > MAX_LINE_STAT_FILE_BYTES || metadata.len() > remaining_bytes {
            continue;
        }
        let Ok(contents) = std::fs::read(path) else {
            continue;
        };
        remaining_bytes = remaining_bytes.saturating_sub(contents.len() as u64);
        change.unstaged_lines = Some(match std::str::from_utf8(&contents) {
            Ok(text) => GitLineStats {
                additions: Some(text.lines().count()),
                deletions: Some(0),
            },
            Err(_) => GitLineStats {
                additions: None,
                deletions: None,
            },
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ProjectSource, ProjectType};
    use std::path::Path;

    #[test]
    fn parses_branch_tracking_and_mixed_changes() {
        let status = parse_status(
            b"## main...origin/main [ahead 2, behind 1]\0M  staged.rs\0 M worktree.rs\0?? new.rs\0R  renamed.rs\0old.rs\0",
        )
        .unwrap();
        assert_eq!(status.branch.as_deref(), Some("main"));
        assert_eq!(status.upstream.as_deref(), Some("origin/main"));
        assert_eq!((status.ahead, status.behind), (2, 1));
        assert_eq!(status.staged_count(), 2);
        assert_eq!(status.unstaged_count(), 2);
        assert_eq!(
            status.changes[3].original_path,
            Some(PathBuf::from("old.rs"))
        );
    }

    #[test]
    fn parses_text_binary_and_rename_numstat_records() {
        let mut status =
            parse_status(b"## main\0 M text.rs\0 M image.png\0R  renamed.rs\0old.rs\0").unwrap();
        apply_numstat(
            &mut status,
            b"4\t2\ttext.rs\0-\t-\timage.png\x003\t1\t\0old.rs\0renamed.rs\0",
            GitDiffKind::Unstaged,
        )
        .unwrap();

        assert_eq!(
            status.changes[0].unstaged_lines,
            Some(GitLineStats {
                additions: Some(4),
                deletions: Some(2),
            })
        );
        assert_eq!(
            status.changes[1].unstaged_lines,
            Some(GitLineStats {
                additions: None,
                deletions: None,
            })
        );
        assert_eq!(status.changes[2].unstaged_lines.unwrap().additions, Some(3));
        assert_eq!(status.line_totals(), (7, 3));
    }

    #[test]
    fn network_errors_have_a_concise_status() {
        let project = test_project(PathBuf::from("repo"));
        let error = classify_error(
            "fatal: unable to access url: Could not resolve host",
            &project,
            CommandClass::Network,
        );
        assert_eq!(error.kind, GitErrorKind::NetworkUnavailable);
        assert_eq!(error.status_text(), "Network unavailable.");

        let auth_error = classify_error(
            "fatal: unable to access url: The requested URL returned error: 403",
            &project,
            CommandClass::Network,
        );
        assert_eq!(auth_error.kind, GitErrorKind::CommandFailed);
    }

    #[test]
    fn push_remote_prefers_origin_then_a_single_remote() {
        assert_eq!(select_push_remote("backup\norigin\n").unwrap(), "origin");
        assert_eq!(select_push_remote("upstream\n").unwrap(), "upstream");
        assert_eq!(
            select_push_remote("").unwrap_err().kind,
            GitErrorKind::Validation
        );
        assert_eq!(
            select_push_remote("one\ntwo\n").unwrap_err().kind,
            GitErrorKind::Validation
        );
    }

    #[test]
    fn local_status_stage_diff_and_unstage_use_system_git() {
        if Command::new("git").arg("--version").output().is_err() {
            return;
        }
        let root = test_directory("git-workflow");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        run_test_git(&root, &["init", "-q"]);
        run_test_git(&root, &["config", "user.name", "DevHub Test"]);
        run_test_git(&root, &["config", "user.email", "devhub@example.invalid"]);
        std::fs::write(root.join("tracked.txt"), "one\n").unwrap();
        run_test_git(&root, &["add", "tracked.txt"]);
        run_test_git(&root, &["commit", "-qm", "initial"]);
        std::fs::write(root.join("tracked.txt"), "one\ntwo\n").unwrap();

        let project = test_project(root.clone());
        let cancellation = CancellationToken::new();
        let status = git_status_cancellable(&project, &cancellation).unwrap();
        let change = status.changes.first().unwrap();
        assert!(change.is_unstaged());
        let diff =
            git_diff_cancellable(&project, change, GitDiffKind::Unstaged, &cancellation).unwrap();
        assert!(diff.contains("+two"));
        git_stage_cancellable(&project, std::slice::from_ref(&change.path), &cancellation).unwrap();
        assert_eq!(git_status(&project).unwrap().staged_count(), 1);
        git_unstage_cancellable(&project, std::slice::from_ref(&change.path), &cancellation)
            .unwrap();
        assert_eq!(git_status(&project).unwrap().staged_count(), 0);

        git_stage_all_cancellable(&project, &cancellation).unwrap();
        git_commit_cancellable(&project, "second", false, &cancellation).unwrap();
        assert!(git_status(&project).unwrap().changes.is_empty());

        std::fs::write(root.join("tracked.txt"), "one\ntwo\nthree\n").unwrap();
        git_stage_all_cancellable(&project, &cancellation).unwrap();
        git_commit_cancellable(&project, "second amended", true, &cancellation).unwrap();
        assert!(git_status(&project).unwrap().changes.is_empty());

        std::fs::write(root.join("tracked.txt"), "discard this\n").unwrap();
        let status = git_status(&project).unwrap();
        let tracked = status.changes.first().unwrap();
        git_discard_cancellable(&project, tracked, &cancellation).unwrap();
        assert_eq!(
            std::fs::read_to_string(root.join("tracked.txt"))
                .unwrap()
                .replace("\r\n", "\n"),
            "one\ntwo\nthree\n"
        );

        std::fs::write(root.join("untracked.txt"), "local only\n").unwrap();
        let status = git_status(&project).unwrap();
        let untracked = status
            .changes
            .iter()
            .find(|change| change.is_untracked())
            .unwrap();
        assert_eq!(untracked.unstaged_lines.unwrap().additions, Some(1));
        let diff = git_diff_cancellable(&project, untracked, GitDiffKind::Unstaged, &cancellation)
            .unwrap();
        assert!(diff.contains("+local only"));
        git_discard_cancellable(&project, untracked, &cancellation).unwrap();
        assert!(!root.join("untracked.txt").exists());

        std::fs::write(
            root.join("large-untracked.txt"),
            vec![b'x'; MAX_LINE_STAT_FILE_BYTES as usize + 1],
        )
        .unwrap();
        let status = git_status(&project).unwrap();
        let large = status
            .changes
            .iter()
            .find(|change| change.path == Path::new("large-untracked.txt"))
            .unwrap();
        assert_eq!(large.unstaged_lines, None);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn unstaging_works_before_the_first_commit() {
        if Command::new("git").arg("--version").output().is_err() {
            return;
        }
        let root = test_directory("git-unborn");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        run_test_git(&root, &["init", "-q"]);
        std::fs::write(root.join("new.txt"), "new\n").unwrap();

        let project = test_project(root.clone());
        let cancellation = CancellationToken::new();
        git_stage_all_cancellable(&project, &cancellation).unwrap();
        assert_eq!(git_status(&project).unwrap().staged_count(), 1);
        git_unstage_all_cancellable(&project, &cancellation).unwrap();
        let status = git_status(&project).unwrap();
        assert_eq!(status.staged_count(), 0);
        assert!(status.changes[0].is_untracked());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn push_sets_and_reuses_the_current_branch_upstream() {
        if Command::new("git").arg("--version").output().is_err() {
            return;
        }
        let root = test_directory("git-push");
        let remote = test_directory("git-push-remote");
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&remote);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&remote).unwrap();
        run_test_git(&remote, &["init", "--bare", "-q"]);
        run_test_git(&root, &["init", "-q"]);
        run_test_git(&root, &["config", "user.name", "DevHub Test"]);
        run_test_git(&root, &["config", "user.email", "devhub@example.invalid"]);
        std::fs::write(root.join("tracked.txt"), "one\n").unwrap();
        run_test_git(&root, &["add", "tracked.txt"]);
        run_test_git(&root, &["commit", "-qm", "initial"]);
        let remote_path = remote.to_string_lossy();
        run_test_git(&root, &["remote", "add", "origin", &remote_path]);

        let project = test_project(root.clone());
        let cancellation = CancellationToken::new();
        git_push_cancellable(&project, true, &cancellation).unwrap();
        assert!(git_status(&project).unwrap().upstream.is_some());

        std::fs::write(root.join("tracked.txt"), "one\ntwo\n").unwrap();
        git_stage_all_cancellable(&project, &cancellation).unwrap();
        git_commit_cancellable(&project, "second", false, &cancellation).unwrap();
        git_push_cancellable(&project, false, &cancellation).unwrap();
        assert_eq!(git_status(&project).unwrap().ahead, 0);

        let _ = std::fs::remove_dir_all(root);
        let _ = std::fs::remove_dir_all(remote);
    }

    #[test]
    fn remote_git_script_quotes_project_paths_and_arguments() {
        let mut project = test_project(PathBuf::from("/srv/work/repo name; touch nope"));
        project.source = ProjectSource::Remote {
            name: "buildbox".into(),
            host: "buildbox".into(),
        };
        let script = remote_git_script(&project, &["status", "--porcelain=v1"]);
        assert!(script.starts_with("exec 'env' 'GIT_OPTIONAL_LOCKS=0' 'git'"));
        assert!(script.contains("'/srv/work/repo name; touch nope'"));
        assert!(script.ends_with("'status' '--porcelain=v1'\n"));
    }

    fn test_project(path: PathBuf) -> Project {
        Project {
            name: "repo".into(),
            path,
            source: ProjectSource::Local,
            project_type: ProjectType::Unknown,
            has_git: true,
            git_remote: None,
            markers_found: vec![".git".into()],
            last_modified: None,
            search_key: String::new(),
        }
    }

    fn test_directory(label: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .unwrap()
            .join("target")
            .join("test-support")
            .join(format!("{label}-{}", std::process::id()))
    }

    fn run_test_git(root: &Path, args: &[&str]) {
        let status = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(args)
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?} failed");
    }
}
