mod commands;
mod diagnostics;
mod scan;
mod shell;
mod theme;

pub use commands::{filtered_commands, filtered_themes, CommandId, CommandSpec, ThemeSelection};
pub use diagnostics::{persistence_status_text, PersistenceHistory};
pub use shell::{visible_project_row, Activity};

use std::path::PathBuf;

use devhub_core::{Config, Project, RemoteHostConfig};

pub fn should_show_ftue(config_exists: bool, cache_exists: bool) -> bool {
    !config_exists && !cache_exists
}

pub fn has_scan_sources(local_roots: usize, remote_hosts: usize) -> bool {
    local_roots > 0 || remote_hosts > 0
}

pub fn scan_sources_changed(
    current: &Config,
    scan_dirs: &[PathBuf],
    remote_hosts: &[RemoteHostConfig],
    max_depth: usize,
) -> bool {
    current.scan_dirs != scan_dirs
        || current.remote_hosts != remote_hosts
        || current.max_depth != max_depth
}

pub fn filtered_project_indices(projects: &[Project], query: &str) -> Vec<usize> {
    if query.is_empty() {
        return (0..projects.len()).collect();
    }

    projects
        .iter()
        .enumerate()
        .filter_map(|(index, project)| project.search_key.contains(query).then_some(index))
        .collect()
}

pub fn partition_local_scan_roots(roots: Vec<PathBuf>) -> (Vec<PathBuf>, Vec<String>) {
    let mut available = Vec::with_capacity(roots.len());
    let mut errors = Vec::new();

    for root in roots {
        match std::fs::metadata(&root) {
            Ok(metadata) if metadata.is_dir() => available.push(root),
            Ok(_) => errors.push(format!("Scan root is not a directory: {}", root.display())),
            Err(error) => errors.push(format!("Cannot scan {}: {error}", root.display())),
        }
    }

    (available, errors)
}

mod text_support {
    use std::path::Path;

