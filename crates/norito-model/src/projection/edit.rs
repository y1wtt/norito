//! Display-space typing, deletion and line breaks, applied to the source.
use super::Projection;
use crate::syntax::{list_prefix, quote_prefix};
use std::{collections::BTreeMap, ops::Range};

impl Projection {
    /// Replace the displayed `display` range by `inserted`. Returns the new
    /// source and the caret position in it.
    pub fn replace(&self, display: Range<usize>, inserted: &str) -> (String, usize) {
        let source_range = self.source_range(display.clone());
        if !inserted.contains('\n')
            && self
                .revealed
                .iter()
                .any(|r| r.start <= source_range.start && source_range.end <= r.end)
        {
            let mut source = self.source.clone();
            let caret = source_range.start + inserted.len();
            source.replace_range(source_range, inserted);
            return (source, caret);
        }
        if inserted.contains('\n') {
            return self.replace_lines(display, inserted);
        }
        if !display.is_empty() && display.start == 0 && display.end == self.text.len() {
            return (inserted.into(), inserted.len());
        }
        let selected: Vec<_> = self
            .cells
            .iter()
            .filter(|c| c.display.end > display.start && c.display.start < display.end)
            .collect();
        let anchor = selected
            .first()
            .map_or_else(|| self.source_position(display.start), |c| c.source.start);
        let mut deletions: Vec<_> = selected.iter().map(|c| c.source.clone()).collect();
        if inserted.is_empty() {
            // A construct whose whole content is deleted loses its delimiters too.
            for wrapper in &self.wrappers {
                let inside: Vec<_> = self
                    .cells
                    .iter()
                    .filter(|c| {
                        c.source.start >= wrapper.inner.start && c.source.end <= wrapper.inner.end
                    })
                    .collect();
                if !inside.is_empty()
                    && inside
                        .iter()
                        .all(|c| c.display.start >= display.start && c.display.end <= display.end)
                {
                    deletions.push(wrapper.outer.clone());
                }
            }
        }
        // Joining lines removes the hidden block prefix of the second line.
        for prefix in &self.prefixes {
            if prefix.start > 0
                && selected.iter().any(|c| {
                    c.source.end == prefix.start && self.text[c.display.clone()].ends_with('\n')
                })
            {
                deletions.push(prefix.clone());
            }
        }
        // Joining two identically formatted runs must not create adjacent
        // closing/opening delimiters (e.g. **a****b**).
        if inserted.is_empty() && self.text[display.clone()].contains('\n') {
            for left in &self.wrappers {
                for right in &self.wrappers {
                    if left.outer.end <= right.outer.start
                        && left.marker == right.marker
                        && self.source[left.inner.end..left.outer.end]
                            == self.source[right.inner.end..right.outer.end]
                        && (left.outer.end..right.outer.start)
                            .all(|at| deletions.iter().any(|r| r.contains(&at)))
                    {
                        deletions.push(left.inner.end..left.outer.end);
                        deletions.push(right.outer.start..right.inner.start);
                    }
                }
            }
        }
        let merged = merge(deletions);
        let removed_before: usize = merged
            .iter()
            .map(|r| anchor.saturating_sub(r.start).min(r.len()))
            .sum();
        let insertion = anchor - removed_before;
        let mut source = self.source.clone();
        for r in merged.into_iter().rev() {
            source.replace_range(r, "");
        }
        source.insert_str(insertion, inserted);
        (source, insertion + inserted.len())
    }

    /// Enter and multiline paste share a structural edit path. A newline
    /// must not leave whitespace inside the end of an inline delimiter.
    fn replace_lines(&self, display: Range<usize>, inserted: &str) -> (String, usize) {
        let (mut source, mut caret) = if display.is_empty() {
            (self.source.clone(), self.source_position(display.start))
        } else {
            self.replace(display, "")
        };
        let enter = inserted == "\n" || inserted == "\r\n";
        for part in inserted.split_inclusive('\n') {
            let projection = Self::new(&source);
            let at = projection.display_position(caret);
            let text = part.strip_suffix('\n').unwrap_or(part);
            let text = if part.ends_with('\n') {
                text.strip_suffix('\r').unwrap_or(text)
            } else {
                text
            };
            if !text.is_empty() {
                (source, caret) = projection.replace(at..at, text);
            }
            if part.ends_with('\n') {
                let projection = Self::new(&source);
                (source, caret) = projection.break_line(
                    projection.display_position(caret),
                    if part.ends_with("\r\n") { "\r\n" } else { "\n" },
                    enter,
                );
            }
        }
        (source, caret)
    }

