mod scan;
mod theme;

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

    pub fn sanitize_markdown_images(source: &str) -> String {
        let mut output = String::with_capacity(source.len());
        let mut rest = source;
        while !rest.is_empty() {
            if rest
                .get(..4)
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case("<img"))
            {
                if let Some(end) = rest.find('>') {
                    output.push_str("[image omitted]");
                    rest = &rest[end + 1..];
                    continue;
                }
            }
            if let Some(image) = rest.strip_prefix("![") {
                if let Some(label_end) = image.find(']') {
                    let label = &image[..label_end];
                    let target = &image[label_end + 1..];
                    let consumed = if let Some(target) = target.strip_prefix('(') {
                        target.find(')').map(|end| label_end + end + 5)
                    } else if let Some(target) = target.strip_prefix('[') {
                        target.find(']').map(|end| label_end + end + 5)
                    } else {
                        None
                    };
                    if let Some(consumed) = consumed {
                        output.push_str("[image omitted");
                        if !label.trim().is_empty() {
                            output.push_str(": ");
                            output.push_str(label.trim());
                        }
                        output.push(']');
                        rest = &rest[consumed..];
                        continue;
                    }
                }
            }
            let character = rest.chars().next().unwrap();
            output.push(character);
            rest = &rest[character.len_utf8()..];
        }
        output
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
        fn markdown_images_are_omitted_before_rendering() {
            let source = "![logo](https://example.com/logo.png)\n<IMG src=\"x\">";
            let sanitized = sanitize_markdown_images(source);
            assert_eq!(sanitized, "[image omitted: logo]\n[image omitted]");
            assert!(!sanitized.contains("https://"));
        }
    }
}

pub use scan::{next_selection, previous_selection, ScanModel, ScanState};
pub use text_support::{language_for_path, markdown_fenced_source, sanitize_markdown_images};
pub use theme::{Theme, MONO_FONT, UI_FONT};
