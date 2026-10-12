//! Markdown recognition shared by structure, projection and editing rules.

/// Shared parser extensions keep navigation and editable projection consistent.
pub fn options() -> pulldown_cmark::Options {
    use pulldown_cmark::Options as O;
    O::ENABLE_TABLES
        | O::ENABLE_MATH
        | O::ENABLE_FOOTNOTES
        | O::ENABLE_TASKLISTS
        | O::ENABLE_STRIKETHROUGH
        | O::ENABLE_YAML_STYLE_METADATA_BLOCKS
        | O::ENABLE_GFM
}

/// Fenced code with this info string renders as a diagram.
pub fn is_mermaid(info: &str) -> bool {
    info.split_whitespace().next() == Some("mermaid")
}

pub fn is_fence(line: &str) -> bool {
    let line = line.trim_start();
    line.starts_with("```") || line.starts_with("~~~")
}

/// Length of consecutive quote markers, including their optional spaces.
pub fn quote_prefix(line: &str) -> usize {
    let mut at = 0;
    while line.as_bytes().get(at) == Some(&b'>') {
        at += 1;
        if line.as_bytes().get(at) == Some(&b' ') {
            at += 1;
        }
    }
    at
}

/// Length of a list marker and the marker that continues the list on the next line.
pub fn list_prefix(line: &str) -> Option<(usize, String)> {
    let end = line.find('\n').unwrap_or(line.len());
    let line = &line[..end];
    if line.starts_with(['-', '*', '+']) && line.as_bytes().get(1) == Some(&b' ') {
        return Some((2, line[..2].into()));
    }
    let digits = line.bytes().take_while(u8::is_ascii_digit).count();
    if digits > 0
        && matches!(line.as_bytes().get(digits), Some(b'.' | b')'))
        && line.as_bytes().get(digits + 1) == Some(&b' ')
    {
        let n: u64 = line[..digits].parse().ok()?;
        return Some((
            digits + 2,
            format!("{}{} ", n.saturating_add(1), &line[digits..digits + 1]),
        ));
    }
    None
}

/// Byte range of the source line containing `at`, without its newline.
pub fn line_at(source: &str, at: usize) -> std::ops::Range<usize> {
    let start = source[..at].rfind('\n').map_or(0, |i| i + 1);
    let end = source[at..].find('\n').map_or(source.len(), |i| at + i);
    start..end
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prefixes_continue_lists_and_nested_quotes() {
        assert_eq!(quote_prefix("> > text"), 4);
        assert_eq!(list_prefix("- item"), Some((2, "- ".into())));
        assert_eq!(list_prefix("9) item"), Some((3, "10) ".into())));
        assert_eq!(list_prefix("-item"), None);
        assert_eq!(line_at("a\nbc\nd", 3), 2..4);
        assert!(is_mermaid("mermaid theme") && !is_mermaid("rust"));
    }
}
