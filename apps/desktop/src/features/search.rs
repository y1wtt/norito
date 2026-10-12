//! Find and replace over the source. Replacements are ordinary, undoable edits.
use super::{Event, message};
use crate::{App, Message, icons};
use iced::{
    Element, Task,
    widget::{self, column, container, row, text, text_input},
};
use norito_viewmodel::Command;
use std::ops::Range;

#[derive(Default)]
pub struct Search {
    pub query: String,
    pub replacement: String,
    pub match_case: bool,
    pub regex: bool,
}

impl Search {
    fn pattern(&self) -> Option<regex::Regex> {
        if self.query.is_empty() {
            return None;
        }
        regex::RegexBuilder::new(&if self.regex {
            self.query.clone()
        } else {
            regex::escape(&self.query)
        })
        .case_insensitive(!self.match_case)
        .build()
        .ok()
    }
    pub fn count(&self, source: &str) -> Option<usize> {
        Some(self.pattern()?.find_iter(source).count())
    }
    /// The next non-empty match after `selection` (or before it), wrapping around.
    pub fn find(
        &self,
        source: &str,
        selection: &Range<usize>,
        backward: bool,
    ) -> Option<Range<usize>> {
        let matches: Vec<_> = self
            .pattern()?
            .find_iter(source)
            .filter(|m| !m.is_empty())
            .map(|m| m.range())
            .collect();
        if backward {
            matches
                .iter()
                .rev()
                .find(|r| r.end <= selection.start)
                .or_else(|| matches.last())
        } else {
            matches
                .iter()
                .find(|r| r.start >= selection.end && *r != selection)
                .or_else(|| matches.first())
        }
        .cloned()
    }
    /// Every match replaced. `$1`-style references expand only in regex mode.
    pub fn replace_all(&self, source: &str) -> Option<String> {
        let pattern = self.pattern()?;
        Some(if self.regex {
            pattern.replace_all(source, &self.replacement).into_owned()
        } else {
            pattern
                .replace_all(source, regex::NoExpand(&self.replacement))
                .into_owned()
        })
    }
    /// `selection` replaced, if it is exactly one match.
    pub fn replace_selection(&self, source: &str, selection: Range<usize>) -> Option<String> {
        let pattern = self.pattern()?;
        let selected = &source[selection.clone()];
        if pattern
            .find(selected)
            .is_none_or(|m| m.range() != (0..selected.len()))
        {
            return None;
        }
        let value = if self.regex {
            pattern.replace(selected, &self.replacement).into_owned()
        } else {
            self.replacement.clone()
        };
        let mut result = source.to_owned();
        result.replace_range(selection, &value);
        Some(result)
    }
}

impl App {
    pub(super) fn edit_search(&mut self, change: impl FnOnce(&mut Search)) -> Task<Message> {
        if let Some(search) = &mut self.features.search {
            change(search);
        }
        Task::none()
    }
    pub(super) fn find_next(&mut self, backward: bool) -> Task<Message> {
        let selection = self.editor.source_selection();
        if let Some(range) =
            self.features.search.as_ref().and_then(|search| {
                search.find(&self.vm.session.document.source, &selection, backward)
            })
        {
            self.editor.select_source(range);
        }
        Task::none()
    }
    /// Replace all, or the selected match and then move to the next one.
    pub(super) fn replace(&mut self, all: bool) -> Task<Message> {
        if !self.vm.can_edit() {
            return Task::none();
        }
        let Some(search) = &self.features.search else {
            return Task::none();
        };
        let source = &self.vm.session.document.source;
        let result = if all {
            search.replace_all(source)
        } else if search.pattern().is_none() {
            None
        } else {
            match search.replace_selection(source, self.editor.source_selection()) {
                Some(result) => Some(result),
                None => return self.find_next(false),
            }
        };
        match result {
            Some(result) => self.dispatch(Command::Edit(result), false),
            None => Task::none(),
        }
    }
    pub(crate) fn search_view(&self) -> Element<'_, Message> {
        let Some(search) = &self.features.search else {
            return widget::space().into();
        };
        let count = search.count(&self.vm.session.document.source);
        let control = |icon, label, event, selected| {
            self.icon_control(icon, label, message(event), true, selected)
        };
        container(
            column![
                row![
                    text_input("検索", &search.query)
                        .id("find")
                        .on_input(|v| message(Event::Query(v)))
                        .on_submit(message(Event::FindNext(false)))
                        .size(13),
                    control(icons::UP, "前の一致", Event::FindNext(true), false),
                    control(icons::DOWN, "次の一致", Event::FindNext(false), false),
                    text(count.map_or_else(|| "—".into(), |n| format!("{n}件"))).size(12),
                    control(icons::CLOSE, "検索を閉じる", Event::FindClose, false),
                ]
                .spacing(8)
                .align_y(iced::Alignment::Center),
                row![
                    text_input("置換", &search.replacement)
                        .on_input(|v| message(Event::Replacement(v)))
                        .size(13),
                    control(icons::REPLACE, "置換", Event::Replace(false), false),
                    control(
                        icons::REPLACE_ALL,
                        "すべて置換",
                        Event::Replace(true),
                        false
                    ),
                    control(
                        icons::CASE,
                        "大文字小文字を区別",
                        Event::MatchCase(!search.match_case),
                        search.match_case
                    ),
                    control(
                        icons::REGEX,
                        "正規表現",
                        Event::Regex(!search.regex),
                        search.regex
                    ),
                ]
                .spacing(8)
                .align_y(iced::Alignment::Center)
            ]
            .spacing(6),
        )
        .padding([8, 24])
        .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn search(query: &str, replacement: &str, regex: bool) -> Search {
        Search {
            query: query.into(),
            replacement: replacement.into(),
            regex,
            ..Default::default()
        }
    }
    #[test]
    fn find_wraps_and_replace_needs_an_exact_selection() {
        let s = search("a", "$0$0", false);
        assert_eq!(s.find("a b a", &(0..1), false), Some(4..5));
        assert_eq!(s.find("a b a", &(4..5), false), Some(0..1));
        assert_eq!(s.find("a b a", &(0..0), true), Some(4..5));
        assert_eq!(s.replace_all("aXa").unwrap(), "$0$0X$0$0");
        assert_eq!(s.replace_selection("ab", 0..2), None);
        let r = search("(b)", "[$1]", true);
        assert_eq!(r.replace_all("abc").unwrap(), "a[b]c");
        assert_eq!(r.replace_selection("abc", 1..2).unwrap(), "a[b]c");
        assert_eq!(search("", "x", false).count("abc"), None);
    }
}
