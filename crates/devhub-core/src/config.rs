use std::path::PathBuf;

use serde::{Deserialize, Serialize};

const APP_QUALIFIER: &str = "";
const APP_ORGANIZATION: &str = "";
const APP_IDENTITY: &str = "devhub-gpui";

fn default_max_depth() -> usize {
    3
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
    pub theme: ThemeId,

    #[serde(default)]
    pub appearance: AppearanceMode,

    #[serde(default)]
    pub scan_dirs: Vec<PathBuf>,

    #[serde(default = "default_max_depth")]
    pub max_depth: usize,

    #[serde(default)]
    pub remote_hosts: Vec<RemoteHostConfig>,
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
            theme: ThemeId::default(),
            appearance: AppearanceMode::default(),
            scan_dirs: Vec::new(),
            max_depth: default_max_depth(),
            remote_hosts: Vec::new(),
        }
    }
}

impl Config {
    pub fn config_dir() -> Option<PathBuf> {
        directories::ProjectDirs::from(APP_QUALIFIER, APP_ORGANIZATION, APP_IDENTITY)
            .map(|dirs| dirs.config_dir().to_path_buf())
    }

    pub fn cache_dir() -> Option<PathBuf> {
        directories::ProjectDirs::from(APP_QUALIFIER, APP_ORGANIZATION, APP_IDENTITY)
            .map(|dirs| dirs.cache_dir().to_path_buf())
    }

    pub fn config_path() -> Option<PathBuf> {
        Self::config_dir().map(|dir| dir.join("config.toml"))
    }

    pub fn load_or_create() -> Result<Self, String> {
        let path = Self::config_path()
            .ok_or_else(|| "cannot determine the devhub-gpui config directory".to_string())?;

        if path.exists() {
            let raw = std::fs::read_to_string(&path)
                .map_err(|error| format!("reading {}: {error}", path.display()))?;
            let mut config: Self = toml::from_str(&raw)
                .map_err(|error| format!("parsing {}: {error}", path.display()))?;
            config.normalize();
            Ok(config)
        } else {
            let config = Self::default();
            config.save()?;
            Ok(config)
        }
    }

    pub fn save(&self) -> Result<(), String> {
        let path = Self::config_path()
            .ok_or_else(|| "cannot determine the devhub-gpui config directory".to_string())?;
        let serialized =
            toml::to_string_pretty(self).map_err(|error| format!("serializing config: {error}"))?;
        write_crash_safe(&path, serialized.as_bytes())
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

pub fn normalize_ssh_host(raw: &str) -> String {
    raw.trim()
        .strip_prefix("ssh ")
        .map(str::trim)
        .unwrap_or_else(|| raw.trim())
        .to_string()
}

pub(crate) fn write_crash_safe(path: &std::path::Path, contents: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("{} has no parent directory", path.display()))?;
    std::fs::create_dir_all(parent)
        .map_err(|error| format!("creating {}: {error}", parent.display()))?;

    let temporary = path.with_extension("tmp");
    let backup = path.with_extension("bak");
    std::fs::write(&temporary, contents)
        .map_err(|error| format!("writing {}: {error}", temporary.display()))?;

    if backup.exists() {
        std::fs::remove_file(&backup)
            .map_err(|error| format!("removing {}: {error}", backup.display()))?;
    }
    if path.exists() {
        std::fs::rename(path, &backup)
            .map_err(|error| format!("backing up {}: {error}", path.display()))?;
    }

    if let Err(error) = std::fs::rename(&temporary, path) {
        if backup.exists() {
            let _ = std::fs::rename(&backup, path);
        }
        return Err(format!("replacing {}: {error}", path.display()));
    }

    if backup.exists() {
        let _ = std::fs::remove_file(backup);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn default_config_has_sensible_scan_defaults() {
        let config = Config::default();
        assert!(config.scan_dirs.is_empty());
        assert_eq!(config.max_depth, 3);
    }

    #[test]
    fn config_roundtrips_through_toml() {
        let config = Config {
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
        };

        let serialized = toml::to_string_pretty(&config).unwrap();
        let deserialized: Config = toml::from_str(&serialized).unwrap();

        assert_eq!(deserialized.scan_dirs, config.scan_dirs);
        assert_eq!(deserialized.max_depth, 5);
        assert_eq!(deserialized.remote_hosts, config.remote_hosts);
        assert_eq!(deserialized.theme, ThemeId::TokyoNightStorm);
        assert_eq!(deserialized.appearance, AppearanceMode::System);
    }

    #[test]
    fn missing_fields_fall_back_to_defaults() {
        let raw = "";
        let config: Config = toml::from_str(raw).unwrap();
        assert!(config.scan_dirs.is_empty());
        assert_eq!(config.max_depth, 3);
        assert!(config.remote_hosts.is_empty());
        assert_eq!(config.theme, ThemeId::MonochromeZero);
        assert_eq!(config.appearance, AppearanceMode::Dark);
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
    fn crash_safe_write_replaces_existing_content_and_cleans_auxiliary_files() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "devhub-gpui-config-test-{}-{unique}",
            std::process::id()
        ));
        let path = directory.join("config.toml");

        write_crash_safe(&path, b"first").unwrap();
        write_crash_safe(&path, b"second").unwrap();

        assert_eq!(std::fs::read(&path).unwrap(), b"second");
        assert!(!path.with_extension("tmp").exists());
        assert!(!path.with_extension("bak").exists());

        std::fs::remove_dir_all(directory).unwrap();
    }
}
