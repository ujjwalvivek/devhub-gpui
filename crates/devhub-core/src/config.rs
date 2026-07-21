use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::persistence::{
    PersistenceEvent, PersistenceFailure, PersistenceOperation, PersistenceRecoverySource,
    PersistenceReport, PersistenceStore,
};

const APP_QUALIFIER: &str = "";
const APP_ORGANIZATION: &str = "";
const APP_IDENTITY: &str = "devhub-gpui";
const CONFIG_VERSION: u32 = 1;
const STATE_DIR_ENV: &str = "DEVHUB_GPUI_STATE_DIR";

fn default_max_depth() -> usize {
    3
}

fn default_mcp_http_port() -> u16 {
    47821
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ThemeId {
    CatppuccinMocha,
    RosePineMoon,
    TokyoNightStorm,
    HorizonBold,
    #[default]
    MonochromeZero,
}

impl ThemeId {
    pub const ALL: [Self; 5] = [
        Self::CatppuccinMocha,
        Self::RosePineMoon,
        Self::TokyoNightStorm,
        Self::HorizonBold,
        Self::MonochromeZero,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::CatppuccinMocha => "Catppuccin",
            Self::RosePineMoon => "Rose Pine",
            Self::TokyoNightStorm => "Tokyo Night",
            Self::HorizonBold => "Horizon",
            Self::MonochromeZero => "Monochrome",
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AppearanceMode {
    System,
    #[default]
    Dark,
    Light,
}

impl AppearanceMode {
    pub const ALL: [Self; 3] = [Self::System, Self::Dark, Self::Light];

    pub fn label(self) -> &'static str {
        match self {
            Self::System => "System",
            Self::Dark => "Dark",
            Self::Light => "Light",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    version: u32,

    #[serde(default)]
    pub theme: ThemeId,

    #[serde(default)]
    pub appearance: AppearanceMode,

    #[serde(default)]
    pub scan_dirs: Vec<PathBuf>,

    #[serde(default = "default_max_depth")]
    pub max_depth: usize,

    #[serde(default)]
    pub remote_hosts: Vec<RemoteHostConfig>,

    #[serde(default)]
    pub pinned_projects: Vec<PathBuf>,

    #[serde(default)]
    pub hidden_projects: Vec<PathBuf>,

    #[serde(default)]
    pub last_project: Option<ProjectLocator>,

    #[serde(default = "default_mcp_http_port")]
    pub mcp_http_port: u16,

    #[serde(default)]
    pub mcp_auth_token: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectLocator {
    pub path: PathBuf,
    #[serde(default)]
    pub remote_host: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteHostConfig {
    #[serde(default)]
    pub name: String,
    pub host: String,
    #[serde(default)]
    pub roots: Vec<String>,
    #[serde(default = "default_max_depth")]
    pub max_depth: usize,
}

impl RemoteHostConfig {
    pub fn label(&self) -> &str {
        if self.name.trim().is_empty() {
            &self.host
        } else {
            &self.name
        }
    }

    pub fn normalize(&mut self) {
        self.name = self.name.trim().to_string();
        self.host = normalize_ssh_host(&self.host);
        self.roots = self
            .roots
            .drain(..)
            .map(|root| root.trim().replace('\\', "/"))
            .filter(|root| !root.is_empty())
            .fold(Vec::new(), |mut roots, root| {
                if !roots.contains(&root) {
                    roots.push(root);
                }
                roots
            });
        self.max_depth = self.max_depth.clamp(1, 20);
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            theme: ThemeId::default(),
            appearance: AppearanceMode::default(),
            scan_dirs: Vec::new(),
            max_depth: default_max_depth(),
            remote_hosts: Vec::new(),
            pinned_projects: Vec::new(),
            hidden_projects: Vec::new(),
            last_project: None,
            mcp_http_port: default_mcp_http_port(),
            mcp_auth_token: None,
        }
    }
}

impl Config {
    pub fn config_dir() -> Option<PathBuf> {
        if let Some(root) = state_dir_override() {
            return Some(root.join("config"));
        }
        directories::ProjectDirs::from(APP_QUALIFIER, APP_ORGANIZATION, APP_IDENTITY)
            .map(|dirs| dirs.config_dir().to_path_buf())
    }

    pub fn cache_dir() -> Option<PathBuf> {
        if let Some(root) = state_dir_override() {
            return Some(root.join("cache"));
        }
        directories::ProjectDirs::from(APP_QUALIFIER, APP_ORGANIZATION, APP_IDENTITY)
            .map(|dirs| dirs.cache_dir().to_path_buf())
    }

    pub fn config_path() -> Option<PathBuf> {
        Self::config_dir().map(|dir| dir.join("config.toml"))
    }

    pub fn load_or_create() -> Result<Self, String> {
        Self::load_or_create_with_diagnostics()
            .map(|report| report.value)
            .map_err(|error| error.to_string())
    }

    pub fn load_or_create_with_diagnostics() -> Result<PersistenceReport<Self>, PersistenceFailure>
    {
        let path = Self::config_path().ok_or_else(|| {
            PersistenceFailure::other("cannot determine the devhub-gpui config directory")
        })?;

        Self::load_or_create_at_with_diagnostics(&path)
    }

    #[cfg(test)]
    fn load_or_create_at(path: &std::path::Path) -> Result<Self, String> {
        Self::load_or_create_at_with_diagnostics(path)
            .map(|report| report.value)
            .map_err(|error| error.to_string())
    }

    fn load_or_create_at_with_diagnostics(
        path: &std::path::Path,
    ) -> Result<PersistenceReport<Self>, PersistenceFailure> {
        if crate::persistence::read_candidates(path)
            .map_err(PersistenceFailure::other)?
            .is_empty()
        {
            let config = Self::default();
            let report = config.save_to_path_with_diagnostics(path)?;
            Ok(PersistenceReport {
                value: config,
                events: report.events,
            })
        } else {
            Self::load_from_path_with_diagnostics(path)
        }
    }

    pub fn save(&self) -> Result<(), String> {
        self.save_with_diagnostics()
            .map(|report| report.value)
            .map_err(|error| error.to_string())
    }

    pub fn save_with_diagnostics(&self) -> Result<PersistenceReport<()>, PersistenceFailure> {
        let path = Self::config_path().ok_or_else(|| {
            PersistenceFailure::other("cannot determine the devhub-gpui config directory")
        })?;
        self.save_to_path_with_diagnostics(&path)
    }

    #[cfg(test)]
    fn load_from_path(path: &std::path::Path) -> Result<Self, String> {
        Self::load_from_path_with_diagnostics(path)
            .map(|report| report.value)
            .map_err(|error| error.to_string())
    }

    fn load_from_path_with_diagnostics(
        path: &std::path::Path,
    ) -> Result<PersistenceReport<Self>, PersistenceFailure> {
        let candidates =
            crate::persistence::read_candidates(path).map_err(PersistenceFailure::other)?;
        if candidates.is_empty() {
            return Err(PersistenceFailure::other(format!(
                "reading {}: file not found",
                path.display()
            )));
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
            let mut config: Self = match toml::from_str(&contents) {
                Ok(config) => config,
                Err(error) => {
                    parse_errors.push(format!(
                        "parsing {} {}: {error}",
                        candidate.kind.label(),
                        path.display()
                    ));
                    continue;
                }
            };

            if config.version > CONFIG_VERSION {
                return Err(PersistenceFailure::other(format!(
                    "config version {} in the {} at {} is newer than supported version {CONFIG_VERSION}; no file was modified",
                    config.version,
                    candidate.kind.label(),
                    path.display()
                )));
            }

            let mut events = Vec::new();
            if candidate.kind != crate::persistence::CandidateKind::Live {
                crate::persistence::restore_recovered(
                    path,
                    live_snapshot.as_deref(),
                    contents.as_bytes(),
                )
                .map_err(|error| {
                    error.into_failure(PersistenceStore::Config, PersistenceOperation::Recovery)
                })?;
                let source = match candidate.kind {
                    crate::persistence::CandidateKind::Backup => PersistenceRecoverySource::Backup,
                    crate::persistence::CandidateKind::Temporary => {
                        PersistenceRecoverySource::Temporary
                    }
                    crate::persistence::CandidateKind::Live => unreachable!(),
                };
                events.push(PersistenceEvent::Recovered {
                    store: PersistenceStore::Config,
                    source,
                });
            }

            match config.version {
                CONFIG_VERSION => {
                    config.normalize();
                    return Ok(PersistenceReport {
                        value: config,
                        events,
                    });
                }
                0 => {
                    config.version = CONFIG_VERSION;
                    config.normalize();
                    let migration =
                        config
                            .save_to_path_with_diagnostics(path)
                            .map_err(|error| {
                                error.context(format!(
                                    "migrating legacy config at {}",
                                    path.display()
                                ))
                            })?;
                    events.extend(migration.events);
                    return Ok(PersistenceReport {
                        value: config,
                        events,
                    });
                }
                version => {
                    parse_errors.push(format!(
                        "unsupported config version {version} in the {} at {}",
                        candidate.kind.label(),
                        path.display()
                    ));
                }
            }
        }

        Err(PersistenceFailure::other(parse_errors.join("; ")))
    }

    #[cfg(test)]
    fn save_to_path(&self, path: &std::path::Path) -> Result<(), String> {
        self.save_to_path_with_diagnostics(path)
            .map(|report| report.value)
            .map_err(|error| error.to_string())
    }

    fn save_to_path_with_diagnostics(
        &self,
        path: &std::path::Path,
    ) -> Result<PersistenceReport<()>, PersistenceFailure> {
        let serialized = toml::to_string_pretty(self)
            .map_err(|error| PersistenceFailure::other(format!("serializing config: {error}")))?;
        crate::persistence::write_recoverable_checked(path, serialized.as_bytes(), || {
            if path.exists() {
                let existing = std::fs::read_to_string(path)
                    .map_err(|error| format!("reading {} before save: {error}", path.display()))?;
                if let Ok(value) = toml::from_str::<toml::Value>(&existing) {
                    let existing_version = value
                        .get("version")
                        .and_then(toml::Value::as_integer)
                        .unwrap_or_default();
                    if existing_version > i64::from(CONFIG_VERSION) {
                        return Err(format!(
                            "refusing to overwrite config version {existing_version} at {}; this build supports version {CONFIG_VERSION}",
                            path.display()
                        ));
                    }
                }
            }
            Ok(())
        })
        .map_err(|error| {
            error.into_failure(PersistenceStore::Config, PersistenceOperation::Write)
        })?;
        Ok(PersistenceReport::new(()))
    }

    pub fn normalize(&mut self) {
        self.max_depth = self.max_depth.clamp(1, 20);
        for host in &mut self.remote_hosts {
            host.normalize();
        }
        self.remote_hosts
            .retain(|host| !host.host.is_empty() && !host.roots.is_empty());
        let mut merged = Vec::<RemoteHostConfig>::new();
        for host in self.remote_hosts.drain(..) {
            if let Some(existing) = merged.iter_mut().find(|item| item.host == host.host) {
                if !host.name.is_empty() {
                    existing.name = host.name;
                }
                for root in host.roots {
                    if !existing.roots.contains(&root) {
                        existing.roots.push(root);
                    }
                }
                existing.max_depth = host.max_depth;
            } else {
                merged.push(host);
            }
        }
        self.remote_hosts = merged;
    }

    pub fn ensure_dirs_exist() -> Result<(), String> {
        if let Some(config_dir) = Self::config_dir() {
            std::fs::create_dir_all(&config_dir)
                .map_err(|error| format!("creating {}: {error}", config_dir.display()))?;
        }
        if let Some(cache_dir) = Self::cache_dir() {
            std::fs::create_dir_all(&cache_dir)
                .map_err(|error| format!("creating {}: {error}", cache_dir.display()))?;
        }
        Ok(())
    }
}

fn state_dir_override() -> Option<PathBuf> {
    std::env::var_os(STATE_DIR_ENV)
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
}

pub fn normalize_ssh_host(raw: &str) -> String {
    raw.trim()
        .strip_prefix("ssh ")
        .map(str::trim)
        .unwrap_or_else(|| raw.trim())
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn default_config_has_sensible_scan_defaults() {
        let config = Config::default();
        assert_eq!(config.version, CONFIG_VERSION);
        assert!(config.scan_dirs.is_empty());
        assert_eq!(config.max_depth, 3);
    }

    #[test]
    fn config_roundtrips_through_toml() {
        let config = Config {
            version: CONFIG_VERSION,
            theme: ThemeId::TokyoNightStorm,
            appearance: AppearanceMode::System,
            scan_dirs: vec![PathBuf::from(r"C:\projects"), PathBuf::from(r"D:\code")],
            max_depth: 5,
            remote_hosts: vec![RemoteHostConfig {
                name: "build".into(),
                host: "dev@example.com".into(),
                roots: vec!["/srv/code".into()],
                max_depth: 4,
            }],
            pinned_projects: Vec::new(),
            hidden_projects: Vec::new(),
            last_project: Some(ProjectLocator {
                path: PathBuf::from("/srv/code/project"),
                remote_host: Some("dev@example.com".into()),
            }),
            mcp_http_port: default_mcp_http_port(),
            mcp_auth_token: None,
        };

        let serialized = toml::to_string_pretty(&config).unwrap();
        let deserialized: Config = toml::from_str(&serialized).unwrap();

        assert_eq!(deserialized.version, CONFIG_VERSION);
        assert!(serialized.contains("version = 1"));
        assert_eq!(deserialized.scan_dirs, config.scan_dirs);
        assert_eq!(deserialized.max_depth, 5);
        assert_eq!(deserialized.remote_hosts, config.remote_hosts);
        assert_eq!(deserialized.theme, ThemeId::TokyoNightStorm);
        assert_eq!(deserialized.appearance, AppearanceMode::System);
        assert_eq!(deserialized.last_project, config.last_project);
    }

    #[test]
    fn missing_fields_fall_back_to_defaults() {
        let raw = "";
        let config: Config = toml::from_str(raw).unwrap();
        assert_eq!(config.version, 0);
        assert!(config.scan_dirs.is_empty());
        assert_eq!(config.max_depth, 3);
        assert!(config.remote_hosts.is_empty());
        assert_eq!(config.theme, ThemeId::MonochromeZero);
        assert_eq!(config.appearance, AppearanceMode::Dark);
        assert!(config.last_project.is_none());
    }

    #[test]
    fn remote_hosts_are_normalized_and_merged() {
        let mut config = Config {
            remote_hosts: vec![
                RemoteHostConfig {
                    name: String::new(),
                    host: " ssh user@example.com ".into(),
                    roots: vec![" /srv/a ".into()],
                    max_depth: 0,
                },
                RemoteHostConfig {
                    name: "Example".into(),
                    host: "user@example.com".into(),
                    roots: vec!["/srv/b".into(), "/srv/a".into()],
                    max_depth: 30,
                },
            ],
            ..Config::default()
        };

        config.normalize();

        assert_eq!(config.remote_hosts.len(), 1);
        assert_eq!(config.remote_hosts[0].name, "Example");
        assert_eq!(config.remote_hosts[0].roots, ["/srv/a", "/srv/b"]);
        assert_eq!(config.remote_hosts[0].max_depth, 20);
    }

    #[test]
    fn legacy_devhub_identity_is_distinct_from_successor_identity() {
        let reference =
            directories::ProjectDirs::from("", "", "devhub").map(|d| d.config_dir().to_path_buf());
        let successor = Config::config_dir();

        if let (Some(reference), Some(successor)) = (reference, successor) {
            assert_ne!(
                reference, successor,
                "devhub-gpui config dir must not collide with egui devhub config dir"
            );
        }
    }

    #[test]
    fn recoverable_write_replaces_content_and_retains_last_known_good_backup() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "devhub-gpui-config-test-{}-{unique}",
            std::process::id()
        ));
        let path = directory.join("config.toml");

        crate::persistence::write_recoverable(&path, b"first").unwrap();
        crate::persistence::write_recoverable(&path, b"second").unwrap();

        assert_eq!(std::fs::read(&path).unwrap(), b"second");
        assert!(!path.with_extension("tmp").exists());
        assert_eq!(std::fs::read(path.with_extension("bak")).unwrap(), b"first");

        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn legacy_config_is_migrated_to_version_one_atomically() {
        let directory = test_directory("legacy-migration");
        let path = directory.join("config.toml");
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            &path,
            "theme = \"tokyo-night-storm\"\nappearance = \"system\"\nmax_depth = 7\n",
        )
        .unwrap();

        let config = Config::load_from_path(&path).unwrap();
        let migrated = std::fs::read_to_string(&path).unwrap();

        assert_eq!(config.version, CONFIG_VERSION);
        assert_eq!(config.theme, ThemeId::TokyoNightStorm);
        assert_eq!(config.appearance, AppearanceMode::System);
        assert_eq!(config.max_depth, 7);
        assert!(migrated.contains("version = 1"));
        assert!(!path.with_extension("tmp").exists());
        assert!(path.with_extension("bak").exists());

        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn future_config_version_is_rejected_without_rewriting() {
        let directory = test_directory("future-version");
        let path = directory.join("config.toml");
        std::fs::create_dir_all(&directory).unwrap();
        let original = b"version = 99\nmax_depth = 12\n";
        std::fs::write(&path, original).unwrap();

        let error = Config::load_from_path(&path).unwrap_err();
        let save_error = Config::default().save_to_path(&path).unwrap_err();

        assert!(error.contains("newer than supported version 1"));
        assert!(save_error.contains("refusing to overwrite config version 99"));
        assert_eq!(std::fs::read(&path).unwrap(), original);
        assert!(!path.with_extension("tmp").exists());
        assert!(!path.with_extension("bak").exists());

        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn clean_install_creates_an_empty_versioned_config() {
        let directory = test_directory("clean-install");
        let path = directory.join("nested").join("config.toml");

        let report = Config::load_or_create_at_with_diagnostics(&path).unwrap();
        assert!(report.events.is_empty());
        let config = report.value;
        let serialized = std::fs::read_to_string(&path).unwrap();

        assert_eq!(config.version, CONFIG_VERSION);
        assert!(config.scan_dirs.is_empty());
        assert!(config.remote_hosts.is_empty());
        assert!(serialized.contains("version = 1"));
        assert!(!path.with_extension("tmp").exists());
        assert!(!path.with_extension("bak").exists());

        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn malformed_config_is_reported_without_modification() {
        let directory = test_directory("malformed");
        let path = directory.join("config.toml");
        std::fs::create_dir_all(&directory).unwrap();
        let malformed = b"version = [broken";
        std::fs::write(&path, malformed).unwrap();

        let error = Config::load_or_create_at(&path).unwrap_err();

        assert!(error.contains("parsing"));
        assert_eq!(std::fs::read(&path).unwrap(), malformed);

        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn missing_live_config_is_restored_from_backup_without_defaulting() {
        let directory = test_directory("missing-live-recovery");
        let path = directory.join("config.toml");
        std::fs::create_dir_all(&directory).unwrap();
        let backup =
            b"version = 1\ntheme = \"tokyo-night-storm\"\nappearance = \"system\"\nmax_depth = 9\n";
        std::fs::write(path.with_extension("bak"), backup).unwrap();
        std::fs::write(path.with_extension("tmp"), b"incomplete = [").unwrap();

        let report = Config::load_or_create_at_with_diagnostics(&path).unwrap();
        assert_eq!(
            report.events,
            [PersistenceEvent::Recovered {
                store: PersistenceStore::Config,
                source: PersistenceRecoverySource::Backup,
            }]
        );
        let config = report.value;

        assert_eq!(config.theme, ThemeId::TokyoNightStorm);
        assert_eq!(config.appearance, AppearanceMode::System);
        assert_eq!(config.max_depth, 9);
        assert_eq!(std::fs::read(&path).unwrap(), backup);
        assert_eq!(std::fs::read(path.with_extension("bak")).unwrap(), backup);
        assert!(!path.with_extension("tmp").exists());

        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn malformed_live_config_is_restored_from_last_known_good_backup() {
        let directory = test_directory("malformed-live-recovery");
        let path = directory.join("config.toml");
        std::fs::create_dir_all(&directory).unwrap();
        let malformed = b"version = [broken";
        let backup =
            b"version = 1\ntheme = \"rose-pine-moon\"\nappearance = \"dark\"\nmax_depth = 6\n";
        std::fs::write(&path, malformed).unwrap();
        std::fs::write(path.with_extension("bak"), backup).unwrap();

        let config = Config::load_from_path(&path).unwrap();

        assert_eq!(config.theme, ThemeId::RosePineMoon);
        assert_eq!(config.max_depth, 6);
        assert_eq!(std::fs::read(&path).unwrap(), backup);
        assert_eq!(std::fs::read(path.with_extension("bak")).unwrap(), backup);

        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn future_live_config_blocks_recovery_from_older_backup() {
        let directory = test_directory("future-live-authority");
        let path = directory.join("config.toml");
        std::fs::create_dir_all(&directory).unwrap();
        let future = b"version = 99\nmax_depth = 12\n";
        let backup = b"version = 1\nmax_depth = 3\n";
        std::fs::write(&path, future).unwrap();
        std::fs::write(path.with_extension("bak"), backup).unwrap();

        let error = Config::load_from_path(&path).unwrap_err();

        assert!(error.contains("newer than supported version 1"));
        assert_eq!(std::fs::read(&path).unwrap(), future);
        assert_eq!(std::fs::read(path.with_extension("bak")).unwrap(), backup);

        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn missing_live_config_is_restored_from_unique_temporary_file() {
        let directory = test_directory("unique-temp-recovery");
        let path = directory.join("config.toml");
        let temporary = directory.join("config.tmp.4242.1");
        std::fs::create_dir_all(&directory).unwrap();
        let recoverable =
            b"version = 1\ntheme = \"horizon-bold\"\nappearance = \"light\"\nmax_depth = 8\n";
        std::fs::write(&temporary, recoverable).unwrap();

        let report = Config::load_or_create_at_with_diagnostics(&path).unwrap();
        assert_eq!(
            report.events,
            [PersistenceEvent::Recovered {
                store: PersistenceStore::Config,
                source: PersistenceRecoverySource::Temporary,
            }]
        );
        let config = report.value;

        assert_eq!(config.theme, ThemeId::HorizonBold);
        assert_eq!(config.appearance, AppearanceMode::Light);
        assert_eq!(config.max_depth, 8);
        assert_eq!(std::fs::read(&path).unwrap(), recoverable);
        assert!(!temporary.exists());

        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn future_backup_blocks_creation_of_an_older_live_config() {
        let directory = test_directory("future-backup-authority");
        let path = directory.join("config.toml");
        std::fs::create_dir_all(&directory).unwrap();
        let future = b"version = 99\nmax_depth = 12\n";
        std::fs::write(path.with_extension("bak"), future).unwrap();

        let error = Config::load_or_create_at(&path).unwrap_err();

        assert!(error.contains("newer than supported version 1"));
        assert!(!path.exists());
        assert_eq!(std::fs::read(path.with_extension("bak")).unwrap(), future);

        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn write_conflict_is_exposed_as_a_typed_diagnostic() {
        let directory = test_directory("typed-write-conflict");
        let path = directory.join("config.toml");
        std::fs::create_dir_all(&directory).unwrap();
        let lock_file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path.with_extension("lock"))
            .unwrap();
        lock_file.try_lock().unwrap();

        let error = Config::default()
            .save_to_path_with_diagnostics(&path)
            .unwrap_err();

        assert_eq!(
            error.event().cloned(),
            Some(PersistenceEvent::Conflict {
                store: PersistenceStore::Config,
                operation: PersistenceOperation::Write,
            })
        );
        assert!(error.message().contains("another process is writing"));
        assert!(!path.exists());

        drop(lock_file);
        std::fs::remove_dir_all(directory).unwrap();
    }

    fn test_directory(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "devhub-gpui-config-{label}-{}-{unique}",
            std::process::id()
        ))
    }
}