    /// Split the line at `display`. Inline constructs are closed before the
    /// break and reopened after it; quotes and lists continue when asked.
    fn break_line(&self, display: usize, newline: &str, continue_block: bool) -> (String, usize) {
        let mut anchor = self.source_position(display);
        let start = self.source[..anchor].rfind('\n').map_or(0, |at| at + 1);
        let line = self.source[start..].split('\n').next().unwrap_or("");
        let in_code = self
            .cells
            .iter()
            .any(|c| c.source.start <= anchor && c.source.end >= anchor && c.style.block_code);
        if !in_code && continue_block {
            // A break before a visible list marker splits after it, not through it.
            let marker = match quote_prefix(line) {
                0 => list_prefix(line).map_or(0, |(len, _)| len),
                len => len,
            };
            anchor = anchor.max(start + marker);
        }
        let mut hidden = vec![false; self.source.len()];
        let mut before = BTreeMap::<usize, String>::new();
        let mut after = BTreeMap::<usize, String>::new();
        let mut active: Vec<_> = self
            .wrappers
            .iter()
            .filter(|w| w.inner.start <= anchor && anchor <= w.inner.end)
            .collect();
        active.sort_by_key(|w| w.outer.start);
        let visible = |range: Range<usize>| {
            self.cells.iter().any(|c| {
                c.source.start >= range.start
                    && c.source.end <= range.end
                    && !self.text[c.display.clone()].trim().is_empty()
            })
        };
        for w in active {
            let opening = &self.source[w.outer.start..w.inner.start];
            let closing = &self.source[w.inner.end..w.outer.end];
            if visible(w.inner.start..anchor) {
                let trimmed = self.source[w.inner.start..anchor].trim_end().len();
                before
                    .entry(w.inner.start + trimmed)
                    .or_default()
                    .insert_str(0, closing);
            } else {
                hidden[w.outer.start..w.inner.start].fill(true);
            }
            if visible(anchor..w.inner.end) {
                let tail = &self.source[anchor..w.inner.end];
                after
                    .entry(anchor + tail.len() - tail.trim_start().len())
                    .or_default()
                    .push_str(opening);
            } else {
                hidden[w.inner.end..w.outer.end].fill(true);
            }
        }
        let mut insertion = newline.to_owned();
        if !in_code && continue_block {
            let quote_len = quote_prefix(line);
            let continuation = if quote_len > 0 {
                Some((quote_len, line[..quote_len].to_owned()))
            } else {
                list_prefix(line)
            };
            if let Some((len, next)) = continuation {
                if line[len..].trim().is_empty()
                    && quote_len == 0
                    && let Some((source, caret)) = self.indent_list(anchor..anchor, true)
                {
                    // An empty nested item moves up a level instead.
                    return (source, caret.end);
                }
                if line[len..].trim().is_empty() {
                    // An empty item ends the block. Keep an actual paragraph
                    // boundary; otherwise later typing is a lazy continuation.
                    hidden[start..start + len].fill(true);
                } else {
                    insertion.push_str(&next);
                }
            }
        }
        let mut result = String::new();
        let mut caret = 0;
        for at in self
            .source
            .char_indices()
            .map(|(i, _)| i)
            .chain(std::iter::once(self.source.len()))
        {
            if let Some(value) = before.get(&at) {
                result.push_str(value);
            }
            if at == anchor {
                result.push_str(&insertion);
            }
            if let Some(value) = after.get(&at) {
                result.push_str(value);
            }
            if at == anchor {
                caret = result.len();
            }
            if at < self.source.len() && !hidden[at] {
                result.push(self.source[at..].chars().next().unwrap());
            }
        }
        (result, caret)
    }
}

