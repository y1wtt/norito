//! Structural editing helpers. All ranges refer to the original Markdown bytes.
use pulldown_cmark::{CodeBlockKind, Event, Parser, Tag, TagEnd};
use std::ops::Range;

#[derive(Debug, Clone)]
pub struct Heading {
    pub level: u8,
    pub title: String,
    pub offset: usize,
    pub anchor: String,
}
#[derive(Debug, Clone)]
pub struct Link {
    pub label: String,
    pub destination: String,
    pub range: Range<usize>,
    pub image: bool,
}
#[derive(Debug, Clone)]
pub struct CodeBlock {
    pub language: String,
    pub content: String,
    pub range: Range<usize>,
}
#[derive(Debug, Clone)]
pub struct Math {
    pub content: String,
    pub range: Range<usize>,
    pub display: bool,
}
#[derive(Debug, Clone)]
pub struct Table {
    pub cells: Vec<Vec<String>>,
    pub alignment: Vec<u8>,
    pub range: Range<usize>,
}
#[derive(Debug, Clone, Default)]
pub struct Structure {
    pub footnotes: Vec<(String, usize)>,
    pub math: Vec<Math>,
    pub headings: Vec<Heading>,
    pub links: Vec<Link>,
    pub code: Vec<CodeBlock>,
    pub tables: Vec<Table>,
}

impl Structure {
    pub fn parse(source: &str) -> Self {
        let mut result = Self::default();
        let mut heading = None;
        let mut link: Option<Link> = None;
        let mut code: Option<CodeBlock> = None;
        let mut table: Option<Table> = None;
        let mut cell: Option<String> = None;
        let mut row = Vec::new();
        for (event, range) in Parser::new_ext(source, crate::markdown_options()).into_offset_iter()
        {
            match event {
                Event::Start(Tag::FootnoteDefinition(label)) => {
                    result.footnotes.push((label.into_string(), range.start))
                }
                Event::FootnoteReference(label) => result.links.push(Link {
                    label: label.to_string(),
                    destination: format!("#fn:{label}"),
                    range,
                    image: false,
                }),
                Event::InlineMath(value) => result.math.push(Math {
                    content: value.into_string(),
                    range,
                    display: false,
                }),
                Event::DisplayMath(value) => result.math.push(Math {
                    content: value.into_string(),
                    range,
                    display: true,
                }),
                Event::Start(Tag::Heading { level, .. }) => {
                    heading = Some(Heading {
                        level: level as u8,
                        title: String::new(),
                        offset: range.start,
                        anchor: String::new(),
                    })
                }
                Event::End(TagEnd::Heading(_)) => {
                    if let Some(mut h) = heading.take() {
                        let base: String = h
                            .title
                            .to_lowercase()
                            .chars()
                            .filter_map(|c| {
                                if c.is_whitespace() {
                                    Some('-')
                                } else if c.is_alphanumeric() || c == '-' || c == '_' {
                                    Some(c)
                                } else {
                                    None
                                }
                            })
                            .collect();
                        h.anchor = base.clone();
                        let mut suffix = 1;
                        while result.headings.iter().any(|other| other.anchor == h.anchor) {
                            h.anchor = format!("{base}-{suffix}");
                            suffix += 1;
                        }
                        result.headings.push(h);
                    }
                }
                Event::Start(Tag::Link { dest_url, .. })
                | Event::Start(Tag::Image { dest_url, .. }) => {
                    let image = source[range.clone()].starts_with('!');
                    link = Some(Link {
                        label: String::new(),
                        destination: dest_url.into_string(),
                        range,
                        image,
                    });
                }
                Event::End(TagEnd::Link | TagEnd::Image) => {
                    if let Some(link) = link.take() {
                        result.links.push(link);
                    }
                }
                Event::Start(Tag::CodeBlock(kind)) => {
                    code = Some(CodeBlock {
                        language: match kind {
                            CodeBlockKind::Fenced(info) => info.into_string(),
                            _ => String::new(),
                        },
                        content: String::new(),
                        range,
                    })
                }
                Event::End(TagEnd::CodeBlock) => {
                    if let Some(code) = code.take() {
                        result.code.push(code);
                    }
                }
                Event::Start(Tag::Table(alignment)) => {
                    table = Some(Table {
                        alignment: alignment
                            .iter()
                            .map(|a| match a {
                                pulldown_cmark::Alignment::Center => 1,
                                pulldown_cmark::Alignment::Right => 2,
                                _ => 0,
                            })
                            .collect(),
                        cells: Vec::new(),
                        range,
                    })
                }
                Event::Start(Tag::TableHead | Tag::TableRow) => row.clear(),
                Event::Start(Tag::TableCell) => {
                    cell = Some(source[range].trim().replace("\\|", "|"))
                }
                Event::End(TagEnd::TableCell) => row.push(cell.take().unwrap_or_default()),
                Event::End(TagEnd::TableHead | TagEnd::TableRow) => {
                    if let Some(table) = &mut table {
                        table.cells.push(std::mem::take(&mut row));
                    }
                }
                Event::End(TagEnd::Table) => {
                    if let Some(table) = table.take() {
                        result.tables.push(table);
                    }
                }
                Event::Text(text) | Event::Code(text) => {
                    if let Some(h) = &mut heading {
                        h.title.push_str(&text);
                    }
                    if let Some(l) = &mut link {
                        l.label.push_str(&text);
                    }
                    if let Some(c) = &mut code {
                        c.content.push_str(&text);
                    }
                }
                _ => {}
            }
        }
        result
    }
}

pub fn table_markdown(cells: &[Vec<String>]) -> String {
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
                    .cloned()
                    .unwrap_or_default()
                    .replace('|', "\\|")
                    .replace(['\r', '\n'], " "),
            );
            out.push_str(" |");
        }
        out.push('\n');
        if index == 0 {
            out.push_str(&format!("|{}\n", " --- |".repeat(columns)));
        }
    }
    out
}

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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn structure_keeps_ranges_and_duplicate_anchors() {
        let source = "# 日本語\n\n# 日本語\n\n[別ページ](other.md#section)\n\n![画像](assets/a.png)\n\n```mermaid\nflowchart LR\n A-->B\n```\n";
        let s = Structure::parse(source);
        assert_eq!(s.headings[1].anchor, "日本語-1");
        assert_eq!(s.links[0].destination, "other.md#section");
        assert!(s.links[1].image);
        assert_eq!(s.code[0].language, "mermaid");
        assert!(source[s.code[0].range.clone()].starts_with("```"));
    }
    #[test]
    fn helpers_round_trip_pipes_and_nested_fences() {
        let cells = vec![
            vec!["名前".into(), "値".into()],
            vec!["a | b".into(), "x".into()],
        ];
        assert_eq!(
            Structure::parse(&table_markdown(&cells)).tables[0].cells,
            cells
        );
        let text = "```rust\nfn main() {}\n```";
        assert_eq!(
            Structure::parse(&fenced_code("markdown", text)).code[0].content,
            format!("{text}\n")
        );
    }
}
