mod ai;
mod cache;
mod cancellation;
mod config;
mod discovery;
mod git;
mod persistence;
mod remote;
mod ssh;
mod workspace;

pub use ai::{
    clear_project_context, delete_zen_api_key, fetch_opencode_models,
    load_or_build_project_context, parse_architecture_response, question_excerpts,
    store_zen_api_key, stream_opencode_answer, zen_api_key_exists, ArchitectureEdge,
    ArchitectureGraph, ArchitectureNode, ArchitectureResponse, OpenCodeService, ProjectContext,
    ProjectContextFile, ZenError, ZenErrorKind, ZenModel,
};
pub use cache::{
    cache_path, load_projects, load_projects_with_diagnostics, save_projects,
    save_projects_with_diagnostics,
};
pub use cancellation::{CancellationToken, OPERATION_CANCELLED};
pub use config::{
    normalize_ssh_host, AppearanceMode, Config, ProjectLocator, RemoteHostConfig, ThemeId,
};
pub use discovery::{
    scan_directories, scan_directories_cancellable, sort_projects, Project, ProjectSource,
    ProjectType,
};
pub use git::{
    git_branches_cancellable, git_commit_cancellable, git_commit_diff_cancellable,
    git_diff_cancellable, git_discard_cancellable, git_fetch_cancellable, git_log_cancellable,
    git_push_cancellable, git_remote_to_github_url, git_stage_all_cancellable,
    git_stage_cancellable, git_status, git_status_cancellable, git_status_summary_cancellable,
    git_switch_branch_cancellable, git_unstage_all_cancellable, git_unstage_cancellable,
    CommitEntry, CommitFileChange, GitBranch, GitDiffKind, GitError, GitErrorKind, GitFileChange,
    GitLineStats, GitOperationResult, GitStatus, HISTORY_PAGE_SIZE,
};
pub use persistence::{
    PersistenceEvent, PersistenceFailure, PersistenceOperation, PersistenceRecoverySource,
    PersistenceReport, PersistenceStore,
};
pub use remote::{
    check_ssh_connection, check_ssh_connection_cancellable, list_remote_subdirs,
    list_remote_subdirs_cancellable, open_project_in_zed, scan_remote_host,
    scan_remote_host_cancellable, shell_quote, validate_remote_path, validate_ssh_host,
    zed_ssh_uri, DirectoryEntry,
};
pub use workspace::{
    list_local_subdirs, list_project_tree, list_project_tree_cancellable, list_tree,
    list_tree_cancellable, local_roots, read_file, read_project_file,
    read_project_file_cancellable, read_project_readme, read_project_readme_cancellable,
    read_readme, search_content, search_content_cancellable, search_project_content,
    search_project_content_cancellable, FileEntry, SearchHit, TreeListing,
};
