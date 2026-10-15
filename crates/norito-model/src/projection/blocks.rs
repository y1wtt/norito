//! Line-level commands: heading and list prefixes, quotes, task markers.
//! Code blocks are never touched.
use super::Projection;
use crate::syntax::{indentation, line_at, list_prefix};
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
    /// Nest the list item at `selection`, with the touched lines and its
    /// sub-items, under the previous item; or, when `outdent`, beside its
    /// parent. Returns the new source and the shifted selection.
    pub fn indent_list(
        &self,
        selection: Range<usize>,
        outdent: bool,
    ) -> Option<(String, Range<usize>)> {
        let first = line_at(&self.source, selection.start);
        let last = line_at(&self.source, selection.end);
        if self.code_block_within(&(first.start..last.end)) {
            return None;
        }
        let line = &self.source[first.clone()];
        list_prefix(line)?;
        let depth = indentation(line);
        let mut above = self.source[..first.start]
            .lines()
            .rev()
            .filter(|l| !l.trim().is_empty());
        let target = if outdent {
            if depth == 0 {
                return None;
            }
            above
                .find(|l| indentation(l) < depth && list_prefix(l).is_some())
                .map_or(0, indentation)
        } else {
            // Only an item with a previous sibling can be nested.
            let sibling = above.find(|l| {
                indentation(l) < depth || (indentation(l) == depth && list_prefix(l).is_some())
            })?;
            if indentation(sibling) < depth {
                return None;
            }
            list_prefix(sibling)?.0
        };
        let mut end = last.end;
        let mut at = last.end;
        while at < self.source.len() {
            let next = line_at(&self.source, at + 1);
            at = next.end;
            let text = &self.source[next];
            if text.trim().is_empty() {
                continue;
            }
            if indentation(text) <= depth {
                break;
            }
            end = at;
        }
        let delta = target as isize - depth as isize;
        let mut source = self.source[..first.start].to_owned();
        // Line start, old and new indentation of every shifted line.
        let mut shifted = vec![];
        let mut start = first.start;
        for l in self.source[first.start..end].split_inclusive('\n') {
            let indent = indentation(l);
            if l.trim().is_empty() {
                source.push_str(l);
            } else {
                let width = (indent as isize + delta).max(0) as usize;
                shifted.push((start, indent, width));
                source.push_str(&" ".repeat(width));
                source.push_str(&l[indent..]);
            }
            start += l.len();
        }
        source.push_str(&self.source[end..]);
        let map = |at: usize| {
            let mut moved = at as isize;
            for &(start, indent, width) in shifted.iter().filter(|s| s.0 <= at) {
                let column = at - start;
                moved += if column >= indent {
                    width as isize - indent as isize
                } else {
                    column.min(width) as isize - column as isize
                };
            }
            moved as usize
        };
        Some((source, map(selection.start)..map(selection.end)))
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
    #[test]
    fn list_items_nest_under_siblings_with_their_children() {
        let p = Projection::new("- a\n- b\n  - c\n- d");
        let (nested, caret) = p.indent_list(6..6, false).unwrap();
        assert_eq!(nested, "- a\n  - b\n    - c\n- d");
        assert_eq!(caret, 8..8);
        let p = Projection::new(&nested);
        assert_eq!(p.indent_list(8..8, true).unwrap().0, "- a\n- b\n  - c\n- d");
        // The first item has nothing to nest under; top-level items cannot outdent.
        assert!(p.indent_list(2..2, false).is_none());
        assert!(p.indent_list(2..2, true).is_none());
        let p = Projection::new("1. a\n2. ");
        assert_eq!(p.indent_list(8..8, false).unwrap().0, "1. a\n   2. ");
        assert!(Projection::new("text").indent_list(0..0, false).is_none());
    }
}
