use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::persistence::{
    PersistenceEvent, PersistenceFailure, PersistenceOperation, PersistenceRecoverySource,
    PersistenceReport,
};
use crate::Project;

const TODOS_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TodoItem {
    pub text: String,
    #[serde(default)]
    pub done: bool,
}

impl TodoItem {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            done: false,
        }
    }
}

pub type TodoMap = BTreeMap<String, Vec<TodoItem>>;

#[derive(Debug, Default, Serialize, Deserialize)]
struct TodoFile {
    version: u32,
    #[serde(default)]
    projects: TodoMap,
}

pub fn todo_key(project: &Project) -> String {
    match project.source.host() {
        Some(host) => format!("ssh:{host}:{}", project.path.display()),
        None => format!("local:{}", project.path.display()),
    }
}

pub fn todos_path() -> Option<PathBuf> {
    crate::Config::config_dir().map(|dir| dir.join("todos.toml"))
}

pub fn load_todos() -> Result<PersistenceReport<TodoMap>, PersistenceFailure> {
    let path = todos_path().ok_or_else(|| {
        PersistenceFailure::other("cannot determine the devhub-gpui config directory")
    })?;
    load_todos_from_path_with_diagnostics(&path)
}

fn load_todos_from_path_with_diagnostics(
    path: &std::path::Path,
) -> Result<PersistenceReport<TodoMap>, PersistenceFailure> {
    let candidates =
        crate::persistence::read_candidates(path).map_err(PersistenceFailure::other)?;
    if candidates.is_empty() {
        return Ok(PersistenceReport::new(TodoMap::new()));
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
        let file: TodoFile = match toml::from_str(&contents) {
            Ok(file) => file,
            Err(error) => {
                parse_errors.push(format!(
                    "parsing {} {}: {error}",
                    candidate.kind.label(),
                    path.display()
                ));
                continue;
            }
        };

        if file.version != TODOS_VERSION {
            if candidate.kind == crate::persistence::CandidateKind::Live {
                return Ok(PersistenceReport::new(TodoMap::new()));
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
                    crate::persistence::PersistenceStore::Todos,
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
                store: crate::persistence::PersistenceStore::Todos,
                source,
            });
        }

        return Ok(PersistenceReport {
            value: file.projects,
            events,
        });
    }

    if parse_errors.is_empty() {
        Ok(PersistenceReport::new(TodoMap::new()))
    } else {
        Err(PersistenceFailure::other(parse_errors.join("; ")))
    }
}

pub fn save_project_todos(
    key: &str,
    items: &[TodoItem],
) -> Result<PersistenceReport<()>, PersistenceFailure> {
    let path = todos_path().ok_or_else(|| {
        PersistenceFailure::other("cannot determine the devhub-gpui config directory")
    })?;
    save_project_todos_to_path(&path, key, items)
}

