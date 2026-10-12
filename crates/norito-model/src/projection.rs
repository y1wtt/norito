//! Loss-preserving Markdown -> editable text projection. All positions are UTF-8 bytes.
use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Parser, Tag};
use std::{collections::BTreeMap, ops::Range};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TextStyle {
    pub heading: u8,
    pub bold: bool,
    pub italic: bool,
    pub code: bool,
    pub block_code: bool,
    pub link: bool,
    pub quote: bool,
    pub strike: bool,
    pub diagram: bool,
    pub syntax: bool,
    pub table: bool,
    pub image: bool,
    pub rule: bool,
    pub math: bool,
    pub highlight: bool,
    pub small: bool,
}
#[derive(Debug, Clone)]
pub struct Cell {
    pub display: Range<usize>,
    pub source: Range<usize>,
    pub style: TextStyle,
}
#[derive(Debug, Clone)]
struct Wrapper {
    outer: Range<usize>,
    inner: Range<usize>,
    marker: String,
}
#[derive(Debug, Clone)]
pub struct Projection {
    pub source: String,
    pub text: String,
    pub cells: Vec<Cell>,
    /// Block formatting survives even when all characters of a line are hidden.
    pub line_styles: Vec<TextStyle>,
    wrappers: Vec<Wrapper>,
    prefixes: Vec<Range<usize>>,
    fence_ends: Vec<Range<usize>>,
    revealed: Vec<Range<usize>>,
}

