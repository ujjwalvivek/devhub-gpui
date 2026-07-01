use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::discovery::{Project, ProjectSource};
use crate::remote::{read_remote_file, read_remote_readme, search_remote_content, DirectoryEntry};

const MAX_FILE_BYTES: u64 = 512 * 1024;
const MAX_TREE_ENTRIES: usize = 500;
const MAX_TREE_SCAN_ENTRIES: usize = 5_000;
const MAX_SEARCH_HITS: usize = 200;
const MAX_PREVIEW_CHARS: usize = 240;

#[derive(Debug, Clone)]
pub struct FileEntry {
    pub name: String,
    pub path: PathBuf,
    pub depth: usize,
    pub is_dir: bool,
}

#[derive(Debug, Clone)]
pub struct TreeListing {
    pub entries: Vec<FileEntry>,
    pub truncated: bool,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct SearchHit {
    pub path: PathBuf,
    pub line: usize,
    pub preview: String,
}

pub fn list_local_subdirs(path: &Path) -> Result<Vec<DirectoryEntry>, String> {
    let mut entries = std::fs::read_dir(path)
        .map_err(|error| format!("{}: {error}", path.display()))?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            entry
                .file_type()
                .ok()
                .filter(|kind| kind.is_dir())
                .map(|_| DirectoryEntry {
                    name: entry.file_name().to_string_lossy().into_owned(),
                    path: entry.path().to_string_lossy().into_owned(),
                })
        })
        .collect::<Vec<_>>();
    entries.sort_by_cached_key(|entry| entry.name.to_lowercase());
    Ok(entries)
}

pub fn local_roots() -> Vec<DirectoryEntry> {
    #[cfg(windows)]
    {
        ('A'..='Z')
            .filter_map(|letter| {
                let path = format!("{letter}:\\");
                Path::new(&path).exists().then(|| DirectoryEntry {
                    name: format!("{letter}:"),
                    path,
                })
            })
            .collect()
    }
    #[cfg(not(windows))]
    {
        vec![DirectoryEntry {
            name: "/".into(),
            path: "/".into(),
        }]
    }
}

pub fn list_project_tree(
    project: &Project,
    max_depth: usize,
    show_hidden: bool,
) -> Result<TreeListing, String> {
    match &project.source {
        ProjectSource::Local => list_tree(&project.path, max_depth, show_hidden),
        ProjectSource::Remote { host, .. } => {
            crate::remote::list_remote_tree(host, &project.path, max_depth, show_hidden)
        }
    }
}

pub fn read_project_file(project: &Project, path: &Path) -> Result<String, String> {
    match &project.source {
        ProjectSource::Local => read_file(path),
        ProjectSource::Remote { host, .. } => read_remote_file(host, path),
    }
}

pub fn read_project_readme(project: &Project) -> Result<Option<String>, String> {
    match &project.source {
        ProjectSource::Local => Ok(read_readme(&project.path)),
        ProjectSource::Remote { host, .. } => read_remote_readme(host, &project.path),
    }
}

pub fn search_project_content(project: &Project, query: &str) -> Result<Vec<SearchHit>, String> {
    match &project.source {
        ProjectSource::Local => Ok(search_content(&project.path, query)),
        ProjectSource::Remote { host, .. } => search_remote_content(host, &project.path, query),
    }
}

/// Hidden paths are optional, but gitignore rules and generated-directory
/// pruning always remain active.
pub fn list_tree(root: &Path, max_depth: usize, show_hidden: bool) -> Result<TreeListing, String> {
    let metadata =
        std::fs::metadata(root).map_err(|error| format!("{}: {error}", root.display()))?;
    if !metadata.is_dir() {
        return Err(format!("{} is not a directory", root.display()));
    }

    let mut children_by_parent: HashMap<PathBuf, Vec<FileEntry>> = HashMap::new();
    let mut warnings = Vec::new();
    let mut scanned_entries = 0usize;
    let mut scan_truncated = false;

    let walker = ignore::WalkBuilder::new(root)
        .max_depth(Some(max_depth))
        .hidden(!show_hidden)
        .git_ignore(true)
        .git_global(true)
        .filter_entry(|entry| !is_skipped_path(entry.path()))
        .build();

    for result in walker {
        let entry = match result {
            Ok(entry) => entry,
            Err(error) => {
                warnings.push(error.to_string());
                continue;
            }
        };
        let path = entry.path();
        if path == root {
            continue;
        }
        if scanned_entries >= MAX_TREE_SCAN_ENTRIES {
            scan_truncated = true;
            break;
        }
        scanned_entries += 1;

        let relative = path.strip_prefix(root).unwrap_or(path);
        let depth = relative.components().count().saturating_sub(1);
        let file_name = relative
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();
        let parent = path.parent().unwrap_or(root).to_path_buf();

        children_by_parent
            .entry(parent)
            .or_default()
            .push(FileEntry {
                name: file_name,
                path: path.to_path_buf(),
                depth,
                is_dir: path.is_dir(),
            });
    }

    let mut entries = Vec::new();
    let mut output_truncated = false;
    append_children(
        root,
        &mut children_by_parent,
        &mut entries,
        &mut output_truncated,
    );

    Ok(TreeListing {
        entries,
        truncated: scan_truncated || output_truncated,
        warnings,
    })
}

