//! Inert clipboard HTML -> Markdown. No scripts, stylesheets or URLs are executed.
use html5ever::tokenizer::{BufferQueue, TagKind, Token, TokenSink, TokenSinkResult, Tokenizer};
use std::cell::RefCell;

#[derive(Default)]
struct Node {
    tag: String,
    attrs: Vec<(String, String)>,
    children: Vec<Node>,
    text: String,
}
impl Node {
    fn attr(&self, name: &str) -> &str {
        self.attrs
            .iter()
            .find(|(key, _)| key == name)
            .map_or("", |(_, value)| value)
    }
}
struct Sink(RefCell<Vec<Node>>);
impl TokenSink for Sink {
    type Handle = ();
    fn process_token(&self, token: Token, _: u64) -> TokenSinkResult<()> {
        let mut stack = self.0.borrow_mut();
        match token {
            Token::CharacterTokens(text) => stack.last_mut().unwrap().children.push(Node {
                text: text.to_string(),
                ..Node::default()
            }),
            Token::TagToken(tag) if tag.kind == TagKind::StartTag => {
                let name = tag.name.to_string();
                // Common omitted end tags in clipboard fragments.
                if matches!(name.as_str(), "li" | "td" | "th" | "tr" | "p")
                    && stack.last().is_some_and(|n| n.tag == name)
                {
                    close(&mut stack);
                }
                let node = Node {
                    tag: name.clone(),
                    attrs: tag
                        .attrs
                        .iter()
                        .map(|a| (a.name.local.to_string(), a.value.to_string()))
                        .collect(),
                    ..Node::default()
                };
                if tag.self_closing
                    || matches!(
                        name.as_str(),
                        "br" | "hr" | "img" | "input" | "meta" | "link" | "wbr"
                    )
                {
                    stack.last_mut().unwrap().children.push(node);
                } else if stack.len() < 128 {
                    stack.push(node);
                }
            }
            Token::TagToken(tag) => {
                if let Some(index) = stack.iter().rposition(|n| n.tag == tag.name.as_ref()) {
                    while stack.len() > index && stack.len() > 1 {
                        close(&mut stack);
                    }
                }
            }
            _ => {}
        }
        TokenSinkResult::Continue
    }
}
fn close(stack: &mut Vec<Node>) {
    let node = stack.pop().unwrap();
    stack.last_mut().unwrap().children.push(node);
}

