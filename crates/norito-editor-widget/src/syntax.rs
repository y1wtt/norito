use iced::Color;
use std::sync::OnceLock;
use syntect::{easy::HighlightLines, highlighting::ThemeSet, parsing::SyntaxSet};

#[test]
fn cached_blocks_keep_colors_after_offsets_shift_and_code_changes() {
    let mut cache = Cache::default();
    cache.colors("Before\n\n```rust\nlet a = 1;\n```\n");
    for source in [
        "More text before\n\n```rust\nlet a = 1;\n```\n",
        "```rust\nlet a = \"changed\";\n```\n",
        "No code left",
    ] {
        assert_eq!(cache.colors(source), colors(source));
    }
}

#[derive(Default)]
pub struct Cache(std::collections::HashMap<String, Vec<Option<Color>>>);

#[cfg(test)]
pub fn colors(source: &str) -> Vec<Option<Color>> {
    Cache::default().colors(source)
}

impl Cache {
    pub fn colors(&mut self, source: &str) -> Vec<Option<Color>> {
        static SYNTAXES: OnceLock<SyntaxSet> = OnceLock::new();
        static THEMES: OnceLock<ThemeSet> = OnceLock::new();
        let syntaxes = SYNTAXES.get_or_init(SyntaxSet::load_defaults_newlines);
        let themes = THEMES.get_or_init(ThemeSet::load_defaults);
        let mut result = vec![None; source.len()];
        let mut previous = std::mem::take(&mut self.0);
        for code in norito_model::structure::Structure::parse(source).code {
            let block = &source[code.range.clone()];
            if let Some(colors) = previous
                .remove(block)
                .or_else(|| self.0.get(block).cloned())
            {
                result[code.range.clone()].copy_from_slice(&colors);
                self.0.insert(block.to_owned(), colors);
                continue;
            }
            let language = code.language.split_whitespace().next().unwrap_or("");
            if language.eq_ignore_ascii_case("mermaid") {
                let colors = mermaid_colors(block);
                result[code.range.clone()].copy_from_slice(&colors);
                self.0.insert(block.to_owned(), colors);
                continue;
            }
            let syntax = syntaxes
                .find_syntax_by_token(language)
                .unwrap_or_else(|| syntaxes.find_syntax_plain_text());
            let mut highlighter = HighlightLines::new(syntax, &themes.themes["base16-ocean.dark"]);
            let mut at = code.range.start;
            for line in source[code.range].split_inclusive('\n') {
                if line.trim_start().starts_with("```") || line.trim_start().starts_with("~~~") {
                    at += line.len();
                    continue;
                }
                if let Ok(spans) = highlighter.highlight_line(line, syntaxes) {
                    let mut offset = at;
                    for (style, text) in spans {
                        let c = style.foreground;
                        result[offset..offset + text.len()]
                            .fill(Some(Color::from_rgb8(c.r, c.g, c.b)));
                        offset += text.len();
                    }
                }
                at += line.len();
            }
            self.0
                .insert(block.to_owned(), result[at - block.len()..at].to_vec());
        }
        result
    }
}

fn mermaid_colors(source: &str) -> Vec<Option<Color>> {
    let keyword = Color::from_rgb8(180, 142, 173);
    let string = Color::from_rgb8(163, 190, 140);
    let operator = Color::from_rgb8(136, 192, 208);
    let number = Color::from_rgb8(208, 135, 112);
    let comment = Color::from_rgb8(101, 115, 126);
    let mut colors = vec![None; source.len()];
    let mut offset = 0;
    for line in source.split_inclusive('\n') {
        if line.trim_start().starts_with("```") || line.trim_start().starts_with("~~~") {
            offset += line.len();
            continue;
        }
        let mut at = 0;
        while at < line.len() {
            let start = at;
            let ch = line[at..].chars().next().unwrap();
            let color = if line[at..].starts_with("%%") {
                at = line.len();
                Some(comment)
            } else if ch == '"' || ch == '\'' || ch == '`' {
                at += ch.len_utf8();
                let mut escaped = false;
                for next in line[at..].chars() {
                    at += next.len_utf8();
                    if next == ch && !escaped {
                        break;
                    }
                    escaped = next == '\\' && !escaped;
                }
                Some(string)
            } else if ch.is_ascii_alphabetic() || ch == '_' {
                at += line[at..]
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .map(char::len_utf8)
                    .sum::<usize>();
                if line[at..].starts_with("-v2") {
                    at += 3;
                }
                let word = &line[start..at];
                matches!(
                    word,
                    "graph"
                        | "flowchart"
                        | "sequenceDiagram"
                        | "classDiagram"
                        | "stateDiagram"
                        | "stateDiagram-v2"
                        | "erDiagram"
                        | "gantt"
                        | "pie"
                        | "journey"
                        | "gitGraph"
                        | "mindmap"
                        | "timeline"
                        | "subgraph"
                        | "end"
                        | "direction"
                        | "participant"
                        | "actor"
                        | "activate"
                        | "deactivate"
                        | "alt"
                        | "else"
                        | "opt"
                        | "loop"
                        | "par"
                        | "and"
                        | "rect"
                        | "critical"
                        | "break"
                        | "Note"
                        | "note"
                        | "over"
                        | "left"
                        | "right"
                        | "of"
                        | "as"
                        | "autonumber"
                        | "class"
                        | "classDef"
                        | "style"
                        | "linkStyle"
                        | "click"
                        | "title"
                        | "section"
                        | "dateFormat"
                        | "axisFormat"
                        | "state"
                        | "LR"
                        | "RL"
                        | "TB"
                        | "TD"
                        | "BT"
                )
                .then_some(keyword)
            } else if ch.is_ascii_digit() || ch == '#' {
                at += ch.len_utf8();
                at += line[at..]
                    .chars()
                    .take_while(|c| c.is_ascii_hexdigit() || *c == '.')
                    .map(char::len_utf8)
                    .sum::<usize>();
                Some(number)
            } else {
                at += ch.len_utf8();
                "-<>=~.:;|[](){}+*/&".contains(ch).then_some(operator)
            };
            colors[offset + start..offset + at].fill(color);
        }
        offset += line.len();
    }
    colors
}

#[test]
fn mermaid_keywords_arrows_strings_and_comments_are_colored() {
    let source = "```mermaid\nflowchart LR\n A[\"日本語\"] --> B\n %% comment\n```";
    let colors = colors(source);
    let at = |token: &str| colors[source.find(token).unwrap()].unwrap();
    assert_ne!(at("flowchart"), at("-->"));
    assert_ne!(at("日本語"), at("flowchart"));
    assert_ne!(at("%%"), at("-->"));
    assert!(colors[..10].iter().all(Option::is_none));
}
