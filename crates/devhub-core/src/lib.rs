mod cache;
mod config;
mod discovery;
mod remote;
mod workspace;

pub use cache::{load_projects, save_projects};
pub use config::{normalize_ssh_host, Config, RemoteHostConfig};
pub use discovery::{scan_directories, sort_projects, Project, ProjectSource, ProjectType};
pub use remote::{
    check_ssh_connection, list_remote_subdirs, open_remote_in_vscode, scan_remote_host,
    validate_remote_path, validate_ssh_host, DirectoryEntry,
};
pub use workspace::{
    list_local_subdirs, list_project_tree, list_tree, local_roots, read_file, read_project_file,
    read_project_readme, read_readme, search_content, search_project_content, FileEntry, SearchHit,
    TreeListing,
};
