//! A small Mermaid lexer for code-block colors. Fences stay uncolored.
use iced::Color;
use norito_model::syntax::is_fence;

/// Per-byte colors for a fenced Mermaid block: keywords, strings, numbers, operators, comments.
pub(crate) fn colors(source: &str) -> Vec<Option<Color>> {
    let keyword = Color::from_rgb8(180, 142, 173);
    let string = Color::from_rgb8(163, 190, 140);
    let operator = Color::from_rgb8(136, 192, 208);
    let number = Color::from_rgb8(208, 135, 112);
    let comment = Color::from_rgb8(101, 115, 126);
    let mut colors = vec![None; source.len()];
    let mut offset = 0;
    for line in source.split_inclusive('\n') {
        if is_fence(line) {
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

#[cfg(test)]
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
