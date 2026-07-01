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
