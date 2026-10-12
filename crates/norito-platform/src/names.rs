//! User-entered names, validated identically by every adapter.
use crate::{ErrorKind, PlatformError, Result};

/// A single path component that is valid on every supported OS.
pub fn entry_name(name: &str) -> Result<String> {
    let name = name.trim();
    let stem = name
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    if name.is_empty()
        || name.starts_with('.')
        || name.ends_with('.')
        || name
            .chars()
            .any(|c| c.is_control() || "<>:\"/\\|?*".contains(c))
        || matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && matches!(stem.as_bytes()[3], b'1'..=b'9'))
    {
        return Err(PlatformError::new(
            ErrorKind::Unsupported,
            "有効な名前を入力してください。",
        ));
    }
    Ok(name.into())
}

/// `entry_name`, completed with `.md` unless it already names Markdown.
pub fn markdown_file_name(name: &str) -> Result<String> {
    let name = entry_name(name)?;
    Ok(if is_markdown(&name) {
        name
    } else {
        format!("{name}.md")
    })
}

/// `.md` / `.markdown`, case-insensitive.
pub fn is_markdown(name: &str) -> bool {
    name.rsplit_once('.').is_some_and(|(_, extension)| {
        extension.eq_ignore_ascii_case("md") || extension.eq_ignore_ascii_case("markdown")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn names_are_portable_and_markdown_is_completed() {
        assert_eq!(markdown_file_name(" note ").unwrap(), "note.md");
        assert_eq!(markdown_file_name("a.MARKDOWN").unwrap(), "a.MARKDOWN");
        for name in ["", "../x", "a/b", "NUL", "COM1.txt", "foo."] {
            assert!(entry_name(name).is_err(), "{name}");
        }
        assert!(is_markdown("日本語.MD") && !is_markdown("md") && !is_markdown("a.png"));
    }
}
