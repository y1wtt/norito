//! Markdown writers for generated blocks, and their placement in a source.
use std::ops::Range;

/// A GFM table. Missing delimiters default to `---` (unaligned).
pub fn table(cells: &[Vec<String>], delimiters: &[String]) -> String {
    let columns = cells.first().map_or(0, Vec::len);
    if columns == 0 {
        return String::new();
    }
    let mut out = String::new();
    for (index, row) in cells.iter().enumerate() {
        out.push('|');
        for column in 0..columns {
            out.push(' ');
            out.push_str(
                &row.get(column)
                    .map_or("", String::as_str)
                    .replace('|', "\\|")
                    .replace(['\r', '\n'], " "),
            );
            out.push_str(" |");
        }
        out.push('\n');
        if index == 0 {
            out.push('|');
            for column in 0..columns {
                out.push(' ');
                out.push_str(delimiters.get(column).map_or("---", String::as_str));
                out.push_str(" |");
            }
            out.push('\n');
        }
    }
    out
}

/// A fence long enough to contain any backtick run in `content`.
pub fn fenced_code(language: &str, content: &str) -> String {
    let longest = content.split(|c| c != '`').map(str::len).max().unwrap_or(0);
    let fence = "`".repeat(3.max(longest + 1));
    let language = language.replace(['`', '\r', '\n'], "");
    format!(
        "{fence}{language}\n{}{fence}\n",
        if content.ends_with('\n') {
            content.to_owned()
        } else {
            format!("{content}\n")
        }
    )
}

/// Insert `markdown` over `range`. A block replaces nothing when `range` is
/// empty: it goes after the current line, separated by a blank line.
/// Returns the new source and the offset just after the inserted Markdown.
pub fn insert(
    source: &str,
    mut range: Range<usize>,
    markdown: &str,
    block: bool,
) -> (String, usize) {
    let mut source = source.to_owned();
    if block && range.is_empty() {
        let line_end = source[range.start..]
            .find('\n')
            .map_or(source.len(), |offset| range.start + offset);
        range = line_end..line_end;
    }
    let mut insertion = String::new();
    if block && range.start > 0 && !source[..range.start].ends_with("\n\n") {
        insertion.push_str(if source[..range.start].ends_with('\n') {
            "\n"
        } else {
            "\n\n"
        });
    }
    insertion.push_str(markdown);
    let end = range.start + insertion.len();
    if block && range.end < source.len() && !source[range.end..].starts_with('\n') {
        insertion.push('\n');
    }
    source.replace_range(range, &insertion);
    (source, end)
}

/// `[label](<destination>)`, or an image when `image` is set. The label is escaped.
pub fn link(label: &str, destination: &str, image: bool) -> String {
    let label = label
        .replace('\\', "\\\\")
        .replace('[', "\\[")
        .replace(']', "\\]");
    format!("{}[{label}](<{destination}>)", if image { "!" } else { "" })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::structure::Structure;
    #[test]
    fn helpers_round_trip_pipes_and_nested_fences() {
        let cells = vec![
            vec!["名前".into(), "値".into()],
            vec!["a | b".into(), "x".into()],
        ];
        assert_eq!(Structure::parse(&table(&cells, &[])).tables[0].cells, cells);
        assert!(table(&cells, &[":---".into(), "---:".into()]).contains("| :--- | ---: |"));
        let text = "```rust\nfn main() {}\n```";
        assert_eq!(
            Structure::parse(&fenced_code("markdown", text)).code[0].content,
            format!("{text}\n")
        );
    }
    #[test]
    fn blocks_are_separated_and_links_escaped() {
        assert_eq!(insert("a\nb", 0..0, "X", true), ("a\n\nX\nb".into(), 4));
        assert_eq!(insert("ab", 1..1, "X", false), ("aXb".into(), 2));
        assert_eq!(link("画像[1]", "a.png", true), "![画像\\[1\\]](<a.png>)");
    }
}