fn save_project_todos_to_path(
    path: &std::path::Path,
    key: &str,
    items: &[TodoItem],
) -> Result<PersistenceReport<()>, PersistenceFailure> {
    // A failed load must not turn into a destructive overwrite of other
    // projects' items, so the read-modify-write propagates load errors.
    let load = load_todos_from_path_with_diagnostics(path)?;
    let mut map = load.value;
    if items.is_empty() {
        map.remove(key);
    } else {
        map.insert(key.to_string(), items.to_vec());
    }

    let file = TodoFile {
        version: TODOS_VERSION,
        projects: map,
    };
    let raw = toml::to_string(&file)
        .map_err(|error| PersistenceFailure::other(format!("serializing todos: {error}")))?;
    crate::persistence::write_recoverable_checked(path, raw.as_bytes(), || {
        if path.exists() {
            let existing = std::fs::read_to_string(path)
                .map_err(|error| format!("reading {} before save: {error}", path.display()))?;
            if let Ok(value) = toml::from_str::<toml::Value>(&existing) {
                let existing_version = value
                    .get("version")
                    .and_then(toml::Value::as_integer)
                    .unwrap_or_default();
                if existing_version > i64::from(TODOS_VERSION) {
                    return Err(format!(
                        "refusing to overwrite todos version {existing_version} at {}; this build supports version {TODOS_VERSION}",
                        path.display()
                    ));
                }
            }
        }
        Ok(())
    })
    .map_err(|error| {
        error.into_failure(
            crate::persistence::PersistenceStore::Todos,
            PersistenceOperation::Write,
        )
    })?;
    Ok(PersistenceReport {
        value: (),
        events: load.events,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Project, ProjectSource, ProjectType};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn fixture_project(name: &str, source: ProjectSource) -> Project {
        let mut project = Project {
            name: name.to_string(),
            path: PathBuf::from(format!(r"F:\fixtures\{name}")),
            source,
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
    fn todo_key_distinguishes_local_and_remote() {
        let local = fixture_project("alpha", ProjectSource::Local);
        let remote = fixture_project(
            "alpha",
            ProjectSource::Remote {
                name: "box".to_string(),
                host: "devbox".to_string(),
            },
        );
        let other_host = fixture_project(
            "alpha",
            ProjectSource::Remote {
                name: String::new(),
                host: "other".to_string(),
            },
        );

        let local_key = todo_key(&local);
        assert!(local_key.starts_with("local:"));
        assert!(todo_key(&remote).starts_with("ssh:devbox:"));
        assert_ne!(todo_key(&remote), local_key);
        assert_ne!(todo_key(&remote), todo_key(&other_host));
    }

    #[test]
    fn todos_roundtrip_through_toml() {
        let directory = test_directory("todos-roundtrip");
        let path = directory.join("todos.toml");
        let items = vec![TodoItem::new("wire the renderer"), {
            let mut item = TodoItem::new("done item");
            item.done = true;
            item
        }];

        save_project_todos_to_path(&path, "local:alpha", &items).unwrap();
        save_project_todos_to_path(&path, "ssh:box:beta", &[TodoItem::new("other project")])
            .unwrap();
        let loaded = load_todos_from_path_with_diagnostics(&path).unwrap();

        assert_eq!(loaded.value.len(), 2);
        assert_eq!(loaded.value["local:alpha"], items);
        assert_eq!(loaded.value["ssh:box:beta"].len(), 1);

        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn missing_file_loads_empty() {
        let directory = test_directory("todos-missing");
        let path = directory.join("todos.toml");

        let report = load_todos_from_path_with_diagnostics(&path).unwrap();

        assert!(report.value.is_empty());
        assert!(report.events.is_empty());
    }

    #[test]
    fn future_version_loads_empty_and_save_refuses() {
        let directory = test_directory("todos-future");
        let path = directory.join("todos.toml");
        std::fs::create_dir_all(&directory).unwrap();
        let future = b"version = 99\nprojects = {}\n";
        std::fs::write(&path, future).unwrap();

        assert!(load_todos_from_path_with_diagnostics(&path)
            .unwrap()
            .value
            .is_empty());
        let error =
            save_project_todos_to_path(&path, "local:alpha", &[TodoItem::new("x")]).unwrap_err();

        assert!(error
            .to_string()
            .contains("refusing to overwrite todos version 99"));
        assert_eq!(std::fs::read(&path).unwrap(), future);

        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn empty_items_remove_the_project_key() {
        let directory = test_directory("todos-remove");
        let path = directory.join("todos.toml");

        save_project_todos_to_path(&path, "local:alpha", &[TodoItem::new("x")]).unwrap();
        save_project_todos_to_path(&path, "local:alpha", &[]).unwrap();
        let loaded = load_todos_from_path_with_diagnostics(&path).unwrap();

        assert!(loaded.value.is_empty());

        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn malformed_live_is_restored_from_backup() {
        let directory = test_directory("todos-backup");
        let path = directory.join("todos.toml");
        std::fs::create_dir_all(&directory).unwrap();
        let backup_items = vec![TodoItem::new("last-known-good")];
        save_project_todos_to_path(&path, "local:alpha", &backup_items).unwrap();
        let backup = std::fs::read_to_string(&path).unwrap();
        std::fs::write(path.with_extension("bak"), backup.as_bytes()).unwrap();
        std::fs::write(&path, b"this is not valid = [toml").unwrap();

        let report = load_todos_from_path_with_diagnostics(&path).unwrap();

        assert_eq!(
            report.events,
            [PersistenceEvent::Recovered {
                store: crate::persistence::PersistenceStore::Todos,
                source: PersistenceRecoverySource::Backup,
            }]
        );
        assert_eq!(report.value["local:alpha"], backup_items);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), backup);

        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn corrupt_live_without_backup_blocks_save() {
        let directory = test_directory("todos-corrupt");
        let path = directory.join("todos.toml");
        std::fs::create_dir_all(&directory).unwrap();
        let corrupt = b"this is not valid = [toml";
        std::fs::write(&path, corrupt).unwrap();

        let error =
            save_project_todos_to_path(&path, "local:alpha", &[TodoItem::new("x")]).unwrap_err();

        assert!(error.to_string().contains("parsing"));
        assert_eq!(std::fs::read(&path).unwrap(), corrupt);

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
