use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::Config;

const ACTIVITY_LOG_MAX_BYTES: u64 = 512 * 1024;
const ACTIVITY_LOG_RETAINED_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivityEntry {
    pub ts: u64,
    pub tool: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    pub ok: bool,
    pub duration_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl ActivityEntry {
    pub fn new(tool: &str, project: Option<String>, ok: bool, duration_ms: u64) -> Self {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or_default();
        Self {
            ts,
            tool: tool.to_string(),
            project,
            ok,
            duration_ms,
            detail: None,
        }
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

pub fn activity_log_path() -> Option<PathBuf> {
    Config::config_dir().map(|dir| dir.join("mcp-activity.jsonl"))
}

pub fn append_activity(entry: &ActivityEntry) -> Result<(), String> {
    let path = activity_log_path()
        .ok_or_else(|| "cannot determine the devhub-gpui config directory".to_string())?;
    append_activity_to_path(&path, entry)
}

fn append_activity_to_path(path: &PathBuf, entry: &ActivityEntry) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("creating {}: {error}", parent.display()))?;
    }
    let mut line = serde_json::to_string(entry)
        .map_err(|error| format!("serializing activity entry: {error}"))?;
    line.push('\n');
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| format!("opening {}: {error}", path.display()))?;
    file.write_all(line.as_bytes())
        .map_err(|error| format!("writing {}: {error}", path.display()))?;
    drop(file);

    let len = std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0);
    if len > ACTIVITY_LOG_MAX_BYTES {
        let contents = std::fs::read(path)
            .map_err(|error| format!("reading {} for trimming: {error}", path.display()))?;
        let start = contents.len().saturating_sub(ACTIVITY_LOG_RETAINED_BYTES);
        let aligned = contents[start..]
            .iter()
            .position(|byte| *byte == b'\n')
            .map(|offset| start + offset + 1)
            .unwrap_or(contents.len());
        let retained = &contents[aligned.min(contents.len())..];
        crate::persistence::write_recoverable_checked(path, retained, || Ok(()))
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

pub fn read_recent_activity(limit: usize) -> Vec<ActivityEntry> {
    let Some(path) = activity_log_path() else {
        return Vec::new();
    };
    read_recent_activity_from_path(&path, limit)
}

fn read_recent_activity_from_path(path: &PathBuf, limit: usize) -> Vec<ActivityEntry> {
    let Ok(contents) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let entries = contents
        .lines()
        .filter_map(|line| serde_json::from_str::<ActivityEntry>(line).ok())
        .collect::<Vec<_>>();
    entries
        .into_iter()
        .rev()
        .take(limit)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_directory(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "devhub-gpui-{label}-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        directory
    }

    #[test]
    fn activity_entries_roundtrip_and_read_newest_last() {
        let directory = test_directory("activity-roundtrip");
        let path = directory.join("mcp-activity.jsonl");

        append_activity_to_path(&path, &ActivityEntry::new("list_projects", None, true, 3))
            .unwrap();
        append_activity_to_path(
            &path,
            &ActivityEntry::new("read_file", Some("alpha".to_string()), false, 41)
                .with_detail("no such file"),
        )
        .unwrap();

        let entries = read_recent_activity_from_path(&path, 10);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].tool, "list_projects");
        assert_eq!(entries[1].project.as_deref(), Some("alpha"));
        assert!(!entries[1].ok);
        assert_eq!(entries[1].detail.as_deref(), Some("no such file"));

        let newest_only = read_recent_activity_from_path(&path, 1);
        assert_eq!(newest_only.len(), 1);
        assert_eq!(newest_only[0].tool, "read_file");

        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn missing_log_reads_empty() {
        let directory = test_directory("activity-missing");
        let path = directory.join("mcp-activity.jsonl");
        assert!(read_recent_activity_from_path(&path, 10).is_empty());
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn oversized_log_is_trimmed_to_recent_lines() {
        let directory = test_directory("activity-trim");
        let path = directory.join("mcp-activity.jsonl");
        let detail = "x".repeat(1024);
        let total = (ACTIVITY_LOG_MAX_BYTES / 1024) + 64;
        for index in 0..total {
            append_activity_to_path(
                &path,
                &ActivityEntry::new("read_file", Some(format!("p{index}")), true, 1)
                    .with_detail(&detail),
            )
            .unwrap();
        }

        let len = std::fs::metadata(&path).unwrap().len();
        assert!(len <= ACTIVITY_LOG_MAX_BYTES + 2048);
        let entries = read_recent_activity_from_path(&path, usize::MAX);
        assert!(!entries.is_empty());
        assert_eq!(
            entries.last().unwrap().project.as_deref(),
            Some(format!("p{}", total - 1).as_str())
        );

        std::fs::remove_dir_all(directory).unwrap();
    }
}