pub fn markdown(html: &str) -> String {
    let input = BufferQueue::default();
    input.push_back(html.into());
    let tokenizer = Tokenizer::new(
        Sink(RefCell::new(vec![Node::default()])),
        Default::default(),
    );
    let _ = tokenizer.feed(&input);
    tokenizer.end();
    let mut stack = tokenizer.sink.0.into_inner();
    while stack.len() > 1 {
        close(&mut stack);
    }
    render(&stack[0], false).trim().to_owned()
}
fn escape(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        if "\\`*_[]<>~^=#|".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}
fn destination(url: &str) -> String {
    url.replace(' ', "%20")
        .replace('(', "%28")
        .replace(')', "%29")
        .replace('<', "%3C")
        .replace('>', "%3E")
}
fn wrap(marker: &str, value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return value.into();
    }
    let start = value.len() - value.trim_start().len();
    let end = value.trim_end().len();
    format!(
        "{}{marker}{trimmed}{marker}{}",
        &value[..start],
        &value[end..]
    )
}
fn render(node: &Node, code: bool) -> String {
    if matches!(
        node.tag.as_str(),
        "script" | "style" | "head" | "iframe" | "object"
    ) {
        return String::new();
    }
    if node.tag.is_empty() && !node.text.is_empty() {
        return if code {
            node.text.clone()
        } else {
            escape(&node.text.split_whitespace().collect::<Vec<_>>().join(" "))
                .pipe_spaces(&node.text)
        };
    }
    let literal = code || node.tag == "pre" || node.tag == "code";
    let mut inner = String::new();
    for child in &node.children {
        let part = render(child, literal);
        if !literal && inner.ends_with('\n') && part.trim().is_empty() {
            continue;
        }
        if !literal && part.starts_with("\n\n") && inner.trim_end_matches(' ').ends_with('\n') {
            inner.truncate(inner.trim_end().len());
        }
        inner.push_str(&part);
    }
    match node.tag.as_str() {
        "b" | "strong" => wrap("**", &inner),
        "i" | "em" => wrap("*", &inner),
        "s" | "del" | "strike" => wrap("~~", &inner),
        "mark" => wrap("==", &inner),
        "sup" => wrap("^", &inner),
        "sub" => wrap("~", &inner),
        "span" => {
            let style = node.attr("style").to_lowercase().replace(' ', "");
            let mut value = inner;
            if style.contains("font-style:italic") {
                value = wrap("*", &value);
            }
            if style.contains("font-weight:bold")
                || style.contains("font-weight:700")
                || style.contains("font-weight:600")
            {
                value = wrap("**", &value);
            }
            if style.contains("line-through") {
                value = wrap("~~", &value);
            }
            value
        }
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => format!(
            "\n\n{} {}\n\n",
            "#".repeat(node.tag.as_bytes()[1] as usize - b'0' as usize),
            inner.trim()
        ),
        "p" | "div" | "section" | "article" => format!("\n\n{}\n\n", inner.trim()),
        "br" => {
            if code {
                "\n".into()
            } else {
                "  \n".into()
            }
        }
        "hr" => "\n\n---\n\n".into(),
        "blockquote" => format!(
            "\n\n{}\n\n",
            inner
                .trim()
                .lines()
                .map(|l| format!("> {l}"))
                .collect::<Vec<_>>()
                .join("\n")
        ),
        "code" if !code => {
            let fence = "`".repeat(inner.split(|c| c != '`').map(str::len).max().unwrap_or(0) + 1);
            let pad = if inner.starts_with('`') || inner.ends_with('`') {
                " "
            } else {
                ""
            };
            format!("{fence}{pad}{inner}{pad}{fence}")
        }
        "pre" => {
            let language = node
                .children
                .iter()
                .find(|c| c.tag == "code")
                .map_or("", |c| c.attr("class"))
                .split_whitespace()
                .find_map(|c| c.strip_prefix("language-"))
                .unwrap_or("");
            let fence =
                "`".repeat((inner.split(|c| c != '`').map(str::len).max().unwrap_or(0) + 1).max(3));
            format!(
                "\n\n{fence}{language}\n{}\n{fence}\n\n",
                inner.trim_end_matches('\n')
            )
        }
        "a" if !node.attr("href").is_empty() => {
            format!("[{inner}]({})", destination(node.attr("href")))
        }
        "img" => format!(
            "![{}]({})",
            escape(node.attr("alt")),
            destination(node.attr("src"))
        ),
        "input" if node.attr("type") == "checkbox" => {
            if node.attrs.iter().any(|(k, _)| k == "checked") {
                "[x] ".into()
            } else {
                "[ ] ".into()
            }
        }
        "ul" | "ol" => {
            let start = node.attr("start").parse::<usize>().unwrap_or(1);
            let lines = node
                .children
                .iter()
                .filter(|c| c.tag == "li")
                .enumerate()
                .map(|(i, c)| {
                    let content = render(c, false);
                    let marker = if node.tag == "ol" {
                        format!("{}. ", start + i)
                    } else {
                        "- ".into()
                    };
                    let mut lines = content.trim().lines();
                    let first = lines.next().unwrap_or("");
                    format!(
                        "{marker}{first}{}",
                        lines
                            .map(|l| format!("\n{}{l}", " ".repeat(marker.len())))
                            .collect::<String>()
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            format!("\n\n{lines}\n\n")
        }
        "table" => {
            let mut rows = Vec::new();
            collect_rows(node, &mut rows);
            let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
            let mut result = String::from("\n\n");
            for (i, mut row) in rows.into_iter().enumerate() {
                row.resize(columns, String::new());
                result.push_str(&format!("| {} |\n", row.join(" | ")));
                if i == 0 {
                    result.push_str(&format!("| {} |\n", vec!["---"; columns].join(" | ")));
                }
            }
            result.push('\n');
            result
        }
        _ => inner,
    }
}
fn collect_rows(node: &Node, rows: &mut Vec<Vec<String>>) {
    if node.tag == "tr" {
        rows.push(
            node.children
                .iter()
                .filter(|c| matches!(c.tag.as_str(), "td" | "th"))
                .map(|c| render(c, false).trim().replace('\n', "<br>"))
                .collect(),
        );
    } else {
        for child in &node.children {
            collect_rows(child, rows);
        }
    }
}
trait Spaces {
    fn pipe_spaces(self, original: &str) -> String;
}
impl Spaces for String {
    fn pipe_spaces(mut self, original: &str) -> String {
        if original.starts_with(char::is_whitespace) {
            self.insert(0, ' ');
        }
        if !self.trim().is_empty() && original.ends_with(char::is_whitespace) {
            self.push(' ');
        }
        self
    }
}

#[test]
fn external_html_keeps_components_and_never_executes_markup() {
    for (html, expected) in [
        (
            "<p><strong>太字 <em>斜体</em></strong></p>",
            "**太字 *斜体***",
        ),
        (
            "<h2>題</h2><blockquote><p>引用</p></blockquote>",
            "## 題\n\n> 引用",
        ),
        (
            "<ul><li><input type=checkbox checked>完了</li><li>次</li></ul>",
            "- [x] 完了\n- 次",
        ),
        (
            "<pre><code class=language-rust>fn main() {\n  x();\n}</code></pre>",
            "```rust\nfn main() {\n  x();\n}\n```",
        ),
        (
            "<table><tr><th>A</th></tr><tr><td><b>B</b></td></tr></table>",
            "| A |\n| --- |\n| **B** |",
        ),
        (
            "<span style='font-weight:700;font-style:italic'>文字</span>",
            "***文字***",
        ),
        (
            "<script>alert(1)</script><p>&lt;tag&gt; &amp; 日本語 👩‍💻</p>",
            "\\<tag\\> & 日本語 👩‍💻",
        ),
        (
            "<p><a href='page.md'>link</a><img src='a.png' alt='画像'></p>",
            "[link](page.md)![画像](a.png)",
        ),
        (
            "<p><mark>強調</mark><sup>2</sup><sub>n</sub><del>削除</del><code>*literal*</code></p>",
            "==強調==^2^~n~~~削除~~`*literal*`",
        ),
    ] {
        assert_eq!(markdown(html), expected, "{html}");
    }
}

#[test]
fn html_boundaries_preserve_code_whitespace_unicode_and_escaped_syntax() {
    assert_eq!(markdown("<p>一</p>\n<p>二</p>"), "一\n\n二");
    assert_eq!(
        markdown("<pre><code>a\n\n\n\nb</code></pre>"),
        "```\na\n\n\n\nb\n```"
    );
    assert_eq!(
        markdown("<p>*literal* &amp; &#x1f600; e&#769;</p>"),
        "\\*literal\\* & 😀 e\u{301}"
    );
    assert_eq!(markdown("<ul><li>one<li>two</ul>"), "- one\n- two");
    assert_eq!(
        markdown("<style>body{}</style><iframe>hidden</iframe><p>visible</p>"),
        "visible"
    );
    assert_eq!(markdown(""), "");
}
