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
    if !path.exists() {
        return Ok(None);
    }

    let raw = std::fs::read_to_string(&path)
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
    let cache = ProjectCache {
        version: CACHE_VERSION,
        projects: projects.to_vec(),
    };
    let raw =
        toml::to_string(&cache).map_err(|error| format!("serializing project cache: {error}"))?;
    crate::config::write_crash_safe(&path, raw.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Project, ProjectSource, ProjectType};
    use std::path::PathBuf;

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
}