    pub fn language_for_path(path: &Path) -> &'static str {
        match path
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str()
        {
            "rs" => "Rust",
            "js" | "jsx" => "JavaScript",
            "ts" | "tsx" => "TypeScript",
            "py" => "Python",
            "go" => "Go",
            "json" => "JSON",
            "toml" => "TOML",
            "yaml" | "yml" => "YAML",
            "md" | "markdown" => "Markdown",
            "html" | "htm" => "HTML",
            "css" | "scss" => "CSS",
            _ => "Text",
        }
    }

    pub fn markdown_fenced_source(language: &str, source: &str) -> String {
        let mut longest = 0;
        let mut current = 0;
        for character in source.chars() {
            if character == '`' {
                current += 1;
                longest = longest.max(current);
            } else {
                current = 0;
            }
        }
        let fence = "`".repeat((longest + 1).max(3));
        format!("{fence}{language}\n{source}\n{fence}")
    }

    pub fn omit_markdown_images(source: &str) -> String {
        let mut output = String::with_capacity(source.len());
        let mut fence: Option<(char, usize)> = None;

        for line in source.split_inclusive('\n') {
            let content = line.strip_suffix('\n').unwrap_or(line);
            let marker = fence_marker(content);
            if let Some((fence_char, fence_len)) = fence {
                output.push_str(line);
                if marker.is_some_and(|(character, length)| {
                    character == fence_char && length >= fence_len
                }) {
                    fence = None;
                }
            } else if let Some(marker) = marker {
                fence = Some(marker);
                output.push_str(line);
            } else {
                output.push_str(&omit_images_in_fragment(line));
            }
        }

        output
    }

    fn omit_images_in_fragment(source: &str) -> String {
        let mut output = String::with_capacity(source.len());
        let mut rest = source;
        let mut inline_code_ticks = 0;

        while !rest.is_empty() {
            if rest.starts_with('`') {
                let ticks = rest
                    .chars()
                    .take_while(|character| *character == '`')
                    .count();
                output.push_str(&rest[..ticks]);
                inline_code_ticks = if inline_code_ticks == ticks { 0 } else { ticks };
                rest = &rest[ticks..];
                continue;
            }
            if inline_code_ticks == 0
                && rest
                    .get(..4)
                    .is_some_and(|prefix| prefix.eq_ignore_ascii_case("<img"))
            {
                if let Some(end) = rest.find('>') {
                    let tag = &rest[..=end];
                    output.push_str(&image_placeholder(html_attribute(tag, "alt")));
                    rest = &rest[end + 1..];
                    continue;
                }
            }
            if inline_code_ticks == 0 {
                if let Some(image) = rest.strip_prefix("![") {
                    if let Some(label_end) = image.find(']') {
                        let target = &image[label_end + 1..];
                        if let Some(target) = target.strip_prefix('(') {
                            if let Some(end) = target.find(')') {
                                output.push_str(&image_placeholder(Some(&image[..label_end])));
                                rest = &target[end + 1..];
                                continue;
                            }
                        }
                    }
                }
            }

            let character = rest.chars().next().unwrap();
            output.push(character);
            rest = &rest[character.len_utf8()..];
        }
        output
    }

    fn fence_marker(line: &str) -> Option<(char, usize)> {
        let line = line.trim_start();
        let character = line.chars().next()?;
        if !matches!(character, '`' | '~') {
            return None;
        }
        let length = line
            .chars()
            .take_while(|candidate| *candidate == character)
            .count();
        (length >= 3).then_some((character, length))
    }

    fn image_placeholder(alt: Option<&str>) -> String {
        let alt = alt
            .unwrap_or_default()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .replace(['[', ']'], "");
        if alt.is_empty() {
            "[image not loaded]".to_string()
        } else {
            format!("[image not loaded: {alt}]")
        }
    }

    fn html_attribute<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
        let lower = tag.to_ascii_lowercase();
        let name_start = lower.find(name)?;
        let after_name = &tag[name_start + name.len()..];
        let equals = after_name.find('=')?;
        if !after_name[..equals].trim().is_empty() {
            return None;
        }
        let value = after_name[equals + 1..].trim_start();
        let quote = value.chars().next()?;
        if matches!(quote, '\'' | '"') {
            let body = &value[quote.len_utf8()..];
            return body.find(quote).map(|end| &body[..end]);
        }
        let end = value
            .find(|character: char| character.is_whitespace() || character == '>')
            .unwrap_or(value.len());
        Some(&value[..end])
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn fenced_source_outgrows_backticks_in_content() {
            let fenced = markdown_fenced_source("rust", "```nested```");
            assert!(fenced.starts_with("````rust\n"));
            assert!(fenced.ends_with("\n````"));
        }

        #[test]
        fn markdown_and_html_images_become_offline_placeholders() {
            let source = "![badge](https://example.com/badge.svg)\n<IMG src=\"demo.gif\" alt=\"Demo animation\">\n![logo](assets/logo.png)";
            let omitted = omit_markdown_images(source);

            assert_eq!(
                omitted,
                "[image not loaded: badge]\n[image not loaded: Demo animation]\n[image not loaded: logo]"
            );
        }

        #[test]
        fn linked_image_keeps_its_destination_link() {
            let source = "[![Rust](badge.svg)](https://rust-lang.org)";
            assert_eq!(
                omit_markdown_images(source),
                "[[image not loaded: Rust]](https://rust-lang.org)"
            );
        }

        #[test]
        fn image_syntax_in_code_is_not_rewritten() {
            let source = "`![inline](image.png)`\n```md\n![fenced](image.png)\n```\n";
            assert_eq!(omit_markdown_images(source), source);
        }
    }
}

pub use scan::{next_selection, previous_selection, ScanModel, ScanState};
pub use text_support::{language_for_path, markdown_fenced_source, omit_markdown_images};
pub use theme::{Theme, MONO_FONT, UI_FONT};

#[cfg(test)]
mod startup_tests {
    use super::{
        filtered_project_indices, has_scan_sources, partition_local_scan_roots,
        scan_sources_changed, should_show_ftue,
    };
    use devhub_core::{Project, ProjectSource, ProjectType};
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn ftue_only_opens_when_config_and_cache_are_both_absent() {
        assert!(should_show_ftue(false, false));
        assert!(!should_show_ftue(true, false));
        assert!(!should_show_ftue(false, true));
        assert!(!should_show_ftue(true, true));
    }

