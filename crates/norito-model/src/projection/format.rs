//! Toggle inline delimiters (`**`, `*`, `` ` ``, `~~`) over a display selection.
use super::{Cell, Projection};
use crate::text::changed_range;
use std::ops::Range;

fn is_bold(marker: &str) -> bool {
    matches!(marker, "**" | "__")
}

impl Projection {
    /// Returns the new source and the selection to keep, or `None` when the
    /// selection cannot be formatted without breaking other constructs.
    pub fn format(&self, display: Range<usize>, marker: &str) -> Option<(String, Range<usize>)> {
        // Bold over a partly bold selection makes all of it bold first.
        let force_add = marker == "**"
            && self.cells.iter().any(|cell| {
                cell.display.end > display.start
                    && cell.display.start < display.end
                    && !cell.style.syntax
                    && !cell.style.bold
                    && !self.text[cell.display.clone()].trim().is_empty()
            });
        self.format_selection(display, marker, force_add)
    }
    fn format_selection(
        &self,
        display: Range<usize>,
        marker: &str,
        force_add: bool,
    ) -> Option<(String, Range<usize>)> {
        if display.is_empty() {
            return None;
        }
        let selected = self.text.get(display.clone())?;
        if selected.contains('\n') || selected.trim() != selected {
            return self.format_lines(display, selected, marker, force_add);
        }
        let content_cell = |c: &&Cell| {
            marker != "**"
                || !self.wrappers.iter().any(|w| {
                    is_bold(&w.marker)
                        && w.outer.contains(&c.source.start)
                        && !w.inner.contains(&c.source.start)
                })
        };
        let first = self
            .cells
            .iter()
            .filter(content_cell)
            .find(|c| c.display.end > display.start)?;
        let last = self
            .cells
            .iter()
            .rev()
            .filter(content_cell)
            .find(|c| c.display.start < display.end)?;
        let mut range = first.source.start..last.source.end;
        if marker == "**" {
            // Bold absorbs every other construct whose content it covers.
            loop {
                let previous = range.clone();
                for w in &self.wrappers {
                    if !is_bold(&w.marker)
                        && range.start <= w.inner.start
                        && w.inner.end <= range.end
                    {
                        range.start = range.start.min(w.outer.start);
                        range.end = range.end.max(w.outer.end);
                    }
                }
                if range == previous {
                    break;
                }
            }
        }
        // Formatting commands stay within a single paragraph and outside code blocks.
        if self.source[range.clone()].contains('\n')
            || self.cells.iter().any(|c| {
                c.display.end > display.start
                    && c.display.start < display.end
                    && (c.style.block_code || (c.style.code && marker != "`"))
            })
        {
            return None;
        }
        let mut source = self.source.clone();
        if let Some(w) = self
            .wrappers
            .iter()
            .filter(|w| {
                (w.marker == marker || (marker == "**" && w.marker == "__"))
                    && w.inner.start <= range.start
                    && w.inner.end >= range.end
            })
            .min_by_key(|w| w.outer.len())
        {
            // Inside the same construct: remove it from the selection only.
            if force_add {
                return Some((source, range));
            }
            let before = wrap_trimmed(&self.source[w.inner.start..range.start], marker);
            let after = wrap_trimmed(&self.source[range.end..w.inner.end], marker);
            let selected = &self.source[range.clone()];
            let start = w.outer.start + before.len();
            source.replace_range(w.outer.clone(), &format!("{before}{selected}{after}"));
            return Some((source, start..start + selected.len()));
        }
        if marker == "**" {
            let affected: Vec<_> = self
                .wrappers
                .iter()
                .filter(|w| {
                    is_bold(&w.marker) && w.outer.start < range.end && w.outer.end > range.start
                })
                .collect();
            if !affected.is_empty() {
                // Overlapping bold runs merge into one, without nested markers.
                let start = affected
                    .iter()
                    .fold(range.start, |at, w| at.min(w.outer.start));
                let end = affected.iter().fold(range.end, |at, w| at.max(w.outer.end));
                // Keep other inline constructs intact; never cross their delimiters.
                if self.wrappers.iter().any(|w| {
                    !is_bold(&w.marker)
                        && w.outer.start < end
                        && w.outer.end > start
                        && !(start <= w.outer.start && w.outer.end <= end)
                        && !(w.inner.start <= start && end <= w.inner.end)
                }) {
                    return None;
                }
                let mut body = String::new();
                let mut selected_start = 0;
                let mut selected_end = 0;
                for (offset, ch) in self.source[start..end].char_indices() {
                    let at = start + offset;
                    if at == range.start {
                        selected_start = body.len();
                    }
                    if !affected
                        .iter()
                        .any(|w| w.outer.contains(&at) && !w.inner.contains(&at))
                    {
                        body.push(ch);
                    }
                    if at + ch.len_utf8() == range.end {
                        selected_end = body.len();
                    }
                }
                source.replace_range(start..end, &format!("**{body}**"));
                return Some((source, start + 2 + selected_start..start + 2 + selected_end));
            }
        }
        // Do not create crossing delimiters or nest the same delimiter ambiguously.
        if self.wrappers.iter().any(|w| {
            w.outer.start < range.end
                && w.outer.end > range.start
                && !(w.inner.start <= range.start && w.inner.end >= range.end)
                && !(marker == "**" && range.start <= w.outer.start && w.outer.end <= range.end)
        }) {
            return None;
        }
        if self.source[range.clone()].trim() != &self.source[range.clone()]
            || (marker != "**" && self.source[range.clone()].contains(marker))
        {
            return None;
        }
        source.insert_str(range.end, marker);
        source.insert_str(range.start, marker);
        Some((source, range.start + marker.len()..range.end + marker.len()))
    }
    /// Format each line separately so newlines and edge spaces stay outside markers.
    fn format_lines(
        &self,
        display: Range<usize>,
        selected: &str,
        marker: &str,
        force_add: bool,
    ) -> Option<(String, Range<usize>)> {
        let mut edits = Vec::new();
        let mut offset = display.start;
        for line in selected.split_inclusive('\n') {
            let core = line.trim();
            if !core.is_empty() {
                let start = offset + line.len() - line.trim_start().len();
                let (updated, _) =
                    self.format_selection(start..start + core.len(), marker, force_add)?;
                if updated != self.source {
                    let (old, new) = changed_range(&self.source, &updated);
                    edits.push((old, updated[new].to_owned()));
                }
            }
            offset += line.len();
        }
        let start =
            self.source_position(display.start + selected.len() - selected.trim_start().len());
        let end = self.source_position(display.end - (selected.len() - selected.trim_end().len()));
        if edits.windows(2).any(|pair| pair[0].0.end > pair[1].0.start) {
            return None;
        }
        let mut source = self.source.clone();
        let mut end = end as isize;
        for (range, replacement) in edits.into_iter().rev() {
            end += replacement.len() as isize - range.len() as isize;
            source.replace_range(range, &replacement);
        }
        Some((source, start..end as usize))
    }
}

