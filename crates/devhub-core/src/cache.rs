use serde::{Deserialize, Serialize};

use crate::persistence::{
    PersistenceEvent, PersistenceFailure, PersistenceOperation, PersistenceRecoverySource,
    PersistenceReport, PersistenceStore,
};
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
    load_projects_with_diagnostics()
        .map(|report| report.value)
        .map_err(|error| error.to_string())
}

pub fn load_projects_with_diagnostics(
) -> Result<PersistenceReport<Option<Vec<Project>>>, PersistenceFailure> {
    let path = cache_path().ok_or_else(|| {
        PersistenceFailure::other("cannot determine the devhub-gpui cache directory")
    })?;
    load_projects_from_path_with_diagnostics(&path)
}

#[cfg(test)]
fn load_projects_from_path(path: &std::path::Path) -> Result<Option<Vec<Project>>, String> {
    load_projects_from_path_with_diagnostics(path)
        .map(|report| report.value)
        .map_err(|error| error.to_string())
}

fn load_projects_from_path_with_diagnostics(
    path: &std::path::Path,
) -> Result<PersistenceReport<Option<Vec<Project>>>, PersistenceFailure> {
    let candidates =
        crate::persistence::read_candidates(path).map_err(PersistenceFailure::other)?;
    if candidates.is_empty() {
        return Ok(PersistenceReport::new(None));
    }

    let live_snapshot = candidates
        .first()
        .filter(|candidate| candidate.kind == crate::persistence::CandidateKind::Live)
        .and_then(|candidate| candidate.contents.as_ref().ok())
        .cloned();
    let mut parse_errors = Vec::new();
    for candidate in candidates {
        let contents = match candidate.contents {
            Ok(contents) => contents,
            Err(error) if candidate.kind == crate::persistence::CandidateKind::Live => {
                return Err(PersistenceFailure::other(error));
            }
            Err(error) => {
                parse_errors.push(error);
                continue;
            }
        };
        let cache: ProjectCache = match toml::from_str(&contents) {
            Ok(cache) => cache,
            Err(error) => {
                parse_errors.push(format!(
                    "parsing {} {}: {error}",
                    candidate.kind.label(),
                    path.display()
                ));
                continue;
            }
        };

        if cache.version != CACHE_VERSION {
            if candidate.kind == crate::persistence::CandidateKind::Live {
                return Ok(PersistenceReport::new(None));
            }
            continue;
        }

        let mut events = Vec::new();
        if candidate.kind != crate::persistence::CandidateKind::Live {
            crate::persistence::restore_recovered(
                path,
                live_snapshot.as_deref(),
                contents.as_bytes(),
            )
            .map_err(|error| {
                error.into_failure(
                    PersistenceStore::ProjectCache,
                    PersistenceOperation::Recovery,
                )
            })?;
            let source = match candidate.kind {
                crate::persistence::CandidateKind::Backup => PersistenceRecoverySource::Backup,
                crate::persistence::CandidateKind::Temporary => {
                    PersistenceRecoverySource::Temporary
                }
                crate::persistence::CandidateKind::Live => unreachable!(),
            };
            events.push(PersistenceEvent::Recovered {
                store: PersistenceStore::ProjectCache,
                source,
            });
        }

        let mut projects = cache.projects;
        for project in &mut projects {
            project.refresh_search_key();
        }
        crate::sort_projects(&mut projects);

        return Ok(PersistenceReport {
            value: Some(projects),
            events,
        });
    }

    if parse_errors.is_empty() {
        Ok(PersistenceReport::new(None))
    } else {
        Err(PersistenceFailure::other(parse_errors.join("; ")))
    }
}

pub fn save_projects(projects: &[Project]) -> Result<(), String> {
    save_projects_with_diagnostics(projects)
        .map(|report| report.value)
        .map_err(|error| error.to_string())
}

pub fn save_projects_with_diagnostics(
    projects: &[Project],
) -> Result<PersistenceReport<()>, PersistenceFailure> {
    let path = cache_path().ok_or_else(|| {
        PersistenceFailure::other("cannot determine the devhub-gpui cache directory")
    })?;
    save_projects_to_path_with_diagnostics(&path, projects)
}

#[cfg(test)]
fn save_projects_to_path(path: &std::path::Path, projects: &[Project]) -> Result<(), String> {
    save_projects_to_path_with_diagnostics(path, projects)
        .map(|report| report.value)
        .map_err(|error| error.to_string())
}

