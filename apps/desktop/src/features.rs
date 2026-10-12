use super::{App, Message};
use iced::{
    Element, Length, Task,
    widget::{self, button, column, container, row, scrollable, text, text_input},
};
use norito_model::structure::Structure;
use norito_platform::*;
use norito_viewmodel::{Action, Command};
use std::{collections::HashMap, ops::Range};

#[derive(Debug, Clone, Copy)]
pub enum Kind {
    Table,
    Code,
    Mermaid,
}
#[derive(Debug, Clone)]
pub enum Event {
    Find,
    FindClose,
    Query(String),
    Replacement(String),
    FindNext(bool),
    Replace(bool),
    MatchCase(bool),
    Regex(bool),
    Open(Kind),
    ToggleOutline,
    Jump(usize),
    Follow(String),
    ExternalOpened(Result<()>),
    Resource(bool),
    ResourceReady(Result<Option<ResourceLink>>, bool, u64, u64, Range<usize>),
    PageReady(Result<FileRef>, String, u64),
    Diagram(String, std::result::Result<String, String>),
    Image(String, Result<Vec<u8>>, u64),
}
#[derive(Default)]
pub struct Features {
    pub search: Option<Search>,
    pub outline: bool,
    pub structure: Structure,
    pub diagrams: HashMap<String, std::result::Result<widget::svg::Handle, String>>,
    pub images: HashMap<String, std::result::Result<widget::image::Handle, String>>,
    pending_diagrams: std::collections::HashSet<String>,
    pub pending_anchor: Option<String>,
    epoch: u64,
}
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
}
fn message(event: Event) -> Message {
    Message::Feature(event)
}

