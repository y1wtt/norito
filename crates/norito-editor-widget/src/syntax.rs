//! Per-byte code-block colors, cached by block text so unchanged blocks are not re-highlighted.
use iced::Color;
use norito_model::{structure::Structure, syntax::is_fence};
use std::{collections::HashMap, sync::OnceLock};
use syntect::{easy::HighlightLines, highlighting::ThemeSet, parsing::SyntaxSet};

#[derive(Default)]
pub(crate) struct Cache(HashMap<String, Vec<Option<Color>>>);

impl Cache {
    /// One entry per source byte; `None` outside code blocks.
    pub(crate) fn colors(&mut self, source: &str) -> Vec<Option<Color>> {
        let mut result = vec![None; source.len()];
        let mut previous = std::mem::take(&mut self.0);
        for code in Structure::parse(source).code {
            let block = &source[code.range.clone()];
            let colors = previous
                .remove(block)
                .or_else(|| self.0.get(block).cloned())
                .unwrap_or_else(|| {
                    if code.is_mermaid() {
                        crate::mermaid::colors(block)
                    } else {
                        highlight(&code.language, block)
                    }
                });
            result[code.range].copy_from_slice(&colors);
            self.0.insert(block.to_owned(), colors);
        }
        result
    }
}

/// syntect colors for one fenced block, keyed by the first word of its info string.
fn highlight(language: &str, block: &str) -> Vec<Option<Color>> {
    static SYNTAXES: OnceLock<SyntaxSet> = OnceLock::new();
    static THEMES: OnceLock<ThemeSet> = OnceLock::new();
    let syntaxes = SYNTAXES.get_or_init(SyntaxSet::load_defaults_newlines);
    let themes = THEMES.get_or_init(ThemeSet::load_defaults);
    let language = language.split_whitespace().next().unwrap_or("");
    let syntax = syntaxes
        .find_syntax_by_token(language)
        .unwrap_or_else(|| syntaxes.find_syntax_plain_text());
    let mut highlighter = HighlightLines::new(syntax, &themes.themes["base16-ocean.dark"]);
    let mut colors = vec![None; block.len()];
    let mut at = 0;
    for line in block.split_inclusive('\n') {
        if !is_fence(line)
            && let Ok(spans) = highlighter.highlight_line(line, syntaxes)
        {
            let mut offset = at;
            for (style, text) in spans {
                let c = style.foreground;
                colors[offset..offset + text.len()].fill(Some(Color::from_rgb8(c.r, c.g, c.b)));
                offset += text.len();
            }
        }
        at += line.len();
    }
    colors
}

#[cfg(test)]
mod tests {
    use super::*;
    fn colors(source: &str) -> Vec<Option<Color>> {
        Cache::default().colors(source)
    }
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
    #[test]
    fn code_is_highlighted_by_language() {
        let colors = colors("```rust\nfn main() { let n = 42; }\n```\n");
        let distinct: std::collections::HashSet<_> = colors
            .iter()
            .flatten()
            .map(|c| (c.r.to_bits(), c.g.to_bits(), c.b.to_bits()))
            .collect();
        assert!(distinct.len() > 2);
    }
}
