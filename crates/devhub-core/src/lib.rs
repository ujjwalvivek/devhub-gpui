mod cache;
mod config;
mod discovery;
mod remote;
mod workspace;

pub use cache::{cache_path, load_projects, save_projects};
pub use config::{normalize_ssh_host, AppearanceMode, Config, RemoteHostConfig, ThemeId};
pub use discovery::{scan_directories, sort_projects, Project, ProjectSource, ProjectType};
pub use remote::{
    check_ssh_connection, list_remote_subdirs, open_project_in_zed, scan_remote_host,
    validate_remote_path, validate_ssh_host, zed_ssh_uri, DirectoryEntry,
};
pub use workspace::{
    list_local_subdirs, list_project_tree, list_tree, local_roots, read_file, read_project_file,
    read_project_readme, read_readme, search_content, search_project_content, FileEntry, SearchHit,
    TreeListing,
};