const LARGE_DIRS: &[&str] = &[
    "node_modules",
    "target",
    "build",
    "dist",
    ".next",
    "vendor",
    "__pycache__",
    "bin",
    "obj",
    "out",
];

fn append_children(
    parent: &Path,
    children_by_parent: &mut HashMap<PathBuf, Vec<FileEntry>>,
    output: &mut Vec<FileEntry>,
    truncated: &mut bool,
) {
    let Some(mut children) = children_by_parent.remove(parent) else {
        return;
    };
    sort_siblings(&mut children);

    for child in children {
        if output.len() >= MAX_TREE_ENTRIES {
            *truncated = true;
            return;
        }
        let child_path = child.path.clone();
        let is_dir = child.is_dir;
        output.push(child);
        if is_dir {
            append_children(&child_path, children_by_parent, output, truncated);
            if *truncated {
                return;
            }
        }
    }
}

pub fn read_file(path: &Path) -> Result<String, String> {
    let metadata = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if !metadata.is_file() {
        return Err("not a file".to_string());
    }
    if metadata.len() > MAX_FILE_BYTES {
        return Err(format!("file is larger than {} KiB", MAX_FILE_BYTES / 1024));
    }

    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if bytes.contains(&0) {
        return Err("binary file preview is not supported".to_string());
    }
    String::from_utf8(bytes).map_err(|_| "file is not valid UTF-8 text".to_string())
}

pub fn read_readme(dir: &Path) -> Option<String> {
    let candidates = [
        "README.md",
        "README.txt",
        "README",
        "Readme.md",
        "readme.md",
    ];
    let path = candidates
        .iter()
        .map(|name| dir.join(name))
        .find(|p| p.exists() && p.is_file())?;

    let metadata = std::fs::metadata(&path).ok()?;
    if metadata.len() > MAX_FILE_BYTES {
        return None;
    }
    let bytes = std::fs::read(&path).ok()?;
    if bytes.contains(&0) {
        return None;
    }
    String::from_utf8(bytes).ok()
}

pub fn search_content(root: &Path, query: &str) -> Vec<SearchHit> {
    let query = query.trim();
    if query.is_empty() {
        return Vec::new();
    }

    let query_lower = query.to_lowercase();
    let mut hits = Vec::new();
    let walker = ignore::WalkBuilder::new(root)
        .hidden(false)
        .git_ignore(true)
        .git_global(true)
        .filter_entry(|entry| !is_skipped_path(entry.path()))
        .build();

    for entry in walker.flatten() {
        if hits.len() >= MAX_SEARCH_HITS {
            break;
        }

        let path = entry.path();
        if !path.is_file() || is_probably_binary_or_large(path) {
            continue;
        }

        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        let content = String::from_utf8_lossy(&bytes);
        for (line_idx, line) in content.lines().enumerate() {
            if line.to_lowercase().contains(&query_lower) {
                hits.push(SearchHit {
                    path: path.to_path_buf(),
                    line: line_idx + 1,
                    preview: line.trim().chars().take(MAX_PREVIEW_CHARS).collect(),
                });
                if hits.len() >= MAX_SEARCH_HITS {
                    break;
                }
            }
        }
    }

    hits
}

fn is_skipped_path(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            LARGE_DIRS
                .iter()
                .any(|&large| name.eq_ignore_ascii_case(large))
                || name == ".git"
        })
}

