//! The sidebar: folder header, creation buttons, and the lazily listed tree.
use super::controls::{ellipsize, outline_icon, panel};
use crate::{App, Message, icons, theme};
use iced::{
    Element, Length,
    widget::{Column, button, column, container, row, scrollable, svg, text, text_input, tooltip},
};
use norito_platform::{DirectoryHandle, Entry};
use norito_viewmodel::{Action, Command, EntryKind};
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

/// Chevron rotation animations, in degrees (0 closed, 90 open).
#[derive(Default)]
pub struct Chevrons {
    pub motion: HashMap<DirectoryHandle, (f32, f32, Instant)>,
    pub now: Option<Instant>,
}
const DURATION: Duration = Duration::from_millis(150);

impl Chevrons {
    fn now(&self) -> Instant {
        self.now.unwrap_or_else(Instant::now)
    }
    /// Start turning towards the state opposite `expanded`, from wherever the chevron is now.
    pub fn start(&mut self, id: DirectoryHandle, expanded: bool) {
        let now = Instant::now();
        let from = self.angle(id, expanded);
        self.now = Some(now);
        self.motion
            .insert(id, (from, if expanded { 0.0 } else { 90.0 }, now));
    }
    pub fn tick(&mut self, now: Instant) {
        self.now = Some(now);
        self.motion
            .retain(|_, (_, _, start)| now.duration_since(*start) < DURATION);
    }
    pub fn animating(&self) -> bool {
        !self.motion.is_empty()
    }
    /// Ease-out cubic between the endpoints of a running animation.
    pub fn angle(&self, id: DirectoryHandle, expanded: bool) -> f32 {
        if let Some((from, to, start)) = self.motion.get(&id) {
            let elapsed = self.now().saturating_duration_since(*start);
            let t = (elapsed.as_secs_f32() / DURATION.as_secs_f32()).min(1.0);
            from + (to - from) * (1.0 - (1.0 - t).powi(3))
        } else if expanded {
            90.0
        } else {
            0.0
        }
    }
}

