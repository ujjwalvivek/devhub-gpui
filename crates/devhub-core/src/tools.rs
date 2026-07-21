use std::path::{Component, Path, PathBuf};

use serde::Serialize;

use crate::{
    cache_path, git_diff_cancellable, git_log_cancellable, git_status_cancellable,
    list_project_tree_cancellable, load_projects, load_todos, read_project_file_cancellable,
    read_project_readme_cancellable, search_project_content_cancellable, todo_key,
    CancellationToken, CommitEntry, Config, GitDiffKind, GitFileChange, GitStatus, Project,
    TodoItem, TodoMap,
};

const MAX_TREE_ENTRIES: usize = 400;
const MAX_SEARCH_HITS: usize = 200;
const MAX_FILE_LINES: usize = 2_000;
const MAX_DIFF_FILES: usize = 8;
const MAX_DIFF_CHARS: usize = 120_000;
const MAX_LOG_COMMITS: usize = 50;
const MAX_STATUS_CHANGES: usize = 200;
const MAX_README_CHARS: usize = 4_000;
const MAX_TOP_LEVEL_ENTRIES: usize = 60;

#[derive(Debug, Clone)]
pub struct ToolContext {
    pub projects: Vec<Project>,
    pub pinned: Vec<PathBuf>,
    pub todos: TodoMap,
    pub catalog_as_of: Option<u64>,
}