impl App {
    pub(super) fn refresh_features(&mut self) -> Task<Message> {
        self.features.structure = Structure::parse(&self.vm.document.source);
        let switched = self.features.epoch != self.vm.document_epoch;
        if switched {
            self.features.epoch = self.vm.document_epoch;
            self.features.images.clear();
        }
        let mut tasks = Vec::new();
        let diagrams: Vec<_> = self
            .features
            .structure
            .code
            .iter()
            .filter(|code| code.language.trim() == "mermaid")
            .map(|code| code.content.clone())
            .collect();
        for source in diagrams {
            tasks.push(self.render_diagram(source));
        }
        for source in self
            .features
            .structure
            .math
            .iter()
            .map(|m| format!("math:{}", m.content))
            .collect::<Vec<_>>()
        {
            tasks.push(self.render_diagram(source));
        }
        if let Some(file) = &self.vm.file {
            for link in self
                .features
                .structure
                .links
                .iter()
                .filter(|link| link.image)
            {
                if self.features.images.contains_key(&link.destination) {
                    continue;
                }
                let destination = link.destination.clone();
                self.features
                    .images
                    .insert(destination.clone(), Err("読み込み中…".into()));
                let resources = self.resources.clone();
                let file = file.file.handle;
                let epoch = self.vm.document_epoch;
                tasks.push(Task::perform(
                    async move {
                        let url = destination.clone();
                        let result =
                            tokio::task::spawn_blocking(move || resources.image_bytes(file, &url))
                                .await
                                .unwrap_or_else(|e| {
                                    Err(PlatformError {
                                        kind: ErrorKind::Io,
                                        message: e.to_string(),
                                    })
                                });
                        (destination, result)
                    },
                    move |(url, result)| message(Event::Image(url, result, epoch)),
                ));
            }
        }
        if let Some(anchor) = switched
            .then(|| self.features.pending_anchor.take())
            .flatten()
        {
            if let Some(heading) = self
                .features
                .structure
                .headings
                .iter()
                .find(|h| h.anchor == anchor)
            {
                self.editor.jump(heading.offset);
            } else if !anchor.is_empty() {
                self.vm.status = "リンク先の見出しが見つかりません。".into();
                self.vm.error = true;
            }
        }
        Task::batch(tasks)
    }
    fn render_diagram(&mut self, source: String) -> Task<Message> {
        if self.features.diagrams.contains_key(&source)
            || !self.features.pending_diagrams.insert(source.clone())
        {
            return Task::none();
        }
        Task::perform(
            async move {
                let input = source.clone();
                let result = tokio::task::spawn_blocking(move || {
                    if input.len() > 32_000 {
                        return Err("図が大きすぎます（32KBまで）。".into());
                    }
                    if let Some(math) = input.strip_prefix("math:") {
                        let font =
                            latex_rust::MathFont::stix_two_math().map_err(|e| e.to_string())?;
                        return latex_rust::latex_to_svg(
                            math,
                            &font,
                            &latex_rust::SvgOptions::new(),
                        )
                        .map_err(|e| e.to_string());
                    }
                    std::panic::catch_unwind(|| mermaid_rs_renderer::render(&input))
                        .map_err(|_| "図の描画に失敗しました。".to_owned())?
                        .map_err(|e| e.to_string())
                })
                .await
                .unwrap_or_else(|e| Err(e.to_string()));
                (source, result)
            },
            |(source, result)| message(Event::Diagram(source, result)),
        )
    }
    pub(super) fn feature(&mut self, event: Event) -> Task<Message> {
        if self.composing && !matches!(event, Event::Diagram(..) | Event::Image(..)) {
            return Task::none();
        }
        match event {
            Event::Find => {
                self.editor.finish_editing();
                self.features.search.get_or_insert_with(Search::default);
                return widget::operation::focus("find");
            }
            Event::FindClose => {
                self.features.search = None;
                return widget::operation::focus("document");
            }
            Event::Query(value) => {
                if let Some(search) = &mut self.features.search {
                    search.query = value;
                }
            }
            Event::Replacement(value) => {
                if let Some(search) = &mut self.features.search {
                    search.replacement = value;
                }
            }
            Event::MatchCase(value) => {
                if let Some(search) = &mut self.features.search {
                    search.match_case = value;
                }
            }
            Event::Regex(value) => {
                if let Some(search) = &mut self.features.search {
                    search.regex = value;
                }
            }
            Event::FindNext(backward) => {
                if let Some(pattern) = self.features.search.as_ref().and_then(Search::pattern) {
                    let matches: Vec<_> = pattern
                        .find_iter(&self.vm.document.source)
                        .filter(|m| !m.is_empty())
                        .map(|m| m.range())
                        .collect();
                    let selection = self.editor.source_selection();
                    let found = if backward {
                        matches
                            .iter()
                            .rev()
                            .find(|r| r.end <= selection.start)
                            .or_else(|| matches.last())
                    } else {
                        matches
                            .iter()
                            .find(|r| r.start >= selection.end && **r != selection)
                            .or_else(|| matches.first())
                    };
                    if let Some(range) = found {
                        self.editor.select_source(range.clone());
                    }
                }
            }
            Event::Replace(all) => {
                if !self.vm.can_edit() {
                    return Task::none();
                }
                if let Some(search) = &self.features.search
                    && let Some(pattern) = search.pattern()
                {
                    let source = &self.vm.document.source;
                    let replacement = &search.replacement;
                    let result = if all {
                        if search.regex {
                            pattern.replace_all(source, replacement).into_owned()
                        } else {
                            pattern
                                .replace_all(source, regex::NoExpand(replacement))
                                .into_owned()
                        }
                    } else {
                        let range = self.editor.source_selection();
                        if pattern
                            .find(&source[range.clone()])
                            .is_none_or(|m| m.range() != (0..range.len()))
                        {
                            return self.feature(Event::FindNext(false));
                        }
                        let value = if search.regex {
                            pattern
                                .replace(&source[range.clone()], replacement)
                                .into_owned()
                        } else {
                            replacement.clone()
                        };
                        let mut result = source.clone();
                        result.replace_range(range, &value);
                        result
                    };
                    return self.dispatch(Command::Edit(result), false);
                }
            }
            Event::ToggleOutline => self.features.outline = !self.features.outline,
            Event::Open(kind) => {
                if !self.vm.can_edit() {
                    return Task::none();
                }
                self.context = None;
                self.mode = norito_editor_widget::Mode::Rich;
                self.editor.set_mode(self.mode);
                if matches!(kind, Kind::Table) {
                    self.editor.open_table();
                    return self
                        .editor
                        .table_focus_id()
                        .map_or_else(Task::none, widget::operation::focus);
                }
                let selection = self.editor.source_selection();
                if let Some(code) = self
                    .features
                    .structure
                    .code
                    .iter()
                    .find(|c| c.range.contains(&selection.start))
                {
                    self.editor.edit_code(code.range.start);
                    return widget::operation::focus("document");
                }
                let language = if matches!(kind, Kind::Mermaid) {
                    "mermaid"
                } else {
                    "text"
                };
                let source = self.editor.insert_code(language);
                return Task::batch([
                    self.dispatch(Command::Edit(source), false),
                    widget::operation::focus("document"),
                ]);
            }
            Event::Jump(offset) => {
                self.editor.jump(offset);
                return widget::operation::focus("document");
            }
            Event::Follow(destination) => {
                if destination.split_once(':').is_some_and(|(scheme, _)| {
                    ["http", "https", "mailto"]
                        .iter()
                        .any(|allowed| scheme.eq_ignore_ascii_case(allowed))
                }) {
                    let resources = self.resources.clone();
                    return Task::perform(
                        async move {
                            tokio::task::spawn_blocking(move || {
                                resources.open_external(&destination)
                            })
                            .await
                            .unwrap_or_else(|e| {
                                Err(PlatformError {
                                    kind: ErrorKind::Io,
                                    message: e.to_string(),
                                })
                            })
                        },
                        |result| message(Event::ExternalOpened(result)),
                    );
                }
                let (path, anchor) = destination.split_once('#').unwrap_or((&destination, ""));
                let decoded_anchor = norito_model::decode_fragment(anchor);
                let anchor = decoded_anchor.as_str();
                if path.is_empty() {
                    if let Some(name) = anchor.strip_prefix("fn:")
                        && let Some((_, offset)) = self
                            .features
                            .structure
                            .footnotes
                            .iter()
                            .find(|(label, _)| label == name)
                    {
                        self.editor.jump(*offset);
                        return widget::operation::focus("document");
                    }
                    if let Some(h) = self
                        .features
                        .structure
                        .headings
                        .iter()
                        .find(|h| h.anchor == anchor)
                    {
                        self.editor.jump(h.offset);
                        return widget::operation::focus("document");
                    }
                    self.vm.status = "見出しが見つかりません。".into();
                    self.vm.error = true;
                    return Task::none();
                }
                let Some(file) = &self.vm.file else {
                    self.vm.status = "ページ間リンクを開くには文書を保存してください。".into();
                    self.vm.error = true;
                    return Task::none();
                };
                let file = file.file.handle;
                let resources = self.resources.clone();
                let extension = path
                    .rsplit('.')
                    .next()
                    .unwrap_or_default()
                    .to_ascii_lowercase();
                if !matches!(extension.as_str(), "md" | "markdown") {
                    return Task::perform(
                        async move {
                            tokio::task::spawn_blocking(move || {
                                resources.open_attachment(file, &destination)
                            })
                            .await
                            .unwrap_or_else(|e| {
                                Err(PlatformError {
                                    kind: ErrorKind::Io,
                                    message: e.to_string(),
                                })
                            })
                        },
                        |result| message(Event::ExternalOpened(result)),
                    );
                }
                let anchor = anchor.to_owned();
                let epoch = self.vm.document_epoch;
                return Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            resources.resolve_page(file, &destination)
                        })
                        .await
                        .unwrap_or_else(|e| {
                            Err(PlatformError {
                                kind: ErrorKind::Io,
                                message: e.to_string(),
                            })
                        })
                    },
                    move |result| message(Event::PageReady(result, anchor.clone(), epoch)),
                );
            }
            Event::ExternalOpened(result) => {
                if let Err(error) = result {
                    self.vm.error = true;
                    self.vm.status = error.message;
                }
            }
            Event::PageReady(result, anchor, epoch) => {
                if epoch != self.vm.document_epoch || self.vm.busy() || self.vm.pending.is_some() {
                    return Task::none();
                }
                match result {
                    Ok(file) => {
                        let task = self.dispatch(Command::Navigate(Action::OpenFile(file)), false);
                        self.features.pending_anchor = Some(anchor);
                        return task;
                    }
                    Err(e) => {
                        self.vm.status = e.message;
                        self.vm.error = true;
                    }
                }
            }
            Event::Resource(image) => {
                if !self.vm.can_edit() {
                    return Task::none();
                }
                let Some(file) = &self.vm.file else {
                    self.vm.status =
                        "画像・ページリンクを挿入する前に文書を保存してください。".into();
                    self.vm.error = true;
                    return Task::none();
                };
                let file = file.file.handle;
                let resources = self.resources.clone();
                let epoch = self.vm.document_epoch;
                let revision = self.vm.document.revision;
                let selection = self.editor.source_selection();
                return Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            if image {
                                resources.choose_image(file)
                            } else {
                                resources.choose_page(file)
                            }
                        })
                        .await
                        .unwrap_or_else(|e| {
                            Err(PlatformError {
                                kind: ErrorKind::Io,
                                message: e.to_string(),
                            })
                        })
                    },
                    move |result| {
                        message(Event::ResourceReady(
                            result,
                            image,
                            epoch,
                            revision,
                            selection.clone(),
                        ))
                    },
                );
            }
            Event::ResourceReady(result, image, epoch, revision, selection) => {
                if epoch != self.vm.document_epoch || revision != self.vm.document.revision {
                    return Task::none();
                }
                match result {
                    Ok(Some(link)) => {
                        let label = if !image
                            && selection == self.editor.source_selection()
                            && !self.editor.selected_text().is_empty()
                        {
                            self.editor.selected_text()
                        } else {
                            link.label
                        };
                        let label = label
                            .replace('\\', "\\\\")
                            .replace('[', "\\[")
                            .replace(']', "\\]");
                        let markdown = format!(
                            "{}[{}](<{}>)",
                            if image { "!" } else { "" },
                            label,
                            link.destination
                        );
                        let source = self.editor.insert_markdown(selection, &markdown, image);
                        self.features.outline = true;
                        return Task::batch([
                            self.dispatch(Command::Edit(source), true),
                            widget::operation::focus("document"),
                        ]);
                    }
                    Err(e) => {
                        self.vm.status = e.message;
                        self.vm.error = true;
                    }
                    _ => {}
                }
            }
            Event::Diagram(source, result) => {
                self.features.pending_diagrams.remove(&source);
                if self.features.diagrams.len() > 64 {
                    self.features.diagrams.clear();
                }
                self.features.diagrams.insert(
                    source,
                    result.map(|svg| widget::svg::Handle::from_memory(svg.into_bytes())),
                );
            }
            Event::Image(url, result, epoch) => {
                if epoch == self.vm.document_epoch {
                    self.features.images.insert(
                        url,
                        result
                            .map(widget::image::Handle::from_bytes)
                            .map_err(|e| e.message),
                    );
                }
            }
        }
        Task::none()
    }
    pub(super) fn outline_view(&self) -> Element<'_, Message> {
        let c = super::theme::colors(self.light);
        let mut content = column![text("目次").size(12).color(c.muted)].spacing(10);
        for h in &self.features.structure.headings {
            content = content.push(
                button(text(&h.title).size(12))
                    .padding(iced::Padding {
                        left: 8.0 + (h.level - 1) as f32 * 10.0,
                        ..iced::Padding::from(5)
                    })
                    .on_press(message(Event::Jump(h.offset)))
                    .style(widget::button::text),
            );
        }
        if self.features.structure.headings.is_empty() {
            content = content.push(text("見出しなし").size(12).color(c.muted));
        }
        for link in &self.features.structure.links {
            if link.image {
                content = content.push(text(&link.label).size(12).color(c.muted));
                match self.features.images.get(&link.destination) {
                    Some(Ok(handle)) => {
                        content = content.push(widget::image(handle.clone()).width(Length::Fill))
                    }
                    Some(Err(error)) => content = content.push(text(error).size(11).color(c.muted)),
                    _ => {}
                }
            } else {
                content = content.push(
                    button(text(format!("↗ {}", link.label)).size(12))
                        .on_press(message(Event::Follow(link.destination.clone())))
                        .style(widget::button::text),
                );
            }
        }
        for code in &self.features.structure.code {
            if code.language.trim() == "mermaid" {
                content = content.push(self.diagram_view(&code.content));
            }
            content = content.push(
                button(
                    text(if code.language.is_empty() {
                        "code"
                    } else {
                        &code.language
                    })
                    .size(11),
                )
                .on_press(Message::EditBlock(code.range.start, Kind::Code))
                .style(widget::button::text),
            );
        }
        for (i, table) in self.features.structure.tables.iter().enumerate() {
            content = content.push(
                button(text(format!("表 {}", i + 1)).size(12))
                    .on_press(Message::EditBlock(table.range.start, Kind::Table))
                    .style(widget::button::text),
            );
        }
        container(scrollable(content))
            .padding(16)
            .width(240)
            .height(Length::Fill)
            .into()
    }
    pub(super) fn search_view(&self) -> Element<'_, Message> {
        let Some(search) = &self.features.search else {
            return widget::space().into();
        };
        let count = search
            .pattern()
            .map(|p| p.find_iter(&self.vm.document.source).count());
        container(
            column![
                row![
                    text_input("検索", &search.query)
                        .id("find")
                        .on_input(|v| message(Event::Query(v)))
                        .on_submit(message(Event::FindNext(false)))
                        .size(13),
                    self.icon_control(
                        super::icons::UP,
                        "前の一致",
                        message(Event::FindNext(true)),
                        true,
                        false
                    ),
                    self.icon_control(
                        super::icons::DOWN,
                        "次の一致",
                        message(Event::FindNext(false)),
                        true,
                        false
                    ),
                    text(count.map_or_else(|| "—".into(), |n| format!("{n}件"))).size(12),
                    self.icon_control(
                        super::icons::CLOSE,
                        "検索を閉じる",
                        message(Event::FindClose),
                        true,
                        false
                    )
                ]
                .spacing(8)
                .align_y(iced::Alignment::Center),
                row![
                    text_input("置換", &search.replacement)
                        .on_input(|v| message(Event::Replacement(v)))
                        .size(13),
                    self.icon_control(
                        super::icons::REPLACE,
                        "置換",
                        message(Event::Replace(false)),
                        true,
                        false
                    ),
                    self.icon_control(
                        super::icons::REPLACE_ALL,
                        "すべて置換",
                        message(Event::Replace(true)),
                        true,
                        false
                    ),
                    self.icon_control(
                        super::icons::CASE,
                        "大文字小文字を区別",
                        message(Event::MatchCase(!search.match_case)),
                        true,
                        search.match_case
                    ),
                    self.icon_control(
                        super::icons::REGEX,
                        "正規表現",
                        message(Event::Regex(!search.regex)),
                        true,
                        search.regex
                    )
                ]
                .spacing(8)
                .align_y(iced::Alignment::Center)
            ]
            .spacing(6),
        )
        .padding([8, 24])
        .into()
    }
    fn diagram_view<'a>(&'a self, source: &str) -> Element<'a, Message> {
        match self.features.diagrams.get(source) {
            Some(Ok(handle)) => container(widget::svg(handle.clone()).width(Length::Fill))
                .padding(8)
                .style(|_| widget::container::Style {
                    background: Some(iced::Color::WHITE.into()),
                    ..Default::default()
                })
                .into(),
            Some(Err(error)) => text(format!("Mermaid: {error}")).size(12).into(),
            None => text("図を描画中…").size(12).into(),
        }
    }
}
