use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TEMPORARY_SEQUENCE: AtomicU64 = AtomicU64::new(0);
const NO_FAULT: usize = usize::MAX;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PersistenceStore {
    Config,
    ProjectCache,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PersistenceRecoverySource {
    Backup,
    Temporary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PersistenceOperation {
    Recovery,
    Write,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PersistenceEvent {
    Recovered {
        store: PersistenceStore,
        source: PersistenceRecoverySource,
    },
    Conflict {
        store: PersistenceStore,
        operation: PersistenceOperation,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistenceReport<T> {
    pub value: T,
    pub events: Vec<PersistenceEvent>,
}

impl<T> PersistenceReport<T> {
    pub(crate) fn new(value: T) -> Self {
        Self {
            value,
            events: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistenceFailure {
    message: String,
    event: Option<PersistenceEvent>,
}

impl PersistenceFailure {
    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn event(&self) -> Option<&PersistenceEvent> {
        self.event.as_ref()
    }

    pub(crate) fn other(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            event: None,
        }
    }

    pub(crate) fn context(mut self, context: impl std::fmt::Display) -> Self {
        self.message = format!("{context}: {}", self.message);
        self
    }
}

impl std::fmt::Display for PersistenceFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for PersistenceFailure {}

#[derive(Debug)]
pub(crate) enum PersistenceError {
    Conflict(String),
    Other(String),
}

impl PersistenceError {
    pub(crate) fn into_failure(
        self,
        store: PersistenceStore,
        operation: PersistenceOperation,
    ) -> PersistenceFailure {
        match self {
            Self::Conflict(message) => PersistenceFailure {
                message,
                event: Some(PersistenceEvent::Conflict { store, operation }),
            },
            Self::Other(message) => PersistenceFailure::other(message),
        }
    }
}

impl std::fmt::Display for PersistenceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Conflict(message) | Self::Other(message) => formatter.write_str(message),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CandidateKind {
    Live,
    Backup,
    Temporary,
}

impl CandidateKind {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Live => "live file",
            Self::Backup => "backup",
            Self::Temporary => "temporary file",
        }
    }
}

#[derive(Debug)]
pub(crate) struct Candidate {
    pub(crate) kind: CandidateKind,
    pub(crate) contents: Result<String, String>,
}

pub(crate) fn backup_path(path: &Path) -> PathBuf {
    path.with_extension("bak")
}

pub(crate) fn temporary_path(path: &Path) -> PathBuf {
    path.with_extension("tmp")
}

pub(crate) fn read_candidates(path: &Path) -> Result<Vec<Candidate>, String> {
    let mut candidates = Vec::new();
    push_candidate(&mut candidates, CandidateKind::Live, path.to_path_buf());
    push_candidate(&mut candidates, CandidateKind::Backup, backup_path(path));

    let mut temporary_paths = discover_temporary_paths(path)?;
    temporary_paths.sort_by_key(|candidate_path| {
        fs::metadata(candidate_path)
            .and_then(|metadata| metadata.modified())
            .ok()
    });
    temporary_paths.reverse();
    for temporary in temporary_paths {
        push_candidate(&mut candidates, CandidateKind::Temporary, temporary);
    }

    Ok(candidates)
}

#[cfg(test)]
pub(crate) fn write_recoverable(path: &Path, contents: &[u8]) -> Result<(), String> {
    write_recoverable_checked(path, contents, || Ok(())).map_err(|error| error.to_string())
}

pub(crate) fn write_recoverable_checked(
    path: &Path,
    contents: &[u8],
    validate_existing: impl FnOnce() -> Result<(), String>,
) -> Result<(), PersistenceError> {
    let parent = ensure_parent(path).map_err(PersistenceError::Other)?;
    let _lock = WriteLock::acquire(path)?;
    validate_existing().map_err(PersistenceError::Other)?;
    write_recoverable_locked(path, parent, contents, NO_FAULT).map_err(PersistenceError::Other)
}

pub(crate) fn restore_recovered(
    path: &Path,
    expected_live: Option<&str>,
    contents: &[u8],
) -> Result<(), PersistenceError> {
    let parent = ensure_parent(path).map_err(PersistenceError::Other)?;
    let _lock = WriteLock::acquire(path)?;
    verify_live_snapshot(path, expected_live)?;
    let temporary = write_unique_temporary(path, contents).map_err(PersistenceError::Other)?;

    if path.exists() {
        fs::remove_file(path).map_err(|error| {
            PersistenceError::Other(format!("removing invalid {}: {error}", path.display()))
        })?;
    }

    fs::rename(&temporary, path).map_err(|error| {
        PersistenceError::Other(format!("restoring {}: {error}", path.display()))
    })?;
    sync_parent_if_supported(parent);

    for temporary_path in discover_temporary_paths(path).unwrap_or_default() {
        let _ = fs::remove_file(temporary_path);
    }
    Ok(())
}

fn verify_live_snapshot(path: &Path, expected_live: Option<&str>) -> Result<(), PersistenceError> {
    match (fs::read_to_string(path), expected_live) {
        (Ok(current), Some(expected)) if current == expected => Ok(()),
        (Err(error), None) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        (Ok(_), _) | (Err(_), Some(_)) => Err(PersistenceError::Conflict(format!(
            "{} changed while recovery was waiting for the writer lock; no file was modified",
            path.display()
        ))),
        (Err(error), None) => Err(PersistenceError::Other(format!(
            "reading {} during recovery: {error}",
            path.display()
        ))),
    }
}

fn push_candidate(candidates: &mut Vec<Candidate>, kind: CandidateKind, path: PathBuf) {
    match fs::read_to_string(&path) {
        Ok(contents) => candidates.push(Candidate {
            kind,
            contents: Ok(contents),
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => candidates.push(Candidate {
            kind,
            contents: Err(format!("reading {}: {error}", path.display())),
        }),
    }
}

fn discover_temporary_paths(path: &Path) -> Result<Vec<PathBuf>, String> {
    let canonical = temporary_path(path);
    let Some(parent) = path.parent() else {
        return Err(format!("{} has no parent directory", path.display()));
    };
    let Some(canonical_name) = canonical.file_name() else {
        return Err(format!("{} has no file name", canonical.display()));
    };
    let prefix = format!("{}.", canonical_name.to_string_lossy());
    let mut paths = Vec::new();

    if canonical.exists() {
        paths.push(canonical);
    }

    let entries = match fs::read_dir(parent) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(paths),
        Err(error) => return Err(format!("reading {}: {error}", parent.display())),
    };
    for entry in entries {
        let entry = entry.map_err(|error| format!("reading {}: {error}", parent.display()))?;
        if entry.file_name().to_string_lossy().starts_with(&prefix) {
            paths.push(entry.path());
        }
    }

    Ok(paths)
}

fn write_recoverable_locked(
    path: &Path,
    parent: &Path,
    contents: &[u8],
    fault_after: usize,
) -> Result<(), String> {
    let temporary = write_unique_temporary(path, contents)?;
    inject_fault(1, fault_after)?;
    let backup = backup_path(path);

    if path.exists() {
        if backup.exists() {
            fs::remove_file(&backup)
                .map_err(|error| format!("removing {}: {error}", backup.display()))?;
        }
        inject_fault(2, fault_after)?;
        fs::rename(path, &backup)
            .map_err(|error| format!("backing up {}: {error}", path.display()))?;
        inject_fault(3, fault_after)?;
    }

    if let Err(error) = fs::rename(&temporary, path) {
        if !path.exists() && backup.exists() {
            let _ = fs::rename(&backup, path);
        }
        return Err(format!("replacing {}: {error}", path.display()));
    }
    inject_fault(4, fault_after)?;

    sync_parent_if_supported(parent);
    Ok(())
}

fn ensure_parent(path: &Path) -> Result<&Path, String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("{} has no parent directory", path.display()))?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("creating {}: {error}", parent.display()))?;
    Ok(parent)
}

fn write_unique_temporary(path: &Path, contents: &[u8]) -> Result<PathBuf, String> {
    for _ in 0..100 {
        let temporary = unique_temporary_path(path);
        let mut file = match OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!("opening {}: {error}", temporary.display()));
            }
        };
        file.write_all(contents)
            .map_err(|error| format!("writing {}: {error}", temporary.display()))?;
        file.sync_all()
            .map_err(|error| format!("syncing {}: {error}", temporary.display()))?;
        drop(file);
        return Ok(temporary);
    }

    Err(format!(
        "could not allocate a unique temporary file for {}",
        path.display()
    ))
}