fn is_probably_binary_or_large(path: &Path) -> bool {
    let Ok(metadata) = std::fs::metadata(path) else {
        return true;
    };
    if metadata.len() > MAX_FILE_BYTES {
        return true;
    }
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            matches!(
                ext.to_ascii_lowercase().as_str(),
                "exe"
                    | "dll"
                    | "pdb"
                    | "png"
                    | "jpg"
                    | "jpeg"
                    | "gif"
                    | "webp"
                    | "ico"
                    | "zip"
                    | "7z"
                    | "tar"
                    | "gz"
                    | "pdf"
                    | "otf"
                    | "ttf"
                    | "woff"
                    | "woff2"
            )
        })
}

fn sort_siblings(entries: &mut [FileEntry]) {
    entries.sort_by(|left, right| {
        right
            .is_dir
            .cmp(&left.is_dir)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
            .then_with(|| left.name.cmp(&right.name))
    });
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
            let path = std::env::temp_dir().join(format!(
                "devhub-core-ws-{label}-{}-{id}",
                std::process::id()
            ));
            fs::create_dir_all(&path).expect("create test directory");
            Self { path }
        }

        fn write(&self, relative: &str, contents: &str) {
            let path = self.path.join(relative);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("create test file parent");
            }
            fs::write(path, contents).expect("write test file");
        }

        fn create_dir(&self, relative: &str) -> PathBuf {
            let path = self.path.join(relative);
            fs::create_dir_all(&path).expect("create nested test directory");
            path
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn list_tree_returns_sorted_entries_with_depth() {
        let root = TestDir::new("tree");
        root.write("Cargo.toml", "[package]\n");
        root.write("src/main.rs", "fn main() {}\n");
        root.create_dir("src");
        root.write("README.md", "# project\n");

        let entries = list_tree(&root.path, 3, true).unwrap().entries;

        assert!(!entries.is_empty());
        assert!(entries.iter().any(|e| e.name == "Cargo.toml"));
        assert!(entries.iter().any(|e| e.name == "main.rs"));
        assert!(entries.iter().any(|e| e.name == "README.md"));

        let src_entry = entries.iter().find(|e| e.name == "main.rs").unwrap();
        assert_eq!(src_entry.depth, 1);
        assert!(!src_entry.is_dir);
    }

    #[test]
    fn list_tree_preserves_parent_child_preorder_and_directories_first() {
        let root = TestDir::new("hierarchy");
        root.write("z-file.txt", "z");
        root.write("a-dir/child.txt", "child");
        root.write("b-dir/nested/grandchild.txt", "grandchild");

        let listing = list_tree(&root.path, 4, false).unwrap();
        let names = listing
            .entries
            .iter()
            .map(|entry| entry.name.as_str())
            .collect::<Vec<_>>();

        assert_eq!(
            names,
            [
                "a-dir",
                "child.txt",
                "b-dir",
                "nested",
                "grandchild.txt",
                "z-file.txt"
            ]
        );
        assert_eq!(listing.entries[1].depth, 1);
        assert_eq!(listing.entries[4].depth, 2);
    }

    #[test]
    fn showing_hidden_does_not_disable_gitignore_rules() {
        let root = TestDir::new("hidden-ignore");
        root.create_dir(".git");
        root.write(".gitignore", "ignored.txt\n");
        root.write("ignored.txt", "ignored");
        root.write(".env", "visible only when requested");

        let hidden_off = list_tree(&root.path, 2, false).unwrap().entries;
        assert!(hidden_off.iter().all(|entry| entry.name != ".env"));
        assert!(hidden_off.iter().all(|entry| entry.name != "ignored.txt"));

        let hidden_on = list_tree(&root.path, 2, true).unwrap().entries;
        assert!(hidden_on.iter().any(|entry| entry.name == ".env"));
        assert!(hidden_on.iter().all(|entry| entry.name != "ignored.txt"));
        assert!(hidden_on.iter().all(|entry| entry.name != ".git"));
    }

    #[test]
    fn list_tree_reports_invalid_roots() {
        let root = TestDir::new("invalid-root");
        let missing = root.path.join("missing");
        assert!(list_tree(&missing, 2, false).is_err());

        root.write("file.txt", "not a directory");
        assert!(list_tree(&root.path.join("file.txt"), 2, false).is_err());
    }

    #[test]
    fn list_tree_skips_node_modules_and_target_when_not_showing_hidden() {
        let root = TestDir::new("skip");
        root.write("Cargo.toml", "");
        root.create_dir("target");
        root.write("target/debug.exe", "binary");
        root.create_dir("node_modules");
        root.write("node_modules/pkg/index.js", "module.exports = {}");

        let entries = list_tree(&root.path, 5, false).unwrap().entries;
        assert!(entries
            .iter()
            .all(|e| !e.path.to_string_lossy().contains("target")));
        assert!(entries
            .iter()
            .all(|e| !e.path.to_string_lossy().contains("node_modules")));

        // Showing hidden paths must not expose generated dependency/build trees.
        let entries = list_tree(&root.path, 5, true).unwrap().entries;
        assert!(entries
            .iter()
            .all(|e| !e.path.to_string_lossy().contains("target")));
        assert!(entries
            .iter()
            .all(|e| !e.path.to_string_lossy().contains("node_modules")));
    }

    #[test]
    fn read_file_returns_contents_for_small_text_file() {
        let root = TestDir::new("read");
        root.write("hello.txt", "Hello, World!\nSecond line.\n");

        let content = read_file(&root.path.join("hello.txt"));
        assert!(content.is_ok());
        assert!(content.unwrap().contains("Hello, World!"));
    }

    #[test]
    fn read_file_errors_on_directory() {
        let root = TestDir::new("readdir");
        root.create_dir("subdir");

        let result = read_file(&root.path.join("subdir"));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not a file"));
    }

    #[test]
    fn read_file_errors_on_nonexistent() {
        let root = TestDir::new("readmissing");

        let result = read_file(&root.path.join("does_not_exist.txt"));
        assert!(result.is_err());
    }

    #[test]
    fn read_file_rejects_binary_and_invalid_utf8() {
        let root = TestDir::new("read-binary");
        std::fs::write(root.path.join("nul.bin"), b"hello\0world").unwrap();
        std::fs::write(root.path.join("invalid.txt"), [0xff, 0xfe]).unwrap();

        assert!(read_file(&root.path.join("nul.bin"))
            .unwrap_err()
            .contains("binary"));
        assert!(read_file(&root.path.join("invalid.txt"))
            .unwrap_err()
            .contains("UTF-8"));
    }

    #[test]
    fn search_content_finds_matching_lines() {
        let root = TestDir::new("search");
        root.write("main.rs", "fn main() {\n    println!(\"hello\");\n}\n");
        root.write("lib.rs", "pub fn hello() {}\n");
        root.write("README.md", "# Hello World\n");

        let hits = search_content(&root.path, "hello");

        assert!(hits.len() >= 3);
        assert!(hits
            .iter()
            .any(|h| h.path.ends_with("main.rs") && h.line == 2));
        assert!(hits
            .iter()
            .any(|h| h.path.ends_with("lib.rs") && h.line == 1));
        assert!(hits
            .iter()
            .any(|h| h.path.ends_with("README.md") && h.line == 1));
    }

    #[test]
    fn search_content_is_case_insensitive() {
        let root = TestDir::new("case");
        root.write("file.txt", "Some UPPERCASE text\n");

        let hits = search_content(&root.path, "uppercase");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].line, 1);
    }

    #[test]
    fn search_content_returns_empty_for_empty_query() {
        let root = TestDir::new("empty");
        root.write("file.txt", "content\n");

        assert!(search_content(&root.path, "   ").is_empty());
    }

    #[test]
    fn readme_is_read_from_root() {
        let root = TestDir::new("readme");
        root.write("README.md", "# My Project\n\nDescription.\n");

        let content = read_readme(&root.path);
        assert!(content.is_some());
        assert!(content.unwrap().contains("My Project"));
    }

    #[test]
    fn list_tree_does_not_count_hidden_against_visible_limit() {
        let root = TestDir::new("cap");
        for i in 0..10 {
            root.write(&format!("file{i}.rs"), "fn f() {}");
        }
        for i in 0..100 {
            root.write(&format!(".hidden{i}"), "hidden");
        }

        // With show_hidden = false, only the 10 visible files appear.
        let entries = list_tree(&root.path, 2, false).unwrap().entries;
        assert_eq!(entries.len(), 10);

        // With show_hidden = true, all files appear (10 visible + 100 hidden -> capped at 500).
        let entries = list_tree(&root.path, 2, true).unwrap().entries;
        assert!(entries.len() > 10);
        assert!(entries.iter().any(|e| e.name.starts_with(".hidden")));
    }

    #[test]
    fn search_content_skips_binary_files_by_extension() {
        let root = TestDir::new("binary");
        root.write("code.rs", "fn hello() {}\n");
        root.write("app.exe", "fake binary hello");

        let hits = search_content(&root.path, "hello");
        assert!(hits.iter().all(|h| !h.path.ends_with("app.exe")));
        assert!(hits.iter().any(|h| h.path.ends_with("code.rs")));
    }
}