impl Projection {
    pub fn new(source: &str) -> Self {
        Self::build(source, None, false, false)
    }
    pub fn editable(source: &str, active: Option<usize>) -> Self {
        Self::build(source, active, true, false)
    }
    pub fn raw(source: &str) -> Self {
        Self::build(source, None, false, true)
    }
    fn build(source: &str, active: Option<usize>, interactive: bool, raw_view: bool) -> Self {
        let mut hidden = vec![false; source.len()];
        let mut styles = vec![TextStyle::default(); source.len()];
        let mut replacements = BTreeMap::<usize, (Range<usize>, String)>::new();
        let mut wrappers = vec![];
        let mut prefixes = vec![];
        let mut fence_ends = vec![];
        let mut code_blocks = vec![];
        let mut revealed = vec![];
        let mut blocks = vec![];
        let mut math_ranges = vec![];
        for (event, range) in Parser::new_ext(source, crate::markdown_options()).into_offset_iter()
        {
            let raw = &source[range.clone()];
            match event {
                Event::Start(Tag::MetadataBlock(_)) => {
                    for style in &mut styles[range] {
                        style.code = true;
                        style.block_code = true;
                    }
                }
                Event::FootnoteReference(ref label) => {
                    for style in &mut styles[range.clone()] {
                        style.link = true;
                        style.small = true;
                    }
                    if !raw_view && !active.is_some_and(|at| range.contains(&at)) {
                        replacements.insert(range.start, (range.clone(), format!("[{label}]")));
                    }
                }
                Event::InlineMath(ref value) | Event::DisplayMath(ref value) => {
                    math_ranges.push(range.clone());
                    if interactive && !active.is_some_and(|at| range.contains(&at)) {
                        let display = matches!(event, Event::DisplayMath(_));
                        styles[range.start].math = true;
                        styles[range.start].diagram = display;
                        replacements.insert(
                            range.start,
                            (
                                range.clone(),
                                if display {
                                    " ".into()
                                } else {
                                    "\u{2003}".repeat(value.chars().count().clamp(2, 32))
                                },
                            ),
                        );
                    }
                }
                Event::Start(Tag::Table(_)) => blocks.push((range.clone(), true)),
                Event::Start(Tag::Image { .. }) => blocks.push((range.clone(), false)),
                Event::Rule => {
                    if interactive && !active.is_some_and(|at| range.contains(&at)) {
                        let end = range.start + raw.trim_end_matches(['\r', '\n']).len();
                        styles[range.start].rule = true;
                        replacements.insert(range.start, (range.start..end, " ".into()));
                    }
                }
                Event::TaskListMarker(checked) => {
                    if !raw_view && !active.is_some_and(|at| range.contains(&at)) {
                        replacements
                            .insert(range.start, (range, if checked { "☑" } else { "☐" }.into()));
                    }
                }
                Event::Text(value) if raw != &*value && value.chars().count() == 1 => {
                    replacements.insert(range.start, (range.clone(), value.into_string()));
                }
                Event::Start(Tag::Heading { level, .. }) => {
                    let level = match level {
                        HeadingLevel::H1 => 1,
                        HeadingLevel::H2 => 2,
                        HeadingLevel::H3 => 3,
                        HeadingLevel::H4 => 4,
                        HeadingLevel::H5 => 5,
                        HeadingLevel::H6 => 6,
                    };
                    for style in &mut styles[range.clone()] {
                        style.heading = level;
                    }
                    let prefix = raw.bytes().take_while(|b| *b == b'#').count();
                    if prefix > 0
                        && raw
                            .as_bytes()
                            .get(prefix)
                            .is_some_and(u8::is_ascii_whitespace)
                    {
                        let end = range.start
                            + prefix
                            + raw[prefix..]
                                .bytes()
                                .take_while(|b| *b == b' ' || *b == b'\t')
                                .count();
                        hidden[range.start..end].fill(true);
                        prefixes.push(range.start..end);
                        let line_end = range.start + raw.trim_end_matches(['\r', '\n']).len();
                        let line = &source[end..line_end];
                        let hashes = line.bytes().rev().take_while(|b| *b == b'#').count();
                        if hashes > 0
                            && line.len() > hashes
                            && line.as_bytes()[line.len() - hashes - 1].is_ascii_whitespace()
                        {
                            hidden[line_end - hashes - 1..line_end].fill(true);
                        }
                    } else if let Some(at) = raw.trim_end_matches(['\r', '\n']).rfind('\n') {
                        hidden[range.start + at..range.end].fill(true);
                    }
                }
                Event::Start(Tag::Strong)
                | Event::Start(Tag::Emphasis)
                | Event::Start(Tag::Strikethrough) => {
                    let strong = matches!(event, Event::Start(Tag::Strong));
                    let strike = matches!(event, Event::Start(Tag::Strikethrough));
                    let n = if strong || strike { 2 } else { 1 };
                    if raw.len() >= 2 * n {
                        hidden[range.start..range.start + n].fill(true);
                        hidden[range.end - n..range.end].fill(true);
                        wrappers.push(Wrapper {
                            outer: range.clone(),
                            inner: range.start + n..range.end - n,
                            marker: raw[..n].into(),
                        });
                        for style in &mut styles[range] {
                            if strong {
                                style.bold = true;
                            } else if strike {
                                style.strike = true;
                            } else {
                                style.italic = true;
                            }
                        }
                    }
                }
                Event::Code(value) => {
                    let n = raw.bytes().take_while(|b| *b == b'`').count();
                    if n > 0 && raw.len() >= n * 2 && raw[n..raw.len() - n] == *value {
                        hidden[range.start..range.start + n].fill(true);
                        hidden[range.end - n..range.end].fill(true);
                        wrappers.push(Wrapper {
                            outer: range.clone(),
                            inner: range.start + n..range.end - n,
                            marker: "`".repeat(n),
                        });
                        for style in &mut styles[range] {
                            style.code = true;
                        }
                    }
                }
                Event::Start(Tag::Link { .. }) => {
                    if raw.starts_with('[')
                        && let Some(close) = raw.find(']')
                    {
                        hidden[range.start] = true;
                        hidden[range.start + close..range.end].fill(true);
                        wrappers.push(Wrapper {
                            outer: range.clone(),
                            inner: range.start + 1..range.start + close,
                            marker: "link".into(),
                        });
                        for style in &mut styles[range] {
                            style.link = true;
                        }
                    }
                }
                Event::Start(Tag::Item) => {
                    if let Some((len, _)) = list_prefix(raw) {
                        let prefix = range.start..range.start + len;
                        let label = if raw.starts_with(['-', '*', '+']) {
                            "• ".into()
                        } else {
                            raw[..len].into()
                        };
                        replacements.insert(prefix.start, (prefix.clone(), label));
                        prefixes.push(prefix);
                    }
                }
                Event::Start(Tag::BlockQuote(_)) => {
                    for style in &mut styles[range.clone()] {
                        style.quote = true;
                    }
                    let mut offset = range.start;
                    for line in raw.split_inclusive('\n') {
                        if line.starts_with('>') {
                            let n = if line.starts_with("> ") { 2 } else { 1 };
                            hidden[offset..offset + n].fill(true);
                            prefixes.push(offset..offset + n);
                        }
                        offset += line.len();
                    }
                }
                Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(ref language))) => {
                    code_blocks.push((
                        range.clone(),
                        language.split_whitespace().next() == Some("mermaid"),
                    ));
                    for style in &mut styles[range.clone()] {
                        style.code = true;
                        style.block_code = true;
                    }
                    let first_end = raw.find('\n').unwrap_or(raw.len());
                    let trimmed = raw.trim_end_matches(['\r', '\n']);
                    let body_start = (first_end + 1).min(raw.len());
                    let closing = trimmed.rfind('\n').map(|at| at + 1).filter(|at| {
                        let last = trimmed[*at..].trim();
                        last.starts_with("```") || last.starts_with("~~~")
                    });
                    let body_end = closing.unwrap_or(raw.len());
                    // An empty fence stays visible so it has an unambiguous input location.
                    if body_start >= body_end || raw[body_start..body_end].trim().is_empty() {
                        continue;
                    }
                    hidden[range.start..range.start + body_start].fill(true);
                    if let Some(at) = trimmed.rfind('\n') {
                        let last = trimmed[at + 1..].trim();
                        if last.starts_with("```") || last.starts_with("~~~") {
                            hidden[range.start + at + 1..range.start + trimmed.len()].fill(true);
                            fence_ends.push(range.start + at + 1..range.start + trimmed.len());
                        }
                    }
                }
                Event::Start(Tag::CodeBlock(CodeBlockKind::Indented)) => {
                    for style in &mut styles[range] {
                        style.code = true;
                        style.block_code = true;
                    }
                }
                _ => {}
            }
        }
        // Optional inline extensions, outside parsed code. Preserve original
        // source ranges so revealing their delimiters uses the same edit path.
        for marker in ["==", "^", "~"] {
            let mut offset = 0;
            while let Some(relative) = source[offset..].find(marker) {
                let start = offset + relative;
                offset = start + marker.len();
                if styles[start].code
                    || math_ranges.iter().any(|r| r.contains(&start))
                    || (start > 0
                        && (source.as_bytes()[start - 1] == b'\\'
                            || source.as_bytes()[start - 1] == marker.as_bytes()[0]))
                    || source.as_bytes().get(offset) == Some(&marker.as_bytes()[0])
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
                hidden[start..offset].fill(true);
                hidden[end..outer.end].fill(true);
                for style in &mut styles[offset..end] {
                    if marker == "==" {
                        style.highlight = true;
                    } else {
                        style.small = true;
                    }
                }
                wrappers.push(Wrapper {
                    outer: outer.clone(),
                    inner: offset..end,
                    marker: marker.into(),
                });
                offset = outer.end;
            }
        }
        if interactive {
            for wrapper in &wrappers {
                if active.is_some_and(|at| wrapper.outer.start <= at && at <= wrapper.outer.end) {
                    hidden[wrapper.outer.clone()].fill(false);
                    revealed.push(wrapper.outer.clone());
                    for at in (wrapper.outer.start..wrapper.inner.start)
                        .chain(wrapper.inner.end..wrapper.outer.end)
                    {
                        styles[at].syntax = true;
                    }
                }
            }
            for (range, mermaid) in code_blocks {
                if active.is_some_and(|at| range.start <= at && at < range.end) {
                    hidden[range.clone()].fill(false);
                    revealed.push(range);
                } else if mermaid {
                    let end =
                        range.start + source[range.clone()].trim_end_matches(['\r', '\n']).len();
                    if end > range.start {
                        for style in &mut styles[range.start..end] {
                            style.diagram = true;
                        }
                        replacements.insert(range.start, (range.start..end, " ".into()));
                    }
                }
            }
            for (range, table) in blocks {
                if !active.is_some_and(|at| range.contains(&at)) {
                    let end =
                        range.start + source[range.clone()].trim_end_matches(['\r', '\n']).len();
                    styles[range.start].table = table;
                    styles[range.start].image = !table;
                    replacements.insert(range.start, (range.start..end, " ".into()));
                }
            }
        }
        if raw_view {
            hidden.fill(false);
            replacements.clear();
        }
        // A hidden closing fence is a paragraph boundary, not an extra code row.
        for fence in &fence_ends {
            if hidden[fence.start] {
                for style in &mut styles[fence.clone()] {
                    style.block_code = false;
                    style.code = false;
                }
                let mut end = fence.end;
                while end < source.len() && matches!(source.as_bytes()[end], b'\r' | b'\n') {
                    styles[end].block_code = false;
                    styles[end].code = false;
                    end += 1;
                }
            }
        }
        let mut text = String::new();
        let mut cells = vec![];
        let mut line_styles = vec![TextStyle::default()];
        let mut at = 0;
        while at < source.len() {
            let line = line_styles.last_mut().unwrap();
            line.heading = line.heading.max(styles[at].heading);
            line.block_code |= styles[at].block_code;
            line.code |= styles[at].block_code;
            line.quote |= styles[at].quote;
            line.diagram |= styles[at].diagram;
            line.table |= styles[at].table;
            line.image |= styles[at].image;
            line.rule |= styles[at].rule;
            line.math |= styles[at].math;
            if let Some((range, value)) = replacements.get(&at) {
                if source[range.clone()].chars().count() != value.chars().count() {
                    let start = text.len();
                    text.push_str(value);
                    for _ in value.chars().filter(|c| *c == '\n') {
                        line_styles.push(TextStyle::default());
                    }
                    cells.push(Cell {
                        display: start..text.len(),
                        source: range.clone(),
                        style: styles[at],
                    });
                    at = range.end;
                    continue;
                }
                for ((source_offset, original), c) in
                    source[range.clone()].char_indices().zip(value.chars())
                {
                    let start = text.len();
                    text.push(c);
                    if c == '\n' {
                        line_styles.push(TextStyle::default());
                    }
                    cells.push(Cell {
                        display: start..text.len(),
                        source: range.start + source_offset
                            ..range.start + source_offset + original.len_utf8(),
                        style: styles[at],
                    });
                }
                at = range.end;
                continue;
            }
            let c = source[at..].chars().next().unwrap();
            if !hidden[at] {
                let start = text.len();
                text.push(c);
                if c == '\n' {
                    line_styles.push(TextStyle::default());
                }
                cells.push(Cell {
                    display: start..text.len(),
                    source: at..at + c.len_utf8(),
                    style: styles[at],
                });
            }
            at += c.len_utf8();
        }
        Self {
            source: source.into(),
            text,
            cells,
            line_styles,
            wrappers,
            prefixes,
            fence_ends,
            revealed,
        }
    }
    pub fn source_position(&self, display: usize) -> usize {
        if let Some(cell) = self
            .cells
            .iter()
            .find(|c| c.display.start < display && display < c.display.end)
        {
            return cell.source.start;
        }
        let previous = self.cells.iter().rev().find(|c| c.display.end <= display);
        let next = self.cells.iter().find(|c| c.display.start >= display);
        match (previous, next) {
            (Some(prev), Some(next)) if self.text[prev.display.clone()].ends_with('\n') => {
                if self
                    .fence_ends
                    .iter()
                    .any(|fence| fence.start == prev.source.end && fence.end == next.source.start)
                {
                    // The visual spacer represents the closing fence. Typing
                    // here belongs after that complete source line.
                    return next.source.end;
                }
                next.source.start
            }
            (Some(prev), None) if self.text[prev.display.clone()].ends_with('\n') => {
                if let Some(fence) = self
                    .fence_ends
                    .iter()
                    .find(|fence| fence.start == prev.source.end)
                {
                    return fence.end;
                }
                // A trailing empty block has no visible cell after its marker.
                // Keep typing inside that block, rather than before its prefix.
                let mut at = prev.source.end;
                while let Some(prefix) = self.prefixes.iter().find(|prefix| prefix.start == at) {
                    at = prefix.end;
                }
                at
            }
            (Some(prev), _) => prev.source.end,
            (_, Some(next)) => next.source.start,
            _ => self.source.len(),
        }
    }
    pub fn display_position(&self, source: usize) -> usize {
        self.cells
            .iter()
            .find(|c| c.source.end > source)
            .map_or(self.text.len(), |c| c.display.start)
    }
    pub fn replace(&self, display: Range<usize>, inserted: &str) -> (String, usize) {
        let source_range = self.source_position(display.start)..self.source_position(display.end);
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
        // Enter and multiline paste share a structural edit path. A newline
        // must not leave whitespace inside the end of an inline delimiter.
        if inserted.contains('\n') {
            let (mut source, mut caret) = if display.is_empty() {
                (self.source.clone(), self.source_position(display.start))
            } else {
                self.replace(display, "")
            };
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
                        inserted == "\n" || inserted == "\r\n",
                    );
                }
            }
            return (source, caret);
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
        let insert = inserted.to_string();
        deletions.sort_by_key(|r| r.start);
        let mut merged: Vec<Range<usize>> = vec![];
        for r in deletions {
            if let Some(last) = merged.last_mut()
                && r.start <= last.end
            {
                last.end = last.end.max(r.end);
            } else {
                merged.push(r);
            }
        }
        let removed_before: usize = merged
            .iter()
            .map(|r| anchor.saturating_sub(r.start).min(r.len()))
            .sum();
        let insertion = anchor - removed_before;
        let mut source = self.source.clone();
        for r in merged.into_iter().rev() {
            source.replace_range(r, "");
        }
        source.insert_str(insertion, &insert);
        (source, insertion + insert.len())
    }
    fn break_line(&self, display: usize, newline: &str, continue_block: bool) -> (String, usize) {
        let anchor = self.source_position(display);
        let mut hidden = vec![false; self.source.len()];
        let mut before = BTreeMap::<usize, String>::new();
        let mut after = BTreeMap::<usize, String>::new();
        let mut active: Vec<_> = self
            .wrappers
            .iter()
            .filter(|w| w.inner.start <= anchor && anchor <= w.inner.end)
            .collect();
        active.sort_by_key(|w| w.outer.start);
        for w in active {
            let left = self.cells.iter().any(|c| {
                c.source.start >= w.inner.start
                    && c.source.end <= anchor
                    && !self.text[c.display.clone()].trim().is_empty()
            });
            let right = self.cells.iter().any(|c| {
                c.source.start >= anchor
                    && c.source.end <= w.inner.end
                    && !self.text[c.display.clone()].trim().is_empty()
            });
            let opening = &self.source[w.outer.start..w.inner.start];
            let closing = &self.source[w.inner.end..w.outer.end];
            if left {
                let trimmed = self.source[w.inner.start..anchor].trim_end().len();
                before
                    .entry(w.inner.start + trimmed)
                    .or_default()
                    .insert_str(0, closing);
            } else {
                hidden[w.outer.start..w.inner.start].fill(true);
            }
            if right {
                let tail = &self.source[anchor..w.inner.end];
                after
                    .entry(anchor + tail.len() - tail.trim_start().len())
                    .or_default()
                    .push_str(opening);
            } else {
                hidden[w.inner.end..w.outer.end].fill(true);
            }
        }
        let start = self.source[..anchor].rfind('\n').map_or(0, |at| at + 1);
        let line = self.source[start..].split('\n').next().unwrap_or("");
        let in_code = self
            .cells
            .iter()
            .any(|c| c.source.start <= anchor && c.source.end >= anchor && c.style.block_code);
        let mut insertion = newline.to_owned();
        if !in_code && continue_block {
            let quote_len = quote_prefix(line);
            let continuation = if quote_len > 0 {
                Some((quote_len, line[..quote_len].to_owned()))
            } else {
                list_prefix(line)
            };
            if let Some((len, next)) = continuation {
                if line[len..].trim().is_empty() {
                    hidden[start..start + len].fill(true);
                    // Keep an actual paragraph boundary. Without this blank
                    // line, later typing is parsed as lazy continuation.
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
    pub fn format(&self, display: Range<usize>, marker: &str) -> Option<(String, Range<usize>)> {
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
            let end =
                self.source_position(display.end - (selected.len() - selected.trim_end().len()));
            if edits.windows(2).any(|pair| pair[0].0.end > pair[1].0.start) {
                return None;
            }
            let mut source = self.source.clone();
            let mut end = end as isize;
            for (range, replacement) in edits.into_iter().rev() {
                end += replacement.len() as isize - range.len() as isize;
                source.replace_range(range, &replacement);
            }
            return Some((source, start..end as usize));
        }
        let content_cell = |c: &&Cell| {
            marker != "**"
                || !self.wrappers.iter().any(|w| {
                    matches!(w.marker.as_str(), "**" | "__")
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
            loop {
                let previous = range.clone();
                for w in &self.wrappers {
                    if !matches!(w.marker.as_str(), "**" | "__")
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
                    matches!(w.marker.as_str(), "**" | "__")
                        && w.outer.start < range.end
                        && w.outer.end > range.start
                })
                .collect();
            if !affected.is_empty() {
                let start = affected
                    .iter()
                    .fold(range.start, |at, w| at.min(w.outer.start));
                let end = affected.iter().fold(range.end, |at, w| at.max(w.outer.end));
                // Keep other inline constructs intact; never cross their delimiters.
                if self.wrappers.iter().any(|w| {
                    !matches!(w.marker.as_str(), "**" | "__")
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
}

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

fn list_prefix(line: &str) -> Option<(usize, String)> {
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

/// A single contiguous change, aligned to UTF-8 boundaries.
pub fn changed_range(old: &str, new: &str) -> (Range<usize>, Range<usize>) {
    let mut prefix = old
        .bytes()
        .zip(new.bytes())
        .take_while(|(a, b)| a == b)
        .count();
    while !old.is_char_boundary(prefix) || !new.is_char_boundary(prefix) {
        prefix -= 1;
    }
    let mut suffix = old[prefix..]
        .bytes()
        .rev()
        .zip(new[prefix..].bytes().rev())
        .take_while(|(a, b)| a == b)
        .count();
    while !old.is_char_boundary(old.len() - suffix) || !new.is_char_boundary(new.len() - suffix) {
        suffix -= 1;
    }
    (prefix..old.len() - suffix, prefix..new.len() - suffix)
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
    fn empty_heading_input_stays_after_prefix_on_any_line() {
        for level in 1..=6 {
            for newline in ["\n", "\r\n"] {
                for before in [String::new(), format!("# 見出し{newline}")] {
                    for after in [String::new(), format!("{newline}本文")] {
                        let prefix = format!("{} ", "#".repeat(level));
                        let source = format!("{before}{prefix}{after}");
                        let p = Projection::new(&source);
                        let anchor = before.len() + prefix.len();
                        let display = p.display_position(anchor);
                        assert_eq!(p.source_position(display), anchor, "{source:?}");
                        assert_eq!(
                            p.replace(display..display, "見出し2"),
                            (
                                format!("{before}{prefix}見出し2{after}"),
                                anchor + "見出し2".len()
                            ),
                        );
                    }
                }
            }
        }
    }
    #[test]
    fn code_content_edits_do_not_modify_fence_language() {
        let p = Projection::new("```rust\nlet x = 1;\n```\n");
        assert!(p.text.starts_with("let x"));
        assert_eq!(p.replace(0..0, "// ").0, "```rust\n// let x = 1;\n```\n");
        let empty = "```\n\n```\n";
        assert_eq!(Projection::new(empty).text, empty);
    }
    #[test]
    fn entity_projection_keeps_original_encoding_on_unrelated_edit() {
        let p = Projection::new("A &amp; B");
        assert_eq!(p.text, "A & B");
        assert_eq!(p.replace(0..1, "X").0, "X &amp; B");
    }
    #[test]
    fn partial_bold_toggle_splits_existing_style_without_broken_delimiters() {
        let p = Projection::new("**hello world**");
        assert_eq!(p.format(6..11, "**").unwrap().0, "**hello** world");
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
                    || Projection::new(&typed).text.contains("\n• あ"),
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
    #[test]
    fn nested_quotes_and_closed_fences_keep_new_input_outside_markers() {
        let p = Projection::new("> > text\n> > ");
        assert_eq!(
            p.replace(p.text.len()..p.text.len(), "次").0,
            "> > text\n> > 次"
        );
        let p = Projection::new("```rust\na\n```\n\ntext");
        let at = p.text.find('\n').unwrap() + 1;
        assert_eq!(p.replace(at..at, "次").0, "```rust\na\n```\n次\ntext");
    }
    #[test]
    fn formatting_round_trips_and_unicode_diff_is_valid() {
        let p = Projection::new("日本語 text");
        let (source, _) = p.format(0..9, "**").unwrap();
        assert_eq!(source, "**日本語** text");
        let p = Projection::new(&source);
        assert_eq!(p.format(0..9, "**").unwrap().0, "日本語 text");
        let (old, new) = changed_range("あいう", "あえう");
        assert_eq!(&"あいう"[old], "い");
        assert_eq!(&"あえう"[new], "え");
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
    fn metadata_is_not_a_heading_and_reference_labels_remain_readable() {
        let source = "---\ntitle: draft\n---\n\n# Title\n\n[page][p]\n\n[p]: other.md\n";
        let structure = crate::structure::Structure::parse(source);
        assert_eq!(structure.headings.len(), 1);
        assert_eq!(structure.links[0].destination, "other.md");
        assert!(Projection::editable(source, None).text.contains("page"));
        assert_eq!(Projection::raw(source).text, source);
        assert_eq!(
            crate::decode_fragment("%E8%A6%8B%E5%87%BA%E3%81%97"),
            "見出し"
        );
    }
}
