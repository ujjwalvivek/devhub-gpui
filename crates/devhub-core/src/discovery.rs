use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const MARKERS: &[(&str, ProjectType)] = &[
    ("Cargo.toml", ProjectType::Rust),
    ("package.json", ProjectType::Node),
    ("go.mod", ProjectType::Go),
    ("pyproject.toml", ProjectType::Python),
    ("requirements.txt", ProjectType::Python),
    ("Makefile", ProjectType::Make),
    ("CMakeLists.txt", ProjectType::CMake),
    ("*.asm", ProjectType::Assembly),
    ("*.sln", ProjectType::DotNet),
    ("build.gradle", ProjectType::Java),
    ("pom.xml", ProjectType::Java),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProjectType {
    Rust,
    Node,
    Go,
    Python,
    Make,
    CMake,
    Assembly,
    DotNet,
    Java,
    Unknown,
}

impl ProjectType {
    pub fn label(self) -> &'static str {
        match self {
            Self::Rust => "Rust",
            Self::Node => "Node",
            Self::Go => "Go",
            Self::Python => "Python",
            Self::Make => "Make",
            Self::CMake => "CMake",
            Self::Assembly => "ASM",
            Self::DotNet => ".NET",
            Self::Java => "Java",
            Self::Unknown => "Unknown",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ProjectSource {
    #[default]
    Local,
    Remote {
        name: String,
        host: String,
    },
}

impl ProjectSource {
    pub fn label(&self) -> &str {
        match self {
            Self::Local => "local",
            Self::Remote { name, host } => {
                if name.is_empty() {
                    host
                } else {
                    name
                }
            }
        }
    }

    pub fn host(&self) -> Option<&str> {
        match self {
            Self::Local => None,
            Self::Remote { host, .. } => Some(host),
        }
    }

    pub fn is_remote(&self) -> bool {
        matches!(self, Self::Remote { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    pub name: String,
    pub path: PathBuf,
    pub source: ProjectSource,
    pub project_type: ProjectType,
    pub has_git: bool,
    pub git_remote: Option<String>,
    pub markers_found: Vec<String>,
    pub last_modified: Option<u64>,
    pub search_key: String,
}

impl Project {
    pub fn refresh_search_key(&mut self) {
        self.search_key = format!(
            "{} {} {} {} {}",
            self.name,
            self.path.display(),
            self.source.label(),
            self.project_type.label(),
            self.git_remote.as_deref().unwrap_or_default()
        )
        .to_lowercase();
    }
}

pub fn scan_directories(dirs: &[PathBuf], max_depth: usize) -> Vec<Project> {
    let mut projects = Vec::new();
    let mut seen = HashSet::new();

    for dir in dirs {
        if dir.exists() {
            scan_root(dir, max_depth, &mut projects, &mut seen);
        }
    }

    sort_projects(&mut projects);
    projects
}

pub fn sort_projects(projects: &mut [Project]) {
    projects.sort_by_cached_key(|project| {
        (
            project.source.label().to_lowercase(),
            project.name.to_lowercase(),
            project.path.to_string_lossy().to_lowercase(),
        )
    });
}

fn scan_root(
    root: &Path,
    max_depth: usize,
    projects: &mut Vec<Project>,
    seen: &mut HashSet<PathBuf>,
) {
    if let Some(project) = detect_project(root) {
        insert_project(project, projects, seen);
        return;
    }

    let project_count_before_children = projects.len();
    let walker = ignore::WalkBuilder::new(root)
        .max_depth(Some(1))
        .hidden(true)
        .git_ignore(true)
        .git_global(true)
        .build();

    for entry in walker.flatten() {
        let path = entry.path();
        if path.is_dir() && path != root {
            if let Some(project) = detect_project_tree(path, max_depth) {
                insert_project(project, projects, seen);
            }
        }
    }

    if projects.len() == project_count_before_children {
        let source_project = detect_project_tree(root, max_depth)
            .unwrap_or_else(|| build_local_project(root, ProjectType::Unknown, Vec::new()));
        insert_project(source_project, projects, seen);
    }
}

fn detect_project(dir: &Path) -> Option<Project> {
    let mut markers_found = Vec::new();
    let mut project_type = ProjectType::Unknown;
    scan_markers_in_dir(dir, &mut markers_found, &mut project_type);

    (!markers_found.is_empty()).then(|| build_local_project(dir, project_type, markers_found))
}

fn detect_project_tree(root: &Path, max_depth: usize) -> Option<Project> {
    let mut markers_found = Vec::new();
    let mut project_type = ProjectType::Unknown;
    let walker = ignore::WalkBuilder::new(root)
        .max_depth(Some(max_depth))
        .hidden(true)
        .git_ignore(true)
        .git_global(true)
        .build();

    for entry in walker.flatten() {
        let path = entry.path();
        if path.is_dir() {
            scan_markers_in_dir(path, &mut markers_found, &mut project_type);
        }
    }

    (!markers_found.is_empty()).then(|| build_local_project(root, project_type, markers_found))
}

fn build_local_project(
    dir: &Path,
    project_type: ProjectType,
    markers_found: Vec<String>,
) -> Project {
    let has_git = dir.join(".git").exists();
    let git_remote = has_git.then(|| read_git_remote(dir)).flatten();
    let name = dir
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| dir.to_string_lossy().into_owned());
    let last_modified = dir
        .metadata()
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(system_time_to_unix);

    let mut project = Project {
        name,
        path: dir.to_path_buf(),
        source: ProjectSource::Local,
        project_type,
        has_git,
        git_remote,
        markers_found,
        last_modified,
        search_key: String::new(),
    };
    project.refresh_search_key();
    project
}

fn insert_project(project: Project, projects: &mut Vec<Project>, seen: &mut HashSet<PathBuf>) {
    let canonical = project
        .path
        .canonicalize()
        .unwrap_or_else(|_| project.path.clone());
    if seen.insert(canonical) {
        projects.push(project);
    }
}

fn scan_markers_in_dir(
    dir: &Path,
    markers_found: &mut Vec<String>,
    project_type: &mut ProjectType,
) {
    for (marker, marker_type) in MARKERS {
        let found = marker
            .strip_prefix('*')
            .map(|extension| has_file_with_ext(dir, extension))
            .unwrap_or_else(|| dir.join(marker).exists());

        if found {
            if !markers_found.iter().any(|seen| seen == marker) {
                markers_found.push((*marker).to_string());
            }
            if *project_type == ProjectType::Unknown {
                *project_type = *marker_type;
            }
        }
    }
}

fn has_file_with_ext(dir: &Path, extension: &str) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    let expected = extension.strip_prefix('.').unwrap_or(extension);
    entries.flatten().any(|entry| {
        entry
            .path()
            .extension()
            .is_some_and(|actual| actual.eq_ignore_ascii_case(expected))
    })
}

fn read_git_remote(dir: &Path) -> Option<String> {
    let content = std::fs::read_to_string(dir.join(".git").join("config")).ok()?;
    let mut in_origin = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_origin = trimmed.contains("remote") && trimmed.contains("origin");
        } else if in_origin && trimmed.starts_with("url") {
            if let Some((_, url)) = trimmed.split_once('=') {
                return Some(url.trim().to_string());
            }
        }
    }
    None
}

fn system_time_to_unix(time: SystemTime) -> Option<u64> {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEST_DIR: AtomicU64 = AtomicU64::new(0);

    struct TestDir {
        path: PathBuf,
    }

    impl TestDir {
        fn new(label: &str) -> Self {
            let id = NEXT_TEST_DIR.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("devhub-core-{label}-{}-{id}", std::process::id()));
            fs::create_dir_all(&path).expect("create test directory");
            Self { path }
        }

        fn create_dir(&self, relative: &str) -> PathBuf {
            let path = self.path.join(relative);
            fs::create_dir_all(&path).expect("create nested test directory");
            path
        }

        fn write(&self, relative: &str, contents: &str) {
            let path = self.path.join(relative);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("create test file parent");
            }
            fs::write(path, contents).expect("write test file");
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn detects_root_project_and_reads_git_metadata_without_git_process() {
        let root = TestDir::new("root-rust");
        root.write("Cargo.toml", "[package]\nname = \"fixture\"\n");
        root.write(
            ".git/config",
            "[remote \"origin\"]\n    url = https://example.com/fixture.git\n",
        );

        let projects = scan_directories(std::slice::from_ref(&root.path), 3);

        assert_eq!(projects.len(), 1);
        let project = &projects[0];
        assert_eq!(project.project_type, ProjectType::Rust);
        assert_eq!(project.markers_found, ["Cargo.toml"]);
        assert!(project.has_git);
        assert_eq!(
            project.git_remote.as_deref(),
            Some("https://example.com/fixture.git")
        );
        assert!(project.search_key.contains("rust"));
    }

    #[test]
    fn finds_and_sorts_child_projects_without_synthetic_parent() {
        let root = TestDir::new("children");
        root.create_dir("zeta");
        root.create_dir("alpha");
        root.write("zeta/package.json", "{}");
        root.write("alpha/Cargo.toml", "[package]\nname = \"alpha\"\n");

        let projects = scan_directories(std::slice::from_ref(&root.path), 2);

        assert_eq!(projects.len(), 2);
        assert_eq!(projects[0].name, "alpha");
        assert_eq!(projects[1].name, "zeta");
        assert!(projects.iter().all(|project| project.path != root.path));
    }

    #[test]
    fn deduplicates_repeated_roots_and_detects_case_insensitive_extensions() {
        let root = TestDir::new("assembly");
        root.write("main.ASM", "mov ax, bx");

        let projects = scan_directories(&[root.path.clone(), root.path.clone()], 1);

        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].project_type, ProjectType::Assembly);
        assert_eq!(projects[0].markers_found, ["*.asm"]);
    }

    #[test]
    fn skips_missing_roots() {
        let missing = std::env::temp_dir().join(format!(
            "devhub-core-missing-{}-{}",
            std::process::id(),
            NEXT_TEST_DIR.fetch_add(1, Ordering::Relaxed)
        ));

        assert!(scan_directories(&[missing], 2).is_empty());
    }
}