impl ToolContext {
    pub fn load() -> Result<Self, String> {
        let projects = load_projects()?.unwrap_or_default();
        let config = Config::load_or_create()?;
        let todos = load_todos().map(|report| report.value).unwrap_or_default();
        let catalog_as_of = cache_path()
            .and_then(|path| std::fs::metadata(path).ok())
            .and_then(|meta| meta.modified().ok())
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs());
        Ok(Self {
            projects,
            pinned: config.pinned_projects,
            todos,
            catalog_as_of,
        })
    }

    pub fn resolve(&self, query: &str) -> Option<&Project> {
        let trimmed = query.trim().trim_matches('"');
        if trimmed.is_empty() {
            return None;
        }
        if let Some(project) = self
            .projects
            .iter()
            .find(|project| project.path == Path::new(trimmed))
        {
            return Some(project);
        }
        let lowered = trimmed.to_lowercase();
        if let Some(project) = self
            .projects
            .iter()
            .find(|project| project.path.to_string_lossy().to_lowercase() == lowered)
        {
            return Some(project);
        }
        if let Some(project) = self
            .projects
            .iter()
            .find(|project| project.name.eq_ignore_ascii_case(trimmed))
        {
            return Some(project);
        }
        let mut matches = self
            .projects
            .iter()
            .filter(|project| project.name.to_lowercase().contains(&lowered));
        match (matches.next(), matches.next()) {
            (Some(project), None) => Some(project),
            _ => None,
        }
    }

    fn summary(&self, project: &Project) -> ProjectSummary {
        let key = todo_key(project);
        let todo_open = self
            .todos
            .get(&key)
            .map(|items| items.iter().filter(|item| !item.done).count())
            .unwrap_or_default();
        ProjectSummary {
            name: project.name.clone(),
            path: project.path.to_string_lossy().into_owned(),
            source: project.source.label().to_string(),
            host: project.source.host().map(str::to_string),
            project_type: project.project_type.label().to_string(),
            has_git: project.has_git,
            git_remote: project.git_remote.clone(),
            markers: project.markers_found.clone(),
            last_modified: project.last_modified,
            pinned: self.pinned.contains(&project.path),
            todo_open,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectSummary {
    pub name: String,
    pub path: String,
    pub source: String,
    pub host: Option<String>,
    pub project_type: String,
    pub has_git: bool,
    pub git_remote: Option<String>,
    pub markers: Vec<String>,
    pub last_modified: Option<u64>,
    pub pinned: bool,
    pub todo_open: usize,
}

#[derive(Debug, Serialize)]
pub struct ProjectCatalog {
    pub catalog_as_of: Option<u64>,
    pub project_count: usize,
    pub projects: Vec<ProjectSummary>,
}

#[derive(Debug, Serialize)]
pub struct GitOverview {
    pub branch: Option<String>,
    pub upstream: Option<String>,
    pub ahead: usize,
    pub behind: usize,
    pub changed_files: usize,
}

#[derive(Debug, Serialize)]
pub struct CommitSummary {
    pub hash: String,
    pub author: String,
    pub date: String,
    pub message: String,
    pub refs: Vec<String>,
    pub files_changed: usize,
    pub sample_paths: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ProjectOverview {
    pub summary: ProjectSummary,
    pub readme_excerpt: Option<String>,
    pub readme_truncated: bool,
    pub top_level: Vec<String>,
    pub git: Option<GitOverview>,
    pub last_commit: Option<CommitSummary>,
    pub todos: Vec<TodoItem>,
}

#[derive(Debug, Serialize)]
pub struct TreeEntry {
    pub path: String,
    pub depth: usize,
    pub is_dir: bool,
}

#[derive(Debug, Serialize)]
pub struct ProjectTree {
    pub entries: Vec<TreeEntry>,
    pub truncated: bool,
    pub warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct FileContent {
    pub path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub returned_lines: usize,
    pub capped: bool,
    pub content: String,
}

#[derive(Debug, Serialize)]
pub struct SearchMatch {
    pub path: String,
    pub line: usize,
    pub preview: String,
}

#[derive(Debug, Serialize)]
pub struct SearchResults {
    pub query: String,
    pub matches: Vec<SearchMatch>,
    pub capped: bool,
}

#[derive(Debug, Serialize)]
pub struct StatusChange {
    pub path: String,
    pub original_path: Option<String>,
    pub index_status: String,
    pub worktree_status: String,
    pub staged_additions: Option<usize>,
    pub staged_deletions: Option<usize>,
    pub unstaged_additions: Option<usize>,
    pub unstaged_deletions: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct GitStatusResult {
    pub branch: Option<String>,
    pub upstream: Option<String>,
    pub ahead: usize,
    pub behind: usize,
    pub changes: Vec<StatusChange>,
    pub capped: bool,
}

#[derive(Debug, Serialize)]
pub struct GitDiffResult {
    pub files: Vec<String>,
    pub diff: String,
    pub truncated: bool,
}

#[derive(Debug, Serialize)]
pub struct GitLogResult {
    pub commits: Vec<CommitSummary>,
    pub skip: usize,
    pub has_more_hint: bool,
}

pub fn tool_list_projects(context: &ToolContext) -> ProjectCatalog {
    let projects = context
        .projects
        .iter()
        .map(|project| context.summary(project))
        .collect::<Vec<_>>();
    ProjectCatalog {
        catalog_as_of: context.catalog_as_of,
        project_count: projects.len(),
        projects,
    }
}

pub fn tool_project_overview(
    context: &ToolContext,
    project: &Project,
) -> Result<ProjectOverview, String> {
    let cancellation = CancellationToken::new();
    let readme = read_project_readme_cancellable(project, &cancellation)?;
    let (readme_excerpt, readme_truncated) = readme.map_or((None, false), |readme| {
        let truncated = readme.chars().count() > MAX_README_CHARS;
        let excerpt = readme.chars().take(MAX_README_CHARS).collect::<String>();
        (Some(excerpt), truncated)
    });
    let top_level = list_project_tree_cancellable(project, 1, false, &cancellation)
        .map(|listing| {
            listing
                .entries
                .into_iter()
                .take(MAX_TOP_LEVEL_ENTRIES)
                .map(|entry| {
                    let name = entry.name;
                    if entry.is_dir {
                        format!("{name}/")
                    } else {
                        name
                    }
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let git = git_status_cancellable(project, &cancellation).ok();
    let last_commit = git_log_cancellable(project, 1, 0, &cancellation)
        .ok()
        .and_then(|commits| commits.into_iter().next());
    let todos = context
        .todos
        .get(&todo_key(project))
        .cloned()
        .unwrap_or_default();
    Ok(ProjectOverview {
        summary: context.summary(project),
        readme_excerpt,
        readme_truncated,
        top_level,
        git: git.as_ref().map(git_overview),
        last_commit: last_commit.as_ref().map(commit_summary),
        todos,
    })
}

pub fn tool_list_tree(
    project: &Project,
    max_depth: usize,
    show_hidden: bool,
) -> Result<ProjectTree, String> {
    let cancellation = CancellationToken::new();
    let depth = max_depth.clamp(1, 6);
    let listing = list_project_tree_cancellable(project, depth, show_hidden, &cancellation)?;
    let capped = listing.entries.len() > MAX_TREE_ENTRIES;
    let entries = listing
        .entries
        .into_iter()
        .take(MAX_TREE_ENTRIES)
        .map(|entry| TreeEntry {
            path: entry.path.to_string_lossy().into_owned(),
            depth: entry.depth,
            is_dir: entry.is_dir,
        })
        .collect();
    Ok(ProjectTree {
        entries,
        truncated: listing.truncated || capped,
        warnings: listing.warnings,
    })
}

pub fn tool_read_file(
    project: &Project,
    relative_path: &str,
    start_line: usize,
    max_lines: usize,
) -> Result<FileContent, String> {
    let absolute = safe_join(&project.path, relative_path)?;
    let cancellation = CancellationToken::new();
    let content = read_project_file_cancellable(project, &absolute, &cancellation)?;
    let lines: Vec<&str> = content.lines().collect();
    let start = start_line.max(1);
    let limit = max_lines.clamp(1, MAX_FILE_LINES);
    let slice: Vec<&str> = lines.iter().skip(start - 1).take(limit).copied().collect();
    let end_line = start + slice.len().saturating_sub(1);
    let capped = start - 1 + slice.len() < lines.len();
    Ok(FileContent {
        path: absolute.to_string_lossy().into_owned(),
        start_line: start,
        end_line,
        returned_lines: slice.len(),
        capped,
        content: slice.join("\n"),
    })
}

pub fn tool_search_content(
    project: &Project,
    query: &str,
    max_hits: usize,
) -> Result<SearchResults, String> {
    let query = query.trim();
    if query.is_empty() {
        return Err("search query is empty".to_string());
    }
    let cancellation = CancellationToken::new();
    let hits = search_project_content_cancellable(project, query, &cancellation)?;
    let limit = max_hits.clamp(1, MAX_SEARCH_HITS);
    let capped = hits.len() > limit;
    let matches = hits
        .into_iter()
        .take(limit)
        .map(|hit| SearchMatch {
            path: hit.path.to_string_lossy().into_owned(),
            line: hit.line,
            preview: hit.preview,
        })
        .collect();
    Ok(SearchResults {
        query: query.to_string(),
        matches,
        capped,
    })
}

pub fn tool_git_status(project: &Project) -> Result<GitStatusResult, String> {
    let cancellation = CancellationToken::new();
    let status =
        git_status_cancellable(project, &cancellation).map_err(|error| error.to_string())?;
    let capped = status.changes.len() > MAX_STATUS_CHANGES;
    let changes = status
        .changes
        .iter()
        .take(MAX_STATUS_CHANGES)
        .map(status_change)
        .collect();
    Ok(GitStatusResult {
        branch: status.branch,
        upstream: status.upstream,
        ahead: status.ahead,
        behind: status.behind,
        changes,
        capped,
    })
}

pub fn tool_git_diff(
    project: &Project,
    path_filter: Option<&str>,
    max_chars: usize,
) -> Result<GitDiffResult, String> {
    let cancellation = CancellationToken::new();
    let status =
        git_status_cancellable(project, &cancellation).map_err(|error| error.to_string())?;
    let filter = path_filter
        .map(str::trim)
        .filter(|filter| !filter.is_empty());
    let changes = status
        .changes
        .iter()
        .filter(|change| {
            filter.is_none_or(|filter| {
                change.path.to_string_lossy().contains(filter)
                    || change
                        .original_path
                        .as_ref()
                        .is_some_and(|path| path.to_string_lossy().contains(filter))
            })
        })
        .take(MAX_DIFF_FILES)
        .collect::<Vec<_>>();
    if changes.is_empty() {
        return Err(match filter {
            Some(filter) => format!("no changed files match '{filter}'"),
            None => "working tree has no changes".to_string(),
        });
    }
    let limit = max_chars.clamp(1_000, MAX_DIFF_CHARS);
    let mut diff = String::new();
    let mut files = Vec::new();
    let mut truncated = false;
    for change in &changes {
        for kind in diff_kinds(change) {
            let piece = git_diff_cancellable(project, change, kind, &cancellation)
                .map_err(|error| error.to_string())?;
            if diff.len() + piece.len() > limit {
                truncated = true;
                let remaining = limit.saturating_sub(diff.len());
                diff.push_str(&piece.chars().take(remaining).collect::<String>());
                diff.push_str("\n... [diff truncated by DevHub bound]\n");
                break;
            }
            diff.push_str(&piece);
            if !piece.ends_with('\n') {
                diff.push('\n');
            }
        }
        files.push(change.path.to_string_lossy().into_owned());
        if truncated {
            break;
        }
    }
    if status.changes.len() > changes.len() {
        truncated = true;
    }
    Ok(GitDiffResult {
        files,
        diff,
        truncated,
    })
}

pub fn tool_git_log(project: &Project, skip: usize, count: usize) -> Result<GitLogResult, String> {
    let cancellation = CancellationToken::new();
    let page = count.clamp(1, MAX_LOG_COMMITS);
    let commits = git_log_cancellable(project, page, skip, &cancellation)
        .map_err(|error| error.to_string())?;
    let has_more_hint = commits.len() == page;
    Ok(GitLogResult {
        commits: commits.iter().map(commit_summary).collect(),
        skip,
        has_more_hint,
    })
}

pub fn tool_list_todos(context: &ToolContext, project: &Project) -> Vec<TodoItem> {
    context
        .todos
        .get(&todo_key(project))
        .cloned()
        .unwrap_or_default()
}

fn git_overview(status: &GitStatus) -> GitOverview {
    GitOverview {
        branch: status.branch.clone(),
        upstream: status.upstream.clone(),
        ahead: status.ahead,
        behind: status.behind,
        changed_files: status.changes.len(),
    }
}

fn status_change(change: &GitFileChange) -> StatusChange {
    StatusChange {
        path: change.path.to_string_lossy().into_owned(),
        original_path: change
            .original_path
            .as_ref()
            .map(|path| path.to_string_lossy().into_owned()),
        index_status: change.index_status.to_string(),
        worktree_status: change.worktree_status.to_string(),
        staged_additions: change.staged_lines.and_then(|stats| stats.additions),
        staged_deletions: change.staged_lines.and_then(|stats| stats.deletions),
        unstaged_additions: change.unstaged_lines.and_then(|stats| stats.additions),
        unstaged_deletions: change.unstaged_lines.and_then(|stats| stats.deletions),
    }
}

fn commit_summary(commit: &CommitEntry) -> CommitSummary {
    CommitSummary {
        hash: commit.hash.chars().take(10).collect(),
        author: commit.author.clone(),
        date: commit.date.clone(),
        message: commit.message.clone(),
        refs: commit.refs.clone(),
        files_changed: commit.files.len(),
        sample_paths: commit
            .files
            .iter()
            .take(10)
            .map(|file| file.path.clone())
            .collect(),
    }
}

fn diff_kinds(change: &GitFileChange) -> Vec<GitDiffKind> {
    let mut kinds = Vec::new();
    if !matches!(change.index_status, ' ' | '?') {
        kinds.push(GitDiffKind::Staged);
    }
    if change.is_untracked() || change.worktree_status != ' ' {
        kinds.push(GitDiffKind::Unstaged);
    }
    if kinds.is_empty() {
        kinds.push(GitDiffKind::Unstaged);
    }
    kinds
}

fn safe_join(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let relative_path = Path::new(relative.trim());
    if relative_path.as_os_str().is_empty() {
        return Err("file path is empty".to_string());
    }
    if relative_path.is_absolute()
        || relative_path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(format!(
            "path '{relative}' must stay inside the project root"
        ));
    }
    Ok(root.join(relative_path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ProjectSource, ProjectType};
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn fixture_project(name: &str, root: &Path) -> Project {
        let mut project = Project {
            name: name.to_string(),
            path: root.to_path_buf(),
            source: ProjectSource::Local,
            project_type: ProjectType::Rust,
            has_git: root.join(".git").exists(),
            git_remote: None,
            markers_found: vec!["Cargo.toml".to_string()],
            last_modified: None,
            search_key: String::new(),
        };
        project.refresh_search_key();
        project
    }

    fn write_files(root: &Path) {
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("Cargo.toml"), "[package]\nname = \"fixture\"\n").unwrap();
        std::fs::write(
            root.join("src").join("main.rs"),
            "fn main() {\n    println!(\"alpha\");\n}\n",
        )
        .unwrap();
        std::fs::write(root.join("README.md"), "# Fixture\n\nhello fixture\n").unwrap();
    }

    fn test_directory(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "devhub-gpui-{label}-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        directory
    }

    fn context_with(projects: Vec<Project>) -> ToolContext {
        ToolContext {
            projects,
            pinned: Vec::new(),
            todos: TodoMap::new(),
            catalog_as_of: Some(1_750_000_000),
        }
    }

    fn run_git(root: &Path, args: &[&str]) {
        let status = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(args)
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?} failed");
    }

    #[test]
    fn resolve_matches_path_name_and_unique_substring() {
        let root = test_directory("tools-resolve");
        let alpha = fixture_project("alpha", &root.join("alpha"));
        let beta = fixture_project("beta", &root.join("beta"));
        let context = context_with(vec![alpha, beta]);

        let by_name = context.resolve("alpha").unwrap();
        assert_eq!(by_name.name, "alpha");
        let by_path = context
            .resolve(&root.join("beta").to_string_lossy())
            .unwrap();
        assert_eq!(by_path.name, "beta");
        let by_case = context.resolve("ALPHA").unwrap();
        assert_eq!(by_case.name, "alpha");

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn resolve_rejects_empty_and_ambiguous_queries() {
        let root = test_directory("tools-ambiguous");
        let alpha_one = fixture_project("alpha-one", &root.join("one"));
        let alpha_two = fixture_project("alpha-two", &root.join("two"));
        let context = context_with(vec![alpha_one, alpha_two]);

        assert!(context.resolve("").is_none());
        assert!(context.resolve("alpha").is_none());
        assert!(context.resolve("missing").is_none());
        assert_eq!(context.resolve("one").unwrap().name, "alpha-one");

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn read_file_returns_ranged_content_and_caps() {
        let root = test_directory("tools-read");
        write_files(&root);
        let project = fixture_project("fixture", &root);

        let content = tool_read_file(&project, "src/main.rs", 2, 1).unwrap();
        assert_eq!(content.start_line, 2);
        assert_eq!(content.returned_lines, 1);
        assert!(content.content.contains("println!"));
        assert!(content.capped);

        let whole = tool_read_file(&project, "README.md", 1, 400).unwrap();
        assert!(whole.content.contains("hello fixture"));
        assert!(!whole.capped);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn read_file_rejects_traversal_and_empty_paths() {
        let root = test_directory("tools-traversal");
        write_files(&root);
        let project = fixture_project("fixture", &root);

        assert!(tool_read_file(&project, "../outside", 1, 10).is_err());
        assert!(tool_read_file(&project, "src/../../outside", 1, 10).is_err());
        assert!(tool_read_file(&project, "/absolute/path", 1, 10).is_err());
        assert!(tool_read_file(&project, "   ", 1, 10).is_err());

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn list_tree_respects_depth() {
        let root = test_directory("tools-tree");
        write_files(&root);
        let project = fixture_project("fixture", &root);

        let shallow = tool_list_tree(&project, 1, false).unwrap();
        assert!(shallow.entries.iter().all(|entry| entry.depth <= 1));
        let names: Vec<_> = shallow.entries.iter().map(|entry| &entry.path).collect();
        assert!(names.iter().any(|path| path.contains("Cargo.toml")));

        let deeper = tool_list_tree(&project, 3, false).unwrap();
        assert!(deeper
            .entries
            .iter()
            .any(|entry| entry.path.contains("main.rs")));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn search_finds_matches_and_rejects_empty_queries() {
        let root = test_directory("tools-search");
        write_files(&root);
        let project = fixture_project("fixture", &root);

        let results = tool_search_content(&project, "alpha", 50).unwrap();
        assert_eq!(results.matches.len(), 1);
        assert!(results.matches[0].path.contains("main.rs"));
        assert_eq!(results.matches[0].line, 2);
        assert!(tool_search_content(&project, "   ", 50).is_err());

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn overview_without_git_omits_git_and_includes_todos() {
        let root = test_directory("tools-overview");
        write_files(&root);
        let project = fixture_project("fixture", &root);
        let mut context = context_with(vec![project.clone()]);
        context
            .todos
            .insert(todo_key(&project), vec![TodoItem::new("wire the renderer")]);

        let overview = tool_project_overview(&context, &project).unwrap();

        assert!(overview.git.is_none());
        assert_eq!(overview.todos.len(), 1);
        assert_eq!(overview.summary.todo_open, 1);
        assert!(overview.readme_excerpt.unwrap().contains("# Fixture"));
        assert!(overview.top_level.iter().any(|entry| entry == "src/"));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn git_tools_report_status_log_and_diff() {
        let root = test_directory("tools-git");
        write_files(&root);
        run_git(&root, &["init", "-q"]);
        run_git(&root, &["add", "."]);
        run_git(&root, &["commit", "-qm", "initial"]);
        std::fs::write(
            root.join("src").join("main.rs"),
            "fn main() {\n    println!(\"beta\");\n}\n",
        )
        .unwrap();
        let project = fixture_project("fixture", &root);

        let status = tool_git_status(&project).unwrap();
        assert_eq!(status.changes.len(), 1);
        assert!(status.branch.is_some());

        let log = tool_git_log(&project, 0, 10).unwrap();
        assert_eq!(log.commits.len(), 1);
        assert_eq!(log.commits[0].message, "initial");

        let diff = tool_git_diff(&project, None, 60_000).unwrap();
        assert!(diff.diff.contains("beta"));
        assert!(!diff.truncated);

        let filtered = tool_git_diff(&project, Some("does-not-exist"), 60_000);
        assert!(filtered.is_err());

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn catalog_lists_summaries_with_pinned_and_todos() {
        let root = test_directory("tools-catalog");
        let alpha = fixture_project("alpha", &root.join("alpha"));
        let beta = fixture_project("beta", &root.join("beta"));
        let mut context = context_with(vec![alpha.clone(), beta]);
        context.pinned.push(alpha.path.clone());
        context.todos.insert(
            todo_key(&alpha),
            vec![TodoItem::new("open item"), {
                let mut item = TodoItem::new("done item");
                item.done = true;
                item
            }],
        );

        let catalog = tool_list_projects(&context);

        assert_eq!(catalog.project_count, 2);
        assert_eq!(catalog.catalog_as_of, Some(1_750_000_000));
        let summary = &catalog.projects[0];
        assert!(summary.pinned);
        assert_eq!(summary.todo_open, 1);
        assert!(!catalog.projects[1].pinned);

        std::fs::remove_dir_all(root).unwrap();
    }
}
