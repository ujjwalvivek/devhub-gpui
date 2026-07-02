mod cache;
mod cancellation;
mod config;
mod discovery;
mod remote;
mod workspace;

pub use cache::{cache_path, load_projects, save_projects};
pub use cancellation::{CancellationToken, OPERATION_CANCELLED};
pub use config::{normalize_ssh_host, AppearanceMode, Config, RemoteHostConfig, ThemeId};
pub use discovery::{
    scan_directories, scan_directories_cancellable, sort_projects, Project, ProjectSource,
    ProjectType,
};
pub use remote::{
    check_ssh_connection, check_ssh_connection_cancellable, list_remote_subdirs,
    list_remote_subdirs_cancellable, open_project_in_zed, scan_remote_host,
    scan_remote_host_cancellable, validate_remote_path, validate_ssh_host, zed_ssh_uri,
    DirectoryEntry,
};
pub use workspace::{
    list_local_subdirs, list_project_tree, list_project_tree_cancellable, list_tree,
    list_tree_cancellable, local_roots, read_file, read_project_file,
    read_project_file_cancellable, read_project_readme, read_project_readme_cancellable,
    read_readme, search_content, search_content_cancellable, search_project_content,
    search_project_content_cancellable, FileEntry, SearchHit, TreeListing,
};
