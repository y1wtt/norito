use super::*;

impl Projection {
    /// HTML for the optional inline extensions, after the standard parser has
    /// separated code, links and other Markdown constructs.
    pub(crate) fn extension_html(source: &str) -> Option<String> {
        let projection = Self::new(source);
        let mut substitutions = Vec::new();
        for wrapper in &projection.wrappers {
            let (open, close) = match wrapper.marker.as_str() {
                "==" => ("<mark>", "</mark>"),
                "^" => ("<sup>", "</sup>"),
                "~" => ("<sub>", "</sub>"),
                _ => continue,
            };
            substitutions.push((wrapper.outer.start..wrapper.inner.start, open));
            substitutions.push((wrapper.inner.end..wrapper.outer.end, close));
        }
        if substitutions.is_empty() {
            return None;
        }
        substitutions.sort_by_key(|(range, _)| range.start);
        let mut html = String::new();
        let mut at = 0;
        for (range, tag) in substitutions {
            if range.start < at {
                continue;
            }
            html.push_str(&escape_html(&source[at..range.start]));
            html.push_str(tag);
            at = range.end;
        }
        html.push_str(&escape_html(&source[at..]));
        Some(html)
    }
    /// A standalone Markdown fragment, including delimiters crossing the selection.
    pub fn copy_markdown(&self, display: Range<usize>) -> String {
        if display.is_empty() {
            return String::new();
        }
        // A whole-document copy includes invisible block delimiters and definitions.
        if display.start == 0 && display.end == self.text.len() {
            return self.source.clone();
        }
        let mut range = self.source_range(display.clone());
        for (event, block) in
            pulldown_cmark::Parser::new_ext(&self.source, crate::syntax::options())
                .into_offset_iter()
        {
            if !matches!(
                event,
                pulldown_cmark::Event::Start(pulldown_cmark::Tag::Heading { .. })
            ) {
                continue;
            }
            let end = block.start
                + self.source[block.clone()]
                    .trim_end_matches(['\r', '\n'])
                    .len();
            let visible: Vec<_> = self
                .cells
                .iter()
                .filter(|c| block.start <= c.source.start && c.source.end <= end)
                .collect();
            if !visible.is_empty()
                && visible
                    .iter()
                    .all(|c| display.start <= c.display.start && c.display.end <= display.end)
            {
                range.start = range.start.min(block.start);
                range.end = range.end.max(end);
            }
        }
        for code in crate::structure::Structure::parse(&self.source).code {
            let cells: Vec<_> = self
                .cells
                .iter()
                .filter(|c| code.range.contains(&c.source.start))
                .collect();
            if let (Some(first), Some(last)) = (cells.first(), cells.last())
                && display.start <= first.display.start
                && last.display.end <= display.end
            {
                range.start = range.start.min(code.range.start);
                range.end = range.end.max(code.range.end);
            }
        }
        if display.start == 0 || self.text[..display.start].ends_with('\n') {
            while let Some(prefix) = self
                .prefixes
                .iter()
                .find(|prefix| prefix.end == range.start)
            {
                range.start = prefix.start;
            }
        }
        for wrapper in self.wrappers.iter().rev() {
            if range.start <= wrapper.inner.start && wrapper.inner.end <= range.end {
                range.start = range.start.min(wrapper.outer.start);
                range.end = range.end.max(wrapper.outer.end);
            }
        }
        let mut open = Vec::new();
        let mut close = Vec::new();
        for w in &self.wrappers {
            if w.inner.start <= range.start && range.start < w.inner.end {
                open.push((w.outer.start, &self.source[w.outer.start..w.inner.start]));
            }
            if w.inner.start < range.end && range.end <= w.inner.end {
                close.push((w.outer.end, &self.source[w.inner.end..w.outer.end]));
            }
        }
        open.sort_by_key(|(at, _)| *at);
        close.sort_by_key(|(at, _)| *at);
        format!(
            "{}{}{}",
            open.into_iter().map(|(_, s)| s).collect::<String>(),
            &self.source[range],
            close.into_iter().map(|(_, s)| s).collect::<String>()
        )
    }
}

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[test]
fn copied_partial_bold_keeps_its_formatting() {
    let p = Projection::new("before **日本語** after");
    let at = p.text.find("本").unwrap();
    assert_eq!(p.copy_markdown(at..at + "本".len()), "**本**");
    assert_eq!(p.copy_markdown(0..p.text.len()), p.source);
    let p = Projection::new("[label](page.md)");
    assert_eq!(p.copy_markdown(1..4), "[abe](page.md)");
}

#[test]
fn copying_components_exports_source_not_placeholders() {
    for source in [
        "# Heading",
        "- item\n- [x] done",
        "```rust\nlet x = 1;\n```",
        "```mermaid\nflowchart LR; A-->B\n```",
        "| A | B |\n|---|---|\n| 1 | 2 |",
    ] {
        let p = Projection::editable(source, None);
        assert_eq!(p.copy_markdown(0..p.text.len()), source, "{source}");
    }
}

#[cfg(test)]
mod coverage {
    use super::*;
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/components.rs"
    ));

    #[test]
    fn every_component_copies_losslessly_in_rich_and_source_modes() {
        for &(name, source) in COMPONENTS {
            for p in [Projection::editable(source, None), Projection::raw(source)] {
                assert_eq!(
                    p.copy_markdown(0..p.text.len()),
                    source,
                    "{name}: {:?}",
                    p.text
                );
                assert_eq!(p.copy_markdown(0..0), "", "{name}");
            }
        }
    }

    #[test]
    fn partial_nested_and_multiline_selection_preserves_delimiters() {
        for (source, selected, expected) in [
            ("***日本語***", "本", "***本***"),
            ("**a *日本語* z**", "本", "***本***"),
            ("**a** and *b*", "a and b", "**a** and *b*"),
            ("**一\n二**", "一\n二", "**一\n二**"),
            ("**一**\n次", "一\n", "**一**\n"),
            ("[**日本語**](page.md)", "本", "[**本**](page.md)"),
            ("~~日本語~~", "本", "~~本~~"),
            ("==日本語==", "本", "==本=="),
            ("前 `日本語` 後", "本", "`本`"),
        ] {
            let p = Projection::editable(source, None);
            let start = p.text.find(selected).unwrap();
            assert_eq!(
                p.copy_markdown(start..start + selected.len()),
                expected,
                "{source}"
            );
        }
    }

    #[test]
    fn heading_copy_in_middle_of_document_preserves_both_heading_syntaxes() {
        for heading in ["見出し\n======", "## 見出し ##"] {
            let source = format!("前\n\n{heading}\n\n後");
            let p = Projection::editable(&source, None);
            let start = p.text.find("見出し").unwrap();
            assert_eq!(p.copy_markdown(start..start + "見出し".len()), heading);
        }
    }

    #[test]
    fn every_component_accepts_every_unicode_selection_boundary() {
        for &(name, source) in COMPONENTS {
            let p = Projection::editable(source, None);
            let boundaries: Vec<_> = p
                .text
                .char_indices()
                .map(|(i, _)| i)
                .chain([p.text.len()])
                .collect();
            for &start in &boundaries {
                for &end in boundaries.iter().filter(|&&end| end >= start) {
                    let result = p.copy_markdown(start..end);
                    assert_eq!(result.is_empty(), start == end, "{name} {start}..{end}");
                }
            }
        }
    }
}