fn unique_temporary_path(path: &Path) -> PathBuf {
    let mut temporary = temporary_path(path).into_os_string();
    let sequence = TEMPORARY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    temporary.push(format!(".{}.{sequence}", std::process::id()));
    PathBuf::from(temporary)
}

fn inject_fault(step: usize, fault_after: usize) -> Result<(), String> {
    if step == fault_after {
        Err(format!("injected persistence fault after step {step}"))
    } else {
        Ok(())
    }
}

struct WriteLock {
    _file: File,
}

impl WriteLock {
    fn acquire(path: &Path) -> Result<Self, PersistenceError> {
        let lock_path = path.with_extension("lock");
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&lock_path)
            .map_err(|error| {
                PersistenceError::Other(format!("opening {}: {error}", lock_path.display()))
            })?;
        match file.try_lock() {
            Ok(()) => Ok(Self { _file: file }),
            Err(TryLockError::WouldBlock) => Err(PersistenceError::Conflict(format!(
                "another process is writing {}; no file was modified",
                path.display()
            ))),
            Err(TryLockError::Error(error)) => Err(PersistenceError::Other(format!(
                "locking {}: {error}",
                lock_path.display()
            ))),
        }
    }
}

#[cfg(unix)]
fn sync_parent_if_supported(parent: &Path) {
    if let Ok(directory) = File::open(parent) {
        let _ = directory.sync_all();
    }
}