/// Sort and coalesce overlapping or touching ranges.
fn merge(mut ranges: Vec<Range<usize>>) -> Vec<Range<usize>> {
    ranges.sort_by_key(|r| r.start);
    let mut merged: Vec<Range<usize>> = vec![];
    for r in ranges {
        if let Some(last) = merged.last_mut()
            && r.start <= last.end
        {
            last.end = last.end.max(r.end);
        } else {
            merged.push(r);
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn deleting_entire_formatted_text_cleans_delimiters() {
        let p = Projection::new("before **bold** after");
        let (source, _) = p.replace(7..11, "");
        assert_eq!(source, "before  after");
    }
    #[test]
    fn partial_cross_style_deletion_keeps_remaining_format() {
        let p = Projection::new("abc **def** ghi");
        let (source, _) = p.replace(2..5, "X");
        assert_eq!(source, "abX**ef** ghi");
    }
    #[test]
    fn joining_heading_removes_its_prefix() {
        let p = Projection::new("text\n\n## Heading\n");
        let (source, _) = p.replace(4..6, "");
        assert_eq!(source, "textHeading\n");
    }
    #[test]
    fn list_enter_continues_and_empty_item_exits() {
        let p = Projection::new("- item");
        let (source, _) = p.replace(p.text.len()..p.text.len(), "\n");
        assert_eq!(source, "- item\n- ");
        let p = Projection::new(&source);
        let (source, _) = p.replace(p.text.len()..p.text.len(), "\n");
        assert_eq!(source, "- item\n\n");
    }
    #[test]
    fn nested_items_continue_and_empty_ones_move_up() {
        let p = Projection::new("- a\n  - b");
        let (source, _) = p.replace(p.text.len()..p.text.len(), "\n");
        assert_eq!(source, "- a\n  - b\n  - ");
        let p = Projection::new(&source);
        let (source, caret) = p.replace(p.text.len()..p.text.len(), "\n");
        assert_eq!((source.as_str(), caret), ("- a\n  - b\n- ", 12));
        let p = Projection::new("1. a\n   1. b");
        let (source, _) = p.replace(p.text.len()..p.text.len(), "\n");
        assert_eq!(source, "1. a\n   1. b\n   2. ");
    }
    #[test]
    fn break_at_line_start_keeps_one_marker() {
        let p = Projection::new("```\n- code\n```");
        assert_eq!(p.replace(0..0, "\n").0, "```\n\n- code\n```");
        for (source, line) in [
            ("- テキスト", 0),
            ("- a\n- テキスト", 4),
            ("- a\n  - テキスト", 4),
        ] {
            let p = Projection::new(source);
            let marker = crate::syntax::list_prefix(&source[line..]).unwrap();
            for at in [line, line + marker.0] {
                let display = p.display_position(at);
                let (actual, caret) = p.replace(display..display, "\n");
                let expected = format!(
                    "{}{}\n{}テキスト",
                    &source[..line],
                    &marker.1[..marker.0],
                    &marker.1[..marker.0]
                );
                assert_eq!(actual, expected, "{source:?} at {at}");
                assert_eq!(&actual[caret..], "テキスト");
            }
        }
    }
    #[test]
    fn inline_breaks_preserve_delimiters_and_cursor() {
        for (source, at, expected) in [
            ("**bold**", 4, "**bold**\n"),
            ("**bold**", 2, "**bo**\n**ld**"),
            ("**bold**", 0, "\n**bold**"),
            ("*italic*", 3, "*ita*\n*lic*"),
            ("`code`", 2, "`co`\n`de`"),
            ("***bold***", 2, "***bo***\n***ld***"),
            ("[link](a.md)", 2, "[li](a.md)\n[nk](a.md)"),
            ("**bold word**", 5, "**bold** \n**word**"),
            ("# **見出し**", 9, "# **見出し**\n"),
            ("> **引用**", 6, "> **引用**\n> "),
            ("- **bold**", 8, "- **bold**\n- "),
        ] {
            let p = Projection::new(source);
            let (actual, caret) = p.replace(at..at, "\n");
            assert_eq!(actual, expected, "{source} at {at}");
            let p = Projection::new(&actual);
            let at = p.display_position(caret);
            let (typed, _) = p.replace(at..at, "あ");
            assert!(
                Projection::new(&typed).text.contains("\nあ")
                    || Projection::new(&typed).text.contains("\n\u{2003} あ"),
                "{typed}"
            );
        }
    }
    #[test]
    fn repeated_breaks_selection_and_multiline_paste() {
        let p = Projection::new("**bold**");
        assert_eq!(p.replace(4..4, "\n\n本文").0, "**bold**\n\n本文");
        assert_eq!(p.replace(1..3, "\n").0, "**b**\n**d**");
        assert_eq!(p.replace(0..4, "\n").0, "\n");
        assert_eq!(p.replace(2..2, "\r\n").0, "**bo**\r\n**ld**");
        let p = Projection::new("**bo**\n**ld**");
        assert_eq!(p.replace(2..3, "").0, "**bold**");
        let p = Projection::new("> 引用");
        let (s, _) = p.replace(p.text.len()..p.text.len(), "\n");
        let p = Projection::new(&s);
        assert_eq!(p.replace(p.text.len()..p.text.len(), "\n").0, "> 引用\n\n");
        for source in [
            "> quote\n> next\n\ntext",
            "- item\n- next\n\ntext",
            "**bold**\n\n*italic*",
            "# Heading\n\n## Next",
        ] {
            assert_eq!(Projection::new("").replace(0..0, source).0, source);
        }
    }
}