/// `text` with its non-blank core wrapped in `marker`, edge whitespace kept outside.
fn wrap_trimmed(text: &str, marker: &str) -> String {
    let core = text.trim();
    if core.is_empty() {
        return text.into();
    }
    let start = text.len() - text.trim_start().len();
    format!(
        "{}{marker}{core}{marker}{}",
        &text[..start],
        &text[start + core.len()..]
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bold_selection_unifies_existing_bold_without_nested_markers() {
        for (source, expected) in [
            ("before **bold** after", "**before bold after**"),
            ("**one** and **two**", "**one and two**"),
            ("前 __太字__ 後", "**前 太字 後**"),
            (
                "before **bold** and *italic*",
                "**before bold and *italic***",
            ),
            ("**one**\ntwo\n", "**one**\n**two**\n"),
        ] {
            for p in [
                Projection::new(source),
                Projection::editable(source, Some(8)),
                Projection::raw(source),
            ] {
                let (result, _) = p
                    .format(0..p.text.len(), "**")
                    .unwrap_or_else(|| panic!("format failed: {source:?}, display {:?}", p.text));
                assert_eq!(result, expected, "{source:?}");
                let rendered = Projection::new(&result);
                assert_eq!(rendered.text, Projection::new(source).text);
                assert!(
                    rendered
                        .cells
                        .iter()
                        .filter(|c| !rendered.text[c.display.clone()].trim().is_empty())
                        .all(|c| c.style.bold)
                );
            }
        }
        let p = Projection::new("left **bold** right");
        let start = p.text.find("ld").unwrap();
        assert_eq!(
            p.format(start..p.text.len(), "**").unwrap().0,
            "left **bold right**"
        );
        let p = Projection::new("left **bold** right");
        let end = p.text.find("ld").unwrap();
        assert_eq!(p.format(0..end, "**").unwrap().0, "**left bold** right");
    }
    #[test]
    fn formatting_multiple_lines_keeps_newlines_outside_markers() {
        for newline in ["\n", "\r\n"] {
            for marker in ["**", "*", "~~", "`"] {
                let source = format!("一行目{newline}{newline}二行目{newline}");
                let p = Projection::new(&source);
                let (formatted, selected) = p.format(0..p.text.len(), marker).unwrap();
                assert_eq!(
                    formatted,
                    format!(
                        "{marker}一行目{marker}{newline}{newline}{marker}二行目{marker}{newline}"
                    )
                );
                assert!(!formatted[selected].ends_with(['\r', '\n']));
            }
        }
        let p = Projection::new("本文\n");
        assert_eq!(p.format(0..p.text.len(), "**").unwrap().0, "**本文**\n");
    }
    #[test]
    fn partial_bold_toggle_splits_existing_style_without_broken_delimiters() {
        let p = Projection::new("**hello world**");
        assert_eq!(p.format(6..11, "**").unwrap().0, "**hello** world");
    }
    #[test]
    fn formatting_round_trips() {
        let p = Projection::new("日本語 text");
        let (source, _) = p.format(0..9, "**").unwrap();
        assert_eq!(source, "**日本語** text");
        let p = Projection::new(&source);
        assert_eq!(p.format(0..9, "**").unwrap().0, "日本語 text");
    }
}