#[cfg(not(unix))]
fn sync_parent_if_supported(_parent: &Path) {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn every_replacement_boundary_retains_a_valid_candidate() {
        for fault_after in 1..=4 {
            let directory = test_directory(&format!("fault-{fault_after}"));
            let path = directory.join("state.toml");
            fs::create_dir_all(&directory).unwrap();
            fs::write(&path, "version = 1\nvalue = \"old\"\n").unwrap();
            fs::write(backup_path(&path), "version = 1\nvalue = \"older\"\n").unwrap();
            let parent = path.parent().unwrap();
            let lock = WriteLock::acquire(&path).unwrap();

            let error = write_recoverable_locked(
                &path,
                parent,
                b"version = 1\nvalue = \"new\"\n",
                fault_after,
            )
            .unwrap_err();

            assert!(error.contains("injected persistence fault"));
            let candidates = read_candidates(&path).unwrap();
            assert!(candidates.iter().any(|candidate| {
                candidate.contents.as_ref().is_ok_and(|contents| {
                    toml::from_str::<toml::Value>(contents).is_ok()
                        && (contents.contains("value = \"old\"")
                            || contents.contains("value = \"older\"")
                            || contents.contains("value = \"new\""))
                })
            }));

            drop(lock);
            fs::remove_dir_all(directory).unwrap();
        }
    }

    #[test]
    fn a_contending_writer_fails_without_modifying_live_data() {
        let directory = test_directory("writer-conflict");
        let path = directory.join("state.toml");
        fs::create_dir_all(&directory).unwrap();
        fs::write(&path, b"original").unwrap();
        let lock = WriteLock::acquire(&path).unwrap();

        let error = write_recoverable(&path, b"replacement").unwrap_err();

        assert!(error.contains("another process is writing"));
        assert_eq!(fs::read(&path).unwrap(), b"original");
        assert!(discover_temporary_paths(&path).unwrap().is_empty());

        drop(lock);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn recovery_rejects_a_live_file_created_after_candidate_selection() {
        let directory = test_directory("recovery-conflict");
        let path = directory.join("state.toml");
        fs::create_dir_all(&directory).unwrap();
        fs::write(&path, b"created-by-another-writer").unwrap();

        let error = restore_recovered(&path, None, b"stale-recovery").unwrap_err();

        assert!(error
            .to_string()
            .contains("changed while recovery was waiting"));
        assert_eq!(fs::read(&path).unwrap(), b"created-by-another-writer");
        assert!(discover_temporary_paths(&path).unwrap().is_empty());

        fs::remove_dir_all(directory).unwrap();
    }

    fn test_directory(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "devhub-gpui-persistence-{label}-{}-{unique}",
            std::process::id()
        ))
    }
}