    #[test]
    fn scan_requires_a_configured_local_or_remote_source() {
        assert!(!has_scan_sources(0, 0));
        assert!(has_scan_sources(1, 0));
        assert!(has_scan_sources(0, 1));
    }

    #[test]
    fn appearance_only_settings_do_not_require_a_catalog_scan() {
        let config = devhub_core::Config::default();
        assert!(!scan_sources_changed(
            &config,
            &config.scan_dirs,
            &config.remote_hosts,
            config.max_depth,
        ));

        let mut changed_roots = config.scan_dirs.clone();
        changed_roots.push(PathBuf::from(r"F:\projects"));
        assert!(scan_sources_changed(
            &config,
            &changed_roots,
            &config.remote_hosts,
            config.max_depth,
        ));
        assert!(scan_sources_changed(
            &config,
            &config.scan_dirs,
            &config.remote_hosts,
            config.max_depth + 1,
        ));
    }

    #[test]
    fn unavailable_local_roots_do_not_block_available_roots() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let available = std::env::temp_dir().join(format!(
            "devhub-gpui-root-partition-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir_all(&available).unwrap();
        let missing = available.join("missing");

        let (roots, errors) = partition_local_scan_roots(vec![missing.clone(), available.clone()]);

        assert_eq!(roots.as_slice(), std::slice::from_ref(&available));
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains(&missing.display().to_string()));

        std::fs::remove_dir_all(available).unwrap();
    }

    #[test]
    fn filtering_remains_correct_for_large_project_collections() {
        let projects = (0..50_000)
            .map(|index| Project {
                name: format!("project-{index:05}"),
                path: PathBuf::from(format!(r"F:\projects\project-{index:05}")),
                source: ProjectSource::Local,
                project_type: ProjectType::Rust,
                has_git: true,
                git_remote: None,
                markers_found: vec!["Cargo.toml".into()],
                last_modified: None,
                search_key: format!("project-{index:05} rust"),
            })
            .collect::<Vec<_>>();

        assert_eq!(
            filtered_project_indices(&projects, "project-49999"),
            [49_999]
        );
        assert!(filtered_project_indices(&projects, "no-match").is_empty());
        assert_eq!(filtered_project_indices(&projects, "").len(), 50_000);
    }

    #[test]
    fn path_only_preferences_collide_across_remote_hosts() {
        let path = PathBuf::from("/srv/project");
        let projects = ["build-a", "build-b"].map(|host| Project {
            name: "project".into(),
            path: path.clone(),
            source: ProjectSource::Remote {
                name: String::new(),
                host: host.into(),
            },
            project_type: ProjectType::Rust,
            has_git: true,
            git_remote: None,
            markers_found: vec!["Cargo.toml".into()],
            last_modified: None,
            search_key: String::new(),
        });
        let pinned_paths = [path];

        assert!(projects
            .iter()
            .all(|project| pinned_paths.contains(&project.path)));
        assert_ne!(projects[0].source.host(), projects[1].source.host());
    }

    #[test]
    #[ignore = "manual release-mode Milestone 0 measurement"]
    fn measure_large_catalog_filter_baseline() {
        let projects = (0..50_000)
            .map(|index| Project {
                name: format!("project-{index:05}"),
                path: PathBuf::from(format!(r"F:\projects\project-{index:05}")),
                source: ProjectSource::Local,
                project_type: ProjectType::Rust,
                has_git: true,
                git_remote: None,
                markers_found: vec!["Cargo.toml".into()],
                last_modified: None,
                search_key: format!("project-{index:05} rust"),
            })
            .collect::<Vec<_>>();

        let empty_started = std::time::Instant::now();
        let all = filtered_project_indices(&projects, "");
        let empty_elapsed = empty_started.elapsed();
        let selective_started = std::time::Instant::now();
        let selective = filtered_project_indices(&projects, "project-49999");
        let selective_elapsed = selective_started.elapsed();

        assert_eq!(all.len(), 50_000);
        assert_eq!(selective, [49_999]);
        println!(
            "M0_BASELINE filter_projects={} filter_empty_ms={:.3} filter_selective_ms={:.3}",
            projects.len(),
            empty_elapsed.as_secs_f64() * 1_000.0,
            selective_elapsed.as_secs_f64() * 1_000.0,
        );
    }
}
