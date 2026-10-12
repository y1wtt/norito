//! Source -> display: which bytes are hidden, replaced or styled, then the cells.
use super::{Cell, Projection, TextStyle, Wrapper};
use crate::syntax::list_prefix;
use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Parser, Tag};
use std::{collections::BTreeMap, ops::Range};

impl Projection {
    pub(super) fn build(
        source: &str,
        active: Option<usize>,
        interactive: bool,
        raw_view: bool,
    ) -> Self {
        let mut marks = Marks::new(source, active, interactive, raw_view);
        for (event, range) in Parser::new_ext(source, crate::syntax::options()).into_offset_iter() {
            marks.event(event, range);
        }
        // Optional inline extensions, outside parsed code. Preserve original
        // source ranges so revealing their delimiters uses the same edit path.
        for marker in ["==", "^", "~"] {
            marks.extension(marker);
        }
        if interactive {
            marks.reveal_and_collapse();
        }
        if raw_view {
            marks.hidden.fill(false);
            marks.replacements.clear();
        }
        marks.detach_closing_fences();
        marks.emit()
    }
}

/// Per-byte decisions collected while scanning one source.
struct Marks<'a> {
    source: &'a str,
    active: Option<usize>,
    interactive: bool,
    raw_view: bool,
    hidden: Vec<bool>,
    styles: Vec<TextStyle>,
    replacements: BTreeMap<usize, (Range<usize>, String)>,
    wrappers: Vec<Wrapper>,
    prefixes: Vec<Range<usize>>,
    fence_ends: Vec<Range<usize>>,
    revealed: Vec<Range<usize>>,
    /// Fenced code and whether it is a diagram.
    code_blocks: Vec<(Range<usize>, bool)>,
    /// Tables (true) and images (false), collapsed to a single cell.
    blocks: Vec<(Range<usize>, bool)>,
    math: Vec<Range<usize>>,
}

impl<'a> Marks<'a> {
    fn new(source: &'a str, active: Option<usize>, interactive: bool, raw_view: bool) -> Self {
        Self {
            source,
            active,
            interactive,
            raw_view,
            hidden: vec![false; source.len()],
            styles: vec![TextStyle::default(); source.len()],
            replacements: BTreeMap::new(),
            wrappers: vec![],
            prefixes: vec![],
            fence_ends: vec![],
            revealed: vec![],
            code_blocks: vec![],
            blocks: vec![],
            math: vec![],
        }
    }
    /// The caret is inside `range`, so it shows its source.
    fn editing(&self, range: &Range<usize>) -> bool {
        self.active.is_some_and(|at| range.contains(&at))
    }
    fn style(&mut self, range: Range<usize>, set: impl Fn(&mut TextStyle)) {
        self.styles[range].iter_mut().for_each(set);
    }
    fn replace(&mut self, range: Range<usize>, value: impl Into<String>) {
        self.replacements.insert(range.start, (range, value.into()));
    }
    /// End of `range` without its trailing line break.
    fn line_end(&self, range: &Range<usize>) -> usize {
        range.start
            + self.source[range.clone()]
                .trim_end_matches(['\r', '\n'])
                .len()
    }
    fn wrap(&mut self, outer: Range<usize>, open: usize, close: usize, marker: &str) {
        self.hidden[outer.start..outer.start + open].fill(true);
        self.hidden[outer.end - close..outer.end].fill(true);
        self.wrappers.push(Wrapper {
            inner: outer.start + open..outer.end - close,
            outer,
            marker: marker.into(),
        });
    }

