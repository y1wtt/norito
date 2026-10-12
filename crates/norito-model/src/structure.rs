//! Navigation-level facts extracted from a source. All ranges refer to the original Markdown bytes.
use pulldown_cmark::{Alignment, CodeBlockKind, Event, Parser, Tag, TagEnd};
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
/// A block rendered as an image. Equal sources share one rendering.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Diagram {
    Mermaid(String),
    Math(String),
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

impl CodeBlock {
    pub fn is_mermaid(&self) -> bool {
        crate::syntax::is_mermaid(&self.language)
    }
}
impl Math {
    pub fn diagram(&self) -> Diagram {
        Diagram::Math(self.content.clone())
    }
}

impl Structure {
    /// Every diagram and formula, in document order: the set worth rendering.
    pub fn diagrams(&self) -> impl Iterator<Item = Diagram> + '_ {
        self.code
            .iter()
            .filter(|code| code.is_mermaid())
            .map(|code| Diagram::Mermaid(code.content.clone()))
            .chain(self.math.iter().map(Math::diagram))
    }
    pub fn heading(&self, anchor: &str) -> Option<&Heading> {
        self.headings.iter().find(|h| h.anchor == anchor)
    }
    pub fn footnote(&self, label: &str) -> Option<usize> {
        self.footnotes
            .iter()
            .find_map(|(name, offset)| (name == label).then_some(*offset))
    }
    /// The non-image link whose source contains `at`, delimiters included.
    pub fn link_at(&self, at: usize) -> Option<&Link> {
        self.links
            .iter()
            .find(|link| !link.image && link.range.start <= at && at <= link.range.end)
    }
    pub fn parse(source: &str) -> Self {
        let mut result = Self::default();
        let mut heading: Option<Heading> = None;
        let mut link: Option<Link> = None;
        let mut code: Option<CodeBlock> = None;
        let mut table: Option<Table> = None;
        let mut cell: Option<String> = None;
        let mut row = Vec::new();
        for (event, range) in Parser::new_ext(source, crate::syntax::options()).into_offset_iter() {
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
                        h.anchor = anchor(&h.title, &result.headings);
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
                                Alignment::Center => 1,
                                Alignment::Right => 2,
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

/// GitHub-style slug, made unique against the headings already seen.
fn anchor(title: &str, seen: &[Heading]) -> String {
    let base: String = title
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
    let mut anchor = base.clone();
    let mut suffix = 1;
    while seen.iter().any(|other| other.anchor == anchor) {
        anchor = format!("{base}-{suffix}");
        suffix += 1;
    }
    anchor
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
        assert_eq!(
            s.diagrams().collect::<Vec<_>>(),
            [Diagram::Mermaid("flowchart LR\n A-->B\n".into())]
        );
        assert_eq!(s.heading("日本語-1").unwrap().offset, 13);
    }
    #[test]
    fn metadata_is_not_a_heading_and_reference_labels_resolve() {
        let source = "---\ntitle: draft\n---\n\n# Title\n\n[page][p]\n\n[p]: other.md\n";
        let structure = Structure::parse(source);
        assert_eq!(structure.headings.len(), 1);
        assert_eq!(structure.links[0].destination, "other.md");
    }
}
