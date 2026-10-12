//! Line-level commands: heading and list prefixes, quotes, task markers.
//! Code blocks are never touched.
use super::Projection;
use crate::syntax::line_at;
use std::ops::Range;

impl Projection {
    /// Give the line at `caret` the block `prefix` (`#`, `##`, `-`), replacing
    /// its current one. The same prefix, or an empty one, removes it.
    pub fn line_prefix(&self, caret: usize, prefix: &str) -> Option<(String, Range<usize>)> {
        let line = line_at(&self.source, caret);
        if self.code_block_within(&line) {
            return None;
        }
        let raw = &self.source[line.clone()];
        let bullet = raw.starts_with("- ") || raw.starts_with("* ") || raw.starts_with("+ ");
        let hashes = raw.bytes().take_while(|b| *b == b'#').count();
        let old = if (1..=6).contains(&hashes) && raw.as_bytes().get(hashes) == Some(&b' ') {
            hashes + 1
        } else if bullet || raw.starts_with("> ") {
            2
        } else {
            0
        };
        let new = if raw[..old].trim() == prefix || prefix.is_empty() {
            String::new()
        } else {
            format!("{prefix} ")
        };
        let mut source = self.source.clone();
        source.replace_range(line.start..line.start + old, &new);
        let caret = line.start + new.len();
        Some((source, caret..caret))
    }
    /// Quote every line touched by `selection`, or unquote them when all are quoted.
    pub fn toggle_quote(&self, selection: Range<usize>) -> Option<(String, Range<usize>)> {
        let lines =
            line_at(&self.source, selection.start).start..line_at(&self.source, selection.end).end;
        if self.code_block_within(&lines) {
            return None;
        }
        let original: Vec<_> = self.source[lines.clone()].split('\n').collect();
        let remove = original.iter().all(|line| line.starts_with('>'));
        let replacement = original
            .iter()
            .map(|line| {
                if remove {
                    line[if line.starts_with("> ") { 2 } else { 1 }..].to_owned()
                } else {
                    format!("> {line}")
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        let mut source = self.source.clone();
        source.replace_range(lines.clone(), &replacement);
        Some((source, lines.start..lines.start + replacement.len()))
    }
    /// Check or uncheck the task marker starting at `at`.
    pub fn toggle_task(&self, at: usize) -> Option<String> {
        let replacement = match self.source.get(at..at + 3)? {
            "[ ]" => "[x]",
            "[x]" | "[X]" => "[ ]",
            _ => return None,
        };
        let mut source = self.source.clone();
        source.replace_range(at..at + 3, replacement);
        Some(source)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prefixes_replace_each_other_and_toggle_off() {
        let p = Projection::new("**text**");
        assert_eq!(p.line_prefix(0, "##").unwrap().0, "## **text**");
        let p = Projection::new("- item");
        assert_eq!(p.line_prefix(3, "#").unwrap().0, "# item");
        let p = Projection::new("# item");
        assert_eq!(p.line_prefix(3, "#").unwrap().0, "item");
        let p = Projection::new("```\ncode\n```\n");
        assert!(p.line_prefix(5, "#").is_none());
    }
    #[test]
    fn quotes_toggle_over_lines_and_tasks_flip() {
        let p = Projection::new("first\nsecond");
        let (quoted, _) = p.toggle_quote(0..12).unwrap();
        assert_eq!(quoted, "> first\n> second");
        let p = Projection::new(&quoted);
        assert_eq!(p.toggle_quote(3..3).unwrap().0, "first\n> second");
        let p = Projection::new("- [ ] todo");
        assert_eq!(p.toggle_task(2).unwrap(), "- [x] todo");
        assert!(p.toggle_task(0).is_none());
    }
}