fn save_projects_to_path_with_diagnostics(
    path: &std::path::Path,
    projects: &[Project],
) -> Result<PersistenceReport<()>, PersistenceFailure> {
    let cache = ProjectCache {
        version: CACHE_VERSION,
        projects: projects.to_vec(),
    };
    let raw = toml::to_string(&cache).map_err(|error| {
        PersistenceFailure::other(format!("serializing project cache: {error}"))
    })?;
    crate::persistence::write_recoverable_checked(path, raw.as_bytes(), || {
        if path.exists() {
            let existing = std::fs::read_to_string(path)
                .map_err(|error| format!("reading {} before save: {error}", path.display()))?;
            if let Ok(value) = toml::from_str::<toml::Value>(&existing) {
                let existing_version = value
                    .get("version")
                    .and_then(toml::Value::as_integer)
                    .unwrap_or_default();
                if existing_version > i64::from(CACHE_VERSION) {
                    return Err(format!(
                        "refusing to overwrite project cache version {existing_version} at {}; this build supports version {CACHE_VERSION}",
                        path.display()
                    ));
                }
            }
            }
            Ok(())
        })
        .map_err(|error| {
            error.into_failure(
                PersistenceStore::ProjectCache,
                PersistenceOperation::Write,
            )
        })?;
    Ok(PersistenceReport::new(()))
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
        let save_error = save_projects_to_path(&path, &[fixture_project("older")]).unwrap_err();

        assert!(save_error.contains("refusing to overwrite project cache version 99"));
        assert_eq!(std::fs::read(&path).unwrap(), future);

        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn missing_live_cache_is_restored_from_backup() {
        let directory = test_directory("missing-live-cache");
        let path = directory.join("projects.toml");
        std::fs::create_dir_all(&directory).unwrap();
        let backup = toml::to_string(&ProjectCache {
            version: CACHE_VERSION,
            projects: vec![fixture_project("recovered")],
        })
        .unwrap();
        std::fs::write(path.with_extension("bak"), backup.as_bytes()).unwrap();

        let report = load_projects_from_path_with_diagnostics(&path).unwrap();
        assert_eq!(
            report.events,
            [PersistenceEvent::Recovered {
                store: PersistenceStore::ProjectCache,
                source: PersistenceRecoverySource::Backup,
            }]
        );
        let projects = report.value.unwrap();

        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].name, "recovered");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), backup);
        assert_eq!(
            std::fs::read_to_string(path.with_extension("bak")).unwrap(),
            backup
        );

        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn malformed_live_cache_is_restored_from_backup() {
        let directory = test_directory("malformed-live-cache");
        let path = directory.join("projects.toml");
        std::fs::create_dir_all(&directory).unwrap();
        let backup = toml::to_string(&ProjectCache {
            version: CACHE_VERSION,
            projects: vec![fixture_project("last-known-good")],
        })
        .unwrap();
        std::fs::write(&path, b"this is not valid = [toml").unwrap();
        std::fs::write(path.with_extension("bak"), backup.as_bytes()).unwrap();

        let report = load_projects_from_path_with_diagnostics(&path).unwrap();
        assert_eq!(
            report.events,
            [PersistenceEvent::Recovered {
                store: PersistenceStore::ProjectCache,
                source: PersistenceRecoverySource::Backup,
            }]
        );
        let projects = report.value.unwrap();

        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].name, "last-known-good");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), backup);
        assert_eq!(
            std::fs::read_to_string(path.with_extension("bak")).unwrap(),
            backup
        );

        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn future_live_cache_blocks_recovery_from_older_backup() {
        let directory = test_directory("future-live-cache");
        let path = directory.join("projects.toml");
        std::fs::create_dir_all(&directory).unwrap();
        let future = b"version = 99\nprojects = []\n";
        let backup = toml::to_string(&ProjectCache {
            version: CACHE_VERSION,
            projects: vec![fixture_project("older")],
        })
        .unwrap();
        std::fs::write(&path, future).unwrap();
        std::fs::write(path.with_extension("bak"), backup.as_bytes()).unwrap();

        assert!(load_projects_from_path(&path).unwrap().is_none());
        assert_eq!(std::fs::read(&path).unwrap(), future);
        assert_eq!(
            std::fs::read_to_string(path.with_extension("bak")).unwrap(),
            backup
        );

        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn missing_live_cache_is_restored_from_unique_temporary_file() {
        let directory = test_directory("unique-temp-cache");
        let path = directory.join("projects.toml");
        let temporary = directory.join("projects.tmp.4242.1");
        std::fs::create_dir_all(&directory).unwrap();
        let recoverable = toml::to_string(&ProjectCache {
            version: CACHE_VERSION,
            projects: vec![fixture_project("temporary")],
        })
        .unwrap();
        std::fs::write(&temporary, recoverable.as_bytes()).unwrap();

        let report = load_projects_from_path_with_diagnostics(&path).unwrap();
        assert_eq!(
            report.events,
            [PersistenceEvent::Recovered {
                store: PersistenceStore::ProjectCache,
                source: PersistenceRecoverySource::Temporary,
            }]
        );
        let projects = report.value.unwrap();

        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].name, "temporary");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), recoverable);
        assert!(!temporary.exists());

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