impl App {
    fn creation_target(&self) -> Option<DirectoryHandle> {
        self.vm.workspace.creation_directory().map(|dir| dir.handle)
    }
    pub(crate) fn tree_angle(&self, id: DirectoryHandle) -> f32 {
        self.chevrons
            .angle(id, self.vm.workspace.expanded.contains(&id))
    }
    pub(super) fn sidebar_view(&self) -> Element<'_, Message> {
        let c = theme::colors(self.light);
        let ready = !self.vm.busy();
        let mut sidebar = column![].spacing(8);
        if let Some(root) = &self.vm.workspace.root {
            // Accent marks the directory that receives new files.
            let color = if self.creation_target() == Some(root.handle) {
                c.accent
            } else {
                c.text
            };
            let header = tooltip(
                button(
                    row![
                        outline_icon(icons::FOLDER, 16.0, color),
                        text(ellipsize(&root.name, 8.0))
                            .size(13)
                            .color(color)
                            .wrapping(text::Wrapping::None)
                    ]
                    .spacing(6)
                    .align_y(iced::Alignment::Center),
                )
                .padding([5, 6])
                .width(Length::Fill)
                .clip(true)
                .on_press(Message::Command(Command::ToggleDirectory(root.clone())))
                .style(move |_, status| theme::control(c, false, status)),
                container(
                    text(if root.location.is_empty() {
                        &root.name
                    } else {
                        &root.location
                    })
                    .size(11)
                    .wrapping(text::Wrapping::WordOrGlyph),
                )
                .max_width(360)
                .padding([5, 8])
                .style(panel(c)),
                tooltip::Position::Bottom,
            );
            let idle = ready && self.vm.creation.is_none();
            sidebar = sidebar
                .push(
                    row![
                        header,
                        self.icon_control(
                            icons::FILE_ADD,
                            "新規ファイル  Ctrl+N",
                            Message::Command(Command::Navigate(Action::New)),
                            idle,
                            false
                        ),
                        self.icon_control(
                            icons::FOLDER_ADD,
                            "新規フォルダ  Ctrl+Shift+N",
                            Message::Command(Command::NewDirectory),
                            idle,
                            false
                        )
                    ]
                    .align_y(iced::Alignment::Center),
                )
                .push(scrollable(self.tree(root.handle, 0)).height(Length::Fill));
        } else {
            sidebar = sidebar
                .push(container(text("フォルダ未選択").size(12).color(c.muted)).padding([4, 10]))
                .push(self.control(
                    "フォルダを開く",
                    Message::Command(Command::OpenFolder),
                    ready,
                    false,
                ));
        }
        container(sidebar)
            .padding([16, 8])
            .width(200)
            .height(Length::Fill)
            .style(move |_| container::Style {
                background: Some(c.surface.into()),
                ..Default::default()
            })
            .into()
    }
    fn tree(&self, dir: DirectoryHandle, depth: usize) -> Column<'_, Message> {
        let mut tree = Column::new().spacing(2);
        let c = theme::colors(self.light);
        let indent = iced::Padding {
            left: (depth.min(8) * 12) as f32,
            ..Default::default()
        };
        if let Some(creation) = &self.vm.creation
            && creation.directory.handle == dir
        {
            let placeholder = match creation.kind {
                EntryKind::Directory => "フォルダ名",
                EntryKind::File => "ファイル名.md",
            };
            tree = tree.push(
                container(
                    text_input(placeholder, &creation.name)
                        .id("new-file")
                        .size(12)
                        .padding([6, 8])
                        .on_input_maybe(
                            (!self.vm.busy())
                                .then_some(|name| Message::Command(Command::NewName(name))),
                        )
                        .on_submit(Message::Command(Command::CreateFile)),
                )
                .padding(indent),
            );
        }
        let Some(entries) = self.vm.workspace.children.get(&dir) else {
            return tree;
        };
        for entry in entries {
            tree = tree.push(container(self.entry(entry)).padding(indent));
            if let Entry::Directory(dir) = entry
                && self.vm.workspace.expanded.contains(&dir.handle)
            {
                tree = tree.push(self.tree(dir.handle, depth + 1));
            }
        }
        if entries.is_empty() && self.vm.creation.is_none() {
            tree = tree.push(container(text("ファイルなし").size(12).color(c.muted)).padding(10));
        }
        tree
    }
    fn entry<'a>(&self, entry: &'a Entry) -> Element<'a, Message> {
        let c = theme::colors(self.light);
        let session = &self.vm.session;
        let (label, command, selected) = match entry {
            Entry::File(file) => {
                let current = session.is(file.handle);
                let mark = if current && session.document.dirty() {
                    "  ●"
                } else {
                    ""
                };
                (
                    format!("{}{mark}", file.name),
                    Command::Navigate(Action::OpenFile(file.clone())),
                    current,
                )
            }
            Entry::Directory(dir) => (
                dir.name.clone(),
                Command::ToggleDirectory(dir.clone()),
                false,
            ),
        };
        let target =
            matches!(entry, Entry::Directory(dir) if self.creation_target() == Some(dir.handle));
        let mut label = row![text(label).size(12).color_maybe(target.then_some(c.accent))]
            .spacing(4)
            .align_y(iced::Alignment::Center);
        if let Entry::Directory(dir) = entry {
            let angle = self.tree_angle(dir.handle);
            let source = format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><path d="M6 4l4 4-4 4" transform="rotate({angle} 8 8)" fill="none" stroke="black" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"/></svg>"#
            );
            let icon = svg(svg::Handle::from_memory(source.into_bytes()))
                .width(14)
                .height(14)
                .style(move |_, _| svg::Style {
                    color: Some(if target { c.accent } else { c.muted }),
                });
            label = row![icon, label]
                .spacing(4)
                .align_y(iced::Alignment::Center);
        }
        button(label)
            .padding([5, 8])
            .width(Length::Fill)
            .on_press_maybe((!self.vm.busy()).then_some(Message::Command(command)))
            .style(move |_, status| theme::control(c, selected, status))
            .into()
    }
}
