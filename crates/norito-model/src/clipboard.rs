use crate::structure::{Diagram, Structure};
use pulldown_cmark::{Event, Parser, Tag, TagEnd, html};
use std::collections::HashMap;

pub struct EmbeddedFigure {
    pub url: String,
    pub extent: Option<crate::structure::Extent>,
}

pub fn html_fragment(markdown: &str) -> String {
    html_with_assets(markdown, &HashMap::new(), &HashMap::new())
}

pub fn html_with_assets(
    markdown: &str,
    images: &HashMap<String, String>,
    diagrams: &HashMap<Diagram, EmbeddedFigure>,
) -> String {
    let mut html = String::new();
    let mut in_code = false;
    let mut skip_code = false;
    let structure = Structure::parse(markdown);
    html::push_html(
        &mut html,
        Parser::new_ext(markdown, crate::syntax::options())
            .into_offset_iter()
            .filter_map(|(event, range)| {
                if skip_code {
                    if matches!(event, Event::End(TagEnd::CodeBlock)) {
                        skip_code = false;
                    }
                    return None;
                }
                if matches!(event, Event::Start(Tag::CodeBlock(_)))
                    && let Some(block) = structure
                        .code
                        .iter()
                        .find(|c| c.range.start == range.start && c.is_mermaid())
                    && let Some(figure) = diagrams.get(&Diagram::Mermaid(block.content.clone()))
                {
                    skip_code = true;
                    let url = &figure.url;
                    return Some(Event::Html(
                        format!(
                            "<p><img alt=\"Mermaid\" style=\"max-width:100%\" src=\"{url}\"></p>"
                        )
                        .into(),
                    ));
                }
                if let Event::InlineMath(value) | Event::DisplayMath(value) = &event {
                    let diagram = if matches!(event, Event::DisplayMath(_)) {
                        Diagram::DisplayMath(value.to_string())
                    } else {
                        Diagram::InlineMath(value.to_string())
                    };
                    if let Some(figure) = diagrams.get(&diagram) {
                        let url = &figure.url;
                        let size = figure.extent.map_or(String::new(), |extent| {
                            format!(
                                "width:{}px;height:{}px;vertical-align:-{}px",
                                extent.width * 16.5,
                                (extent.height + extent.depth) * 16.5,
                                extent.depth * 16.5
                            )
                        });
                        let block = if matches!(diagram, Diagram::DisplayMath(_)) {
                            "display:block;"
                        } else {
                            ""
                        };
                        return Some(Event::InlineHtml(
                            format!("<img alt=\"formula\" style=\"{block}{size}\" src=\"{url}\">")
                                .into(),
                        ));
                    }
                }
                Some(match event {
                    Event::Start(Tag::Image {
                        link_type,
                        dest_url,
                        title,
                        id,
                    }) => {
                        let destination = images
                            .get(dest_url.as_ref())
                            .cloned()
                            .map(Into::into)
                            .unwrap_or(dest_url);
                        Event::Start(Tag::Image {
                            link_type,
                            dest_url: destination,
                            title,
                            id,
                        })
                    }
                    Event::Start(Tag::CodeBlock(_)) => {
                        in_code = true;
                        event
                    }
                    Event::End(TagEnd::CodeBlock) => {
                        in_code = false;
                        event
                    }
                    Event::Text(ref text)
                        if !in_code
                            && markdown[..range.start]
                                .bytes()
                                .rev()
                                .take_while(|&b| b == b'\\')
                                .count()
                                % 2
                                == 0
                            && markdown[range] == **text =>
                    {
                        match crate::projection::Projection::extension_html(text) {
                            Some(html) => Event::InlineHtml(html.into()),
                            None => event,
                        }
                    }
                    Event::Html(text) | Event::InlineHtml(text) => Event::Text(text),
                    other => other,
                })
            }),
    );
    let html = html.replace("<pre>", "<pre style=\"background:#211e1b;color:#d2d8e1;padding:12px 16px;white-space:pre-wrap\">")
        .replace("<blockquote>", "<blockquote style=\"border-left:3px solid #a89b88;padding-left:16px\">")
        .replace("<table>", "<table style=\"border-collapse:collapse\">")
        .replace("<td>", "<td style=\"border:1px solid #ccc;padding:6px 10px\">")
        .replace("<th>", "<th style=\"border:1px solid #ccc;padding:6px 10px\">");
    format!("<div style=\"font-family:sans-serif;font-size:15px;line-height:1.8\">{html}</div>")
}

#[test]
fn html_keeps_semantics_and_does_not_execute_raw_markup() {
    let html = html_fragment(
        "# Title\n\n**bold** and *italic* [link](page.md)\n\n- [x] done\n\n<script>alert(1)</script>",
    );
    for tag in [
        "<h1>",
        "<strong>",
        "<em>",
        "href=\"page.md\"",
        "type=\"checkbox\"",
    ] {
        assert!(html.contains(tag), "{html}");
    }
    assert!(!html.contains("<script>"));
}

#[test]
fn html_component_semantics() {
    for (source, fragments) in [
        ("# 題", vec!["<h1>題</h1>"]),
        ("**太字 *斜体***", vec!["<strong>", "<em>斜体</em>"]),
        ("~~削除~~", vec!["<del>削除</del>"]),
        (
            "==強調== x^2^ H~2~O",
            vec!["<mark>強調</mark>", "<sup>2</sup>", "<sub>2</sub>"],
        ),
        ("> 引用", vec!["<blockquote style=", "引用"]),
        ("- 項目", vec!["<ul>", "<li>項目</li>"]),
        ("1. 項目", vec!["<ol>", "<li>項目</li>"]),
        ("- [x] 完了", vec!["type=\"checkbox\"", "checked=\"\""]),
        (
            "![説明](image.png)",
            vec!["<img src=\"image.png\" alt=\"説明\""],
        ),
        ("`<b>`", vec!["<code>&lt;b&gt;</code>"]),
        (
            "```rust\nfn main() {}\n```",
            vec!["<pre style=", "class=\"language-rust\"", "fn main() {}"],
        ),
        (
            "```mermaid\nA-->B\n```",
            vec!["class=\"language-mermaid\"", "A--&gt;B"],
        ),
        (
            "| A | B |\n|---|---|\n| **太字** | *斜体* |",
            vec!["<table style=", "<strong>太字</strong>", "<em>斜体</em>"],
        ),
        ("---", vec!["<hr />"]),
        ("一  \n二", vec!["<br />"]),
        ("$x^2$", vec!["x^2"]),
        ("本文[^1]\n\n[^1]: 注釈", vec!["footnote-reference", "注釈"]),
    ] {
        let html = html_fragment(source);
        for fragment in fragments {
            assert!(
                html.contains(fragment),
                "{source}: missing {fragment} in {html}"
            );
        }
    }
}

#[test]
fn extensions_do_not_reinterpret_code_or_escaped_markers() {
    for source in [
        "`==literal==`",
        "```text\n==literal==\n```",
        "\\^literal^",
        "\\~literal~",
    ] {
        let html = html_fragment(source);
        for tag in ["<mark>", "<sup>", "<sub>"] {
            assert!(!html.contains(tag), "{source}: {html}");
        }
    }
}
