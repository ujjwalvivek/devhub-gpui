use serde::{Deserialize, Serialize};

use crate::Project;

const CACHE_VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
struct ProjectCache {
    version: u32,
    projects: Vec<Project>,
}

pub fn cache_path() -> Option<std::path::PathBuf> {
    crate::Config::cache_dir().map(|dir| dir.join("projects.toml"))
}

pub fn load_projects() -> Result<Option<Vec<Project>>, String> {
    let path = cache_path()
        .ok_or_else(|| "cannot determine the devhub-gpui cache directory".to_string())?;
    load_projects_from_path(&path)
}

fn load_projects_from_path(path: &std::path::Path) -> Result<Option<Vec<Project>>, String> {
    if !path.exists() {
        return Ok(None);
    }

    let raw = std::fs::read_to_string(path)
        .map_err(|error| format!("reading {}: {error}", path.display()))?;
    let cache: ProjectCache =
        toml::from_str(&raw).map_err(|error| format!("parsing {}: {error}", path.display()))?;

    if cache.version != CACHE_VERSION {
        return Ok(None);
    }

    let mut projects = cache.projects;
    for project in &mut projects {
        project.refresh_search_key();
    }
    crate::sort_projects(&mut projects);

    Ok(Some(projects))
}

pub fn save_projects(projects: &[Project]) -> Result<(), String> {
    let path = cache_path()
        .ok_or_else(|| "cannot determine the devhub-gpui cache directory".to_string())?;
    save_projects_to_path(&path, projects)
}

fn save_projects_to_path(path: &std::path::Path, projects: &[Project]) -> Result<(), String> {
    let cache = ProjectCache {
        version: CACHE_VERSION,
        projects: projects.to_vec(),
    };
    let raw =
        toml::to_string(&cache).map_err(|error| format!("serializing project cache: {error}"))?;
    crate::config::write_crash_safe(path, raw.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Project, ProjectSource, ProjectType};
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn fixture_project(name: &str) -> Project {
        let mut project = Project {
            name: name.to_string(),
            path: PathBuf::from(format!(r"F:\fixtures\{name}")),
            source: ProjectSource::Local,
            project_type: ProjectType::Rust,
            has_git: false,
            git_remote: None,
            markers_found: vec!["Cargo.toml".to_string()],
            last_modified: None,
            search_key: String::new(),
        };
        project.refresh_search_key();
        project
    }

    #[test]
    fn cache_roundtrips_through_toml() {
        let projects = vec![fixture_project("alpha"), fixture_project("beta")];

        let cache = ProjectCache {
            version: CACHE_VERSION,
            projects: projects.clone(),
        };
        let raw = toml::to_string(&cache).unwrap();
        let deserialized: ProjectCache = toml::from_str(&raw).unwrap();

        assert_eq!(deserialized.version, CACHE_VERSION);
        assert_eq!(deserialized.projects.len(), 2);
        assert_eq!(deserialized.projects[0].name, "alpha");
        assert_eq!(deserialized.projects[1].name, "beta");
    }

    #[test]
    fn wrong_version_cache_is_rejected() {
        let raw = r#"version = 999
projects = []
"#;
        let cache: ProjectCache = toml::from_str(raw).unwrap();
        assert_ne!(cache.version, CACHE_VERSION);
    }

    #[test]
    fn missing_and_malformed_cache_fail_safely() {
        let directory = test_directory("cache-recovery");
        let path = directory.join("projects.toml");

        assert!(load_projects_from_path(&path).unwrap().is_none());

        std::fs::create_dir_all(&directory).unwrap();
        let malformed = b"this is not valid = [toml";
        std::fs::write(&path, malformed).unwrap();
        let error = load_projects_from_path(&path).unwrap_err();

        assert!(error.contains("parsing"));
        assert_eq!(std::fs::read(&path).unwrap(), malformed);

        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn future_cache_version_is_ignored_without_modification() {
        let directory = test_directory("future-cache");
        let path = directory.join("projects.toml");
        std::fs::create_dir_all(&directory).unwrap();
        let future = b"version = 99\nprojects = []\n";
        std::fs::write(&path, future).unwrap();

        assert!(load_projects_from_path(&path).unwrap().is_none());
        assert_eq!(std::fs::read(&path).unwrap(), future);

        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn large_cache_roundtrip_preserves_all_projects() {
        let directory = test_directory("large-cache");
        let path = directory.join("projects.toml");
        let projects = (0..10_000)
            .map(|index| fixture_project(&format!("project-{index:05}")))
            .collect::<Vec<_>>();

        save_projects_to_path(&path, &projects).unwrap();
        let loaded = load_projects_from_path(&path).unwrap().unwrap();

        assert_eq!(loaded.len(), projects.len());
        assert_eq!(loaded.first().unwrap().name, "project-00000");
        assert_eq!(loaded.last().unwrap().name, "project-09999");
        assert!(loaded.iter().all(|project| !project.search_key.is_empty()));

        std::fs::remove_dir_all(directory).unwrap();
    }

    fn test_directory(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "devhub-gpui-{label}-{}-{unique}",
            std::process::id()
        ))
    }
}