    fn event(&mut self, event: Event<'_>, range: Range<usize>) {
        let source = self.source;
        let raw = &source[range.clone()];
        match event {
            Event::Start(Tag::MetadataBlock(_)) => self.style(range, |s| {
                s.code = true;
                s.block_code = true;
            }),
            Event::FootnoteReference(ref label) => {
                self.style(range.clone(), |s| {
                    s.link = true;
                    s.small = true;
                });
                if !self.raw_view && !self.editing(&range) {
                    self.replace(range, format!("[{label}]"));
                }
            }
            Event::InlineMath(ref value) | Event::DisplayMath(ref value) => {
                self.math.push(range.clone());
                if self.interactive && !self.editing(&range) {
                    let display = matches!(event, Event::DisplayMath(_));
                    self.styles[range.start].math = true;
                    self.styles[range.start].diagram = display;
                    self.replace(
                        range,
                        if display {
                            " ".into()
                        } else {
                            "\u{2003}".repeat(value.chars().count().clamp(2, 32))
                        },
                    );
                }
            }
            Event::Start(Tag::Table(_)) => self.blocks.push((range, true)),
            Event::Start(Tag::Image { .. }) => self.blocks.push((range, false)),
            Event::Rule => {
                if self.interactive && !self.editing(&range) {
                    let end = self.line_end(&range);
                    self.styles[range.start].rule = true;
                    self.replace(range.start..end, " ");
                }
            }
            Event::TaskListMarker(checked) => {
                if !self.raw_view && !self.editing(&range) {
                    self.replace(range, if checked { "☑" } else { "☐" });
                }
            }
            Event::Text(value) if raw != &*value && value.chars().count() == 1 => {
                self.replace(range, value.into_string());
            }
            Event::Start(Tag::Heading { level, .. }) => self.heading(level, range),
            Event::Start(Tag::Strong)
            | Event::Start(Tag::Emphasis)
            | Event::Start(Tag::Strikethrough) => {
                let strong = matches!(event, Event::Start(Tag::Strong));
                let strike = matches!(event, Event::Start(Tag::Strikethrough));
                let n = if strong || strike { 2 } else { 1 };
                if raw.len() >= 2 * n {
                    self.wrap(range.clone(), n, n, &raw[..n]);
                    self.style(range, |s| {
                        if strong {
                            s.bold = true;
                        } else if strike {
                            s.strike = true;
                        } else {
                            s.italic = true;
                        }
                    });
                }
            }
            Event::Code(value) => {
                let n = raw.bytes().take_while(|b| *b == b'`').count();
                if n > 0 && raw.len() >= n * 2 && raw[n..raw.len() - n] == *value {
                    self.wrap(range.clone(), n, n, &"`".repeat(n));
                    self.style(range, |s| s.code = true);
                }
            }
            Event::Start(Tag::Link { .. }) => {
                if raw.starts_with('[')
                    && let Some(close) = raw.find(']')
                {
                    self.wrap(range.clone(), 1, range.len() - close, "link");
                    self.style(range, |s| s.link = true);
                }
            }
            Event::Start(Tag::Item) => {
                if let Some((len, _)) = list_prefix(raw) {
                    let prefix = range.start..range.start + len;
                    let label = if raw.starts_with(['-', '*', '+']) {
                        "• ".into()
                    } else {
                        raw[..len].to_owned()
                    };
                    self.replace(prefix.clone(), label);
                    self.prefixes.push(prefix);
                }
            }
            Event::Start(Tag::BlockQuote(_)) => {
                self.style(range.clone(), |s| s.quote = true);
                let mut offset = range.start;
                for line in raw.split_inclusive('\n') {
                    if line.starts_with('>') {
                        let n = if line.starts_with("> ") { 2 } else { 1 };
                        self.hidden[offset..offset + n].fill(true);
                        self.prefixes.push(offset..offset + n);
                    }
                    offset += line.len();
                }
            }
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(ref language))) => {
                self.fenced(range, crate::syntax::is_mermaid(language))
            }
            Event::Start(Tag::CodeBlock(CodeBlockKind::Indented)) => self.style(range, |s| {
                s.code = true;
                s.block_code = true;
            }),
            _ => {}
        }
    }

    fn heading(&mut self, level: HeadingLevel, range: Range<usize>) {
        let source = self.source;
        let raw = &source[range.clone()];
        let level = match level {
            HeadingLevel::H1 => 1,
            HeadingLevel::H2 => 2,
            HeadingLevel::H3 => 3,
            HeadingLevel::H4 => 4,
            HeadingLevel::H5 => 5,
            HeadingLevel::H6 => 6,
        };
        self.style(range.clone(), |s| s.heading = level);
        let prefix = raw.bytes().take_while(|b| *b == b'#').count();
        if prefix > 0
            && raw
                .as_bytes()
                .get(prefix)
                .is_some_and(u8::is_ascii_whitespace)
        {
            // ATX: hide the opening hashes and any closing sequence.
            let end = range.start
                + prefix
                + raw[prefix..]
                    .bytes()
                    .take_while(|b| *b == b' ' || *b == b'\t')
                    .count();
            self.hidden[range.start..end].fill(true);
            self.prefixes.push(range.start..end);
            let line_end = self.line_end(&range);
            let line = &source[end..line_end];
            let hashes = line.bytes().rev().take_while(|b| *b == b'#').count();
            if hashes > 0
                && line.len() > hashes
                && line.as_bytes()[line.len() - hashes - 1].is_ascii_whitespace()
            {
                self.hidden[line_end - hashes - 1..line_end].fill(true);
            }
        } else if let Some(at) = raw.trim_end_matches(['\r', '\n']).rfind('\n') {
            // Setext: hide the underline.
            self.hidden[range.start + at..range.end].fill(true);
        }
    }

    fn fenced(&mut self, range: Range<usize>, mermaid: bool) {
        let source = self.source;
        let raw = &source[range.clone()];
        self.code_blocks.push((range.clone(), mermaid));
        self.style(range.clone(), |s| {
            s.code = true;
            s.block_code = true;
        });
        let first_end = raw.find('\n').unwrap_or(raw.len());
        let trimmed = raw.trim_end_matches(['\r', '\n']);
        let body_start = (first_end + 1).min(raw.len());
        let closing = trimmed
            .rfind('\n')
            .map(|at| at + 1)
            .filter(|at| crate::syntax::is_fence(trimmed[*at..].trim()));
        let body_end = closing.unwrap_or(raw.len());
        // An empty fence stays visible so it has an unambiguous input location.
        if body_start >= body_end || raw[body_start..body_end].trim().is_empty() {
            return;
        }
        self.hidden[range.start..range.start + body_start].fill(true);
        if let Some(at) = closing {
            let fence = range.start + at..range.start + trimmed.len();
            self.hidden[fence.clone()].fill(true);
            self.fence_ends.push(fence);
        }
    }

    /// `==highlight==`, `^small^` and `~small~` on a single line.
    fn extension(&mut self, marker: &str) {
        let source = self.source;
        let first = marker.as_bytes()[0];
        let mut offset = 0;
        while let Some(relative) = source[offset..].find(marker) {
            let start = offset + relative;
            offset = start + marker.len();
            if self.styles[start].code
                || self.math.iter().any(|r| r.contains(&start))
                || (start > 0 && [b'\\', first].contains(&source.as_bytes()[start - 1]))
                || source.as_bytes().get(offset) == Some(&first)
            {
                continue;
            }
            let Some(end_at) = source[offset..].find(marker) else {
                break;
            };
            let end = offset + end_at;
            let inner = &source[offset..end];
            if inner.is_empty() || inner.contains('\n') || inner.trim() != inner {
                continue;
            }
            let outer = start..end + marker.len();
            self.wrap(outer.clone(), marker.len(), marker.len(), marker);
            self.style(offset..end, |s| {
                if marker == "==" {
                    s.highlight = true;
                } else {
                    s.small = true;
                }
            });
            offset = outer.end;
        }
    }

    /// Show the delimiters around the caret; fold diagrams, tables and images elsewhere.
    fn reveal_and_collapse(&mut self) {
        let active = self.active;
        for wrapper in &self.wrappers {
            if active.is_some_and(|at| wrapper.outer.start <= at && at <= wrapper.outer.end) {
                self.hidden[wrapper.outer.clone()].fill(false);
                self.revealed.push(wrapper.outer.clone());
                for at in (wrapper.outer.start..wrapper.inner.start)
                    .chain(wrapper.inner.end..wrapper.outer.end)
                {
                    self.styles[at].syntax = true;
                }
            }
        }
        for (range, mermaid) in std::mem::take(&mut self.code_blocks) {
            if self.editing(&range) {
                self.hidden[range.clone()].fill(false);
                self.revealed.push(range);
            } else if mermaid {
                let end = self.line_end(&range);
                if end > range.start {
                    self.style(range.start..end, |s| s.diagram = true);
                    self.replace(range.start..end, " ");
                }
            }
        }
        for (range, table) in std::mem::take(&mut self.blocks) {
            if !self.editing(&range) {
                let end = self.line_end(&range);
                self.styles[range.start].table = table;
                self.styles[range.start].image = !table;
                self.replace(range.start..end, " ");
            }
        }
    }

    /// A hidden closing fence is a paragraph boundary, not an extra code row.
    fn detach_closing_fences(&mut self) {
        let source = self.source.as_bytes();
        for fence in &self.fence_ends {
            if self.hidden[fence.start] {
                let mut end = fence.end;
                while end < source.len() && matches!(source[end], b'\r' | b'\n') {
                    end += 1;
                }
                for style in &mut self.styles[fence.start..end] {
                    style.block_code = false;
                    style.code = false;
                }
            }
        }
    }

    fn emit(self) -> Projection {
        let source = self.source;
        let styles = &self.styles;
        let mut text = String::new();
        let mut cells = vec![];
        let mut push = |display: &str, range: Range<usize>, style: TextStyle| {
            let start = text.len();
            text.push_str(display);
            cells.push(Cell {
                display: start..text.len(),
                source: range,
                style,
            });
        };
        // A line takes the block styles of every byte up to its break, hidden or not.
        let mut at = 0;
        let mut line = TextStyle::default();
        let mut lines = vec![];
        while at < source.len() {
            let style = styles[at];
            line.heading = line.heading.max(style.heading);
            line.block_code |= style.block_code;
            line.code |= style.block_code;
            line.quote |= style.quote;
            line.diagram |= style.diagram;
            line.table |= style.table;
            line.image |= style.image;
            line.rule |= style.rule;
            line.math |= style.math;
            if let Some((range, value)) = self.replacements.get(&at) {
                if source[range.clone()].chars().count() != value.chars().count() {
                    push(value, range.clone(), style);
                } else {
                    // Same length: keep a per-character mapping for precise carets.
                    for ((offset, original), c) in
                        source[range.clone()].char_indices().zip(value.chars())
                    {
                        let from = range.start + offset;
                        push(
                            c.encode_utf8(&mut [0; 4]),
                            from..from + original.len_utf8(),
                            style,
                        );
                    }
                }
                for _ in value.chars().filter(|c| *c == '\n') {
                    lines.push(std::mem::take(&mut line));
                }
                at = range.end;
                continue;
            }
            let c = source[at..].chars().next().unwrap();
            if !self.hidden[at] {
                push(c.encode_utf8(&mut [0; 4]), at..at + c.len_utf8(), style);
                if c == '\n' {
                    lines.push(std::mem::take(&mut line));
                }
            }
            at += c.len_utf8();
        }
        lines.push(line);
        Projection {
            source: source.into(),
            text,
            cells,
            line_styles: lines,
            wrappers: self.wrappers,
            prefixes: self.prefixes,
            fence_ends: self.fence_ends,
            revealed: self.revealed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn closing_fence_is_not_an_extra_code_row() {
        for source in [
            "```rust\nlet x = 1;\n```",
            "```rust\nlet x = 1;\n```\n\n本文",
        ] {
            let p = Projection::editable(source, None);
            assert_eq!(p.line_styles.iter().filter(|s| s.block_code).count(), 1);
            let editing = Projection::editable(source, Some(10));
            assert!(editing.line_styles[2].block_code);
        }
    }
    #[test]
    fn hides_syntax_and_preserves_formats_when_typing() {
        let p =
            Projection::new("# Title\n\nA **bold** and *italic* [link](https://example.com).\n");
        assert_eq!(p.text, "Title\n\nA bold and italic link.\n");
        let at = p.text.find("bold").unwrap() + 2;
        let (source, _) = p.replace(at..at, "日本語");
        assert_eq!(
            source,
            "# Title\n\nA **bo日本語ld** and *italic* [link](https://example.com).\n"
        );
        assert!(p.cells.iter().any(|c| c.style.heading == 1));
    }
    #[test]
    fn collapsed_blocks_delete_atomically_and_reveal_losslessly() {
        for block in [
            "```mermaid\nflowchart LR; A-->B\n```",
            "| a | b |\n| --- | --- |\n| c | d |",
            "![image](assets/a.png)",
            "$$x^2$$",
        ] {
            let source = format!("before\n\n{block}\n\nafter");
            let p = Projection::editable(&source, None);
            let cell = p
                .cells
                .iter()
                .find(|c| c.source.len() > c.display.len() && c.source.start == 8)
                .unwrap();
            let (deleted, _) = p.replace(cell.display.clone(), "");
            assert_eq!(deleted, "before\n\n\n\nafter");
            let editing = Projection::editable(&source, Some(8));
            assert!(editing.text.contains(block));
            assert_eq!(Projection::raw(&source).text, source);
        }
    }
    #[test]
    fn metadata_and_reference_labels_remain_readable() {
        let source = "---\ntitle: draft\n---\n\n# Title\n\n[page][p]\n\n[p]: other.md\n";
        assert!(Projection::editable(source, None).text.contains("page"));
        assert_eq!(Projection::raw(source).text, source);
    }
    #[test]
    fn entity_projection_keeps_original_encoding_on_unrelated_edit() {
        let p = Projection::new("A &amp; B");
        assert_eq!(p.text, "A & B");
        assert_eq!(p.replace(0..1, "X").0, "X &amp; B");
    }
    #[test]
    fn code_content_edits_do_not_modify_fence_language() {
        let p = Projection::new("```rust\nlet x = 1;\n```\n");
        assert!(p.text.starts_with("let x"));
        assert_eq!(p.replace(0..0, "// ").0, "```rust\n// let x = 1;\n```\n");
        let empty = "```\n\n```\n";
        assert_eq!(Projection::new(empty).text, empty);
    }
}
