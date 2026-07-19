use std::fs;
use std::path::PathBuf;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use devhub_core::{
    load_projects, save_projects, scan_directories, Project, ProjectSource, ProjectType,
};

const PROJECT_COUNT: usize = 10_000;
const SCAN_DIRECTORY_COUNT: usize = 500;

#[test]
#[ignore = "manual release-mode Milestone 0 measurement"]
fn measure_cache_and_local_scan_baseline() {
    let root = unique_temp_dir("core-baseline");
    let state = root.join("state");
    let scan_root = root.join("scan");
    fs::create_dir_all(&scan_root).unwrap();
    std::env::set_var("DEVHUB_GPUI_STATE_DIR", &state);

    for index in 0..SCAN_DIRECTORY_COUNT {
        let directory = scan_root.join(format!("project-{index:04}"));
        fs::create_dir(&directory).unwrap();
        fs::write(
            directory.join("Cargo.toml"),
            format!("[package]\nname = \"project-{index:04}\"\n"),
        )
        .unwrap();
    }

    let scan_started = Instant::now();
    let scanned = scan_directories(std::slice::from_ref(&scan_root), 1);
    let scan_elapsed = scan_started.elapsed();
    assert_eq!(scanned.len(), SCAN_DIRECTORY_COUNT);

    let projects = (0..PROJECT_COUNT)
        .map(|index| {
            let mut project = Project {
                name: format!("project-{index:05}"),
                path: PathBuf::from(format!(r"F:\projects\project-{index:05}")),
                source: ProjectSource::Local,
                project_type: ProjectType::Rust,
                has_git: true,
                git_remote: Some(format!("https://example.com/project-{index:05}.git")),
                markers_found: vec!["Cargo.toml".into(), ".git".into()],
                last_modified: Some(index as u64),
                search_key: String::new(),
            };
            project.refresh_search_key();
            project
        })
        .collect::<Vec<_>>();

    let save_started = Instant::now();
    save_projects(&projects).unwrap();
    let save_elapsed = save_started.elapsed();
    let cache_bytes = fs::metadata(devhub_core::cache_path().unwrap())
        .unwrap()
        .len();

    let load_started = Instant::now();
    let loaded = load_projects().unwrap().unwrap();
    let load_elapsed = load_started.elapsed();
    assert_eq!(loaded.len(), PROJECT_COUNT);

    println!(
        "M0_BASELINE scan_projects={} scan_ms={:.3} cache_projects={} cache_bytes={} cache_save_ms={:.3} cache_load_ms={:.3}",
        scanned.len(),
        scan_elapsed.as_secs_f64() * 1_000.0,
        loaded.len(),
        cache_bytes,
        save_elapsed.as_secs_f64() * 1_000.0,
        load_elapsed.as_secs_f64() * 1_000.0,
    );

    std::env::remove_var("DEVHUB_GPUI_STATE_DIR");
    fs::remove_dir_all(root).unwrap();
}

fn unique_temp_dir(label: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "devhub-gpui-{label}-{}-{unique}",
        std::process::id()
    ))
}
