use super::{App, EditAction, Message, menu::MenuAction, theme};
use iced::{
    Element, Length,
    widget::{
        Column, button, column, container, mouse_area, opaque, responsive, row, scrollable, space,
        stack, text, text_input, tooltip,
    },
};
use norito_editor_widget::Mode;
use norito_platform::{DirectoryHandle, Entry};
use norito_viewmodel::{Action, Command};

impl App {
    pub(super) fn tree_angle(&self, id: norito_platform::DirectoryHandle) -> f32 {
        if let Some((from, to, start)) = self.tree_motion.get(&id) {
            let t = (self
                .animation_time
                .saturating_duration_since(*start)
                .as_secs_f32()
                / 0.15)
                .min(1.0);
            from + (to - from) * (1.0 - (1.0 - t).powi(3))
        } else if self.vm.expanded.contains(&id) {
            90.0
        } else {
            0.0
        }
    }
    pub(super) fn icon_control<'a>(
        &self,
        path: &str,
        label: &'a str,
        message: Message,
        enabled: bool,
        selected: bool,
    ) -> Element<'a, Message> {
        let c = theme::colors(self.light);
        let svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20"><path d="{path}" fill="none" stroke="black" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"/></svg>"#
        );
        let icon = iced::widget::svg(iced::widget::svg::Handle::from_memory(svg.into_bytes()))
            .width(18)
            .height(18)
            .style(move |_, _| iced::widget::svg::Style {
                color: Some(if !enabled {
                    c.muted.scale_alpha(0.45)
                } else if selected {
                    c.accent
                } else {
                    c.text
                }),
            });
        tooltip(
            button(icon)
                .padding(6)
                .width(30)
                .height(30)
                .on_press_maybe(enabled.then_some(message))
                .style(move |_, status| theme::control(c, selected, status)),
            container(text(label).size(11))
                .padding([5, 8])
                .style(move |_| norito_iced_ext::panel(c.surface, c.line)),
            tooltip::Position::Bottom,
        )
        .into()
    }
    fn control<'a>(
        &self,
        label: &'a str,
        message: Message,
        enabled: bool,
        selected: bool,
    ) -> Element<'a, Message> {
        let c = theme::colors(self.light);
        button(text(label).size(12))
            .padding([8, 12])
            .on_press_maybe(enabled.then_some(message))
            .style(move |_, status| theme::control(c, selected, status))
            .into()
    }
    fn table_toolbar(&self, remove_row: bool, remove_column: bool) -> Element<'_, Message> {
        use norito_editor_widget::{EditorEvent, TableAction};
        let c = theme::colors(self.light);
        let actions = [
            (
                "下に行を追加",
                "M3 3h14v8H3z M3 7h14 M10 13v6 M7 16h6",
                TableAction::Row(1),
                true,
            ),
            (
                "行を削除",
                "M3 3h14v8H3z M3 7h14 M7 16h6",
                TableAction::Row(-1),
                remove_row,
            ),
            (
                "右に列を追加",
                "M2 3h8v14H2z M6 3v14 M12 10h6 M15 7v6",
                TableAction::Column(1),
                true,
            ),
            (
                "列を削除",
                "M2 3h8v14H2z M6 3v14 M12 10h6",
                TableAction::Column(-1),
                remove_column,
            ),
            (
                "左揃え",
                "M3 4h14 M3 8h9 M3 12h14 M3 16h9",
                TableAction::Align(0),
                true,
            ),
            (
                "中央揃え",
                "M3 4h14 M6 8h8 M3 12h14 M6 16h8",
                TableAction::Align(1),
                true,
            ),
            (
                "右揃え",
                "M3 4h14 M8 8h9 M3 12h14 M8 16h9",
                TableAction::Align(2),
                true,
            ),
        ];
        let mut bar = row![].spacing(2);
        for (label, path, action, enabled) in actions {
            bar = bar.push(self.icon_control(
                path,
                label,
                Message::Editor(EditorEvent::Table(action)),
                enabled,
                false,
            ));
        }
        container(bar)
            .padding(3)
            .style(move |_| norito_iced_ext::panel(c.surface, c.line))
            .into()
    }
    fn menu<'a>(&self, index: usize, label: &'a str) -> Element<'a, Message> {
        let c = theme::colors(self.light);
        let selected = self.open_menu == Some(index);
        let control = button(
            text(label)
                .size(12)
                .width(Length::Fill)
                .align_x(iced::alignment::Horizontal::Center),
        )
        .width(if index == 0 { 72 } else { 48 })
        .padding([5, 6])
        .on_press(Message::MenuToggle(index))
        .style(move |_, status| theme::control(c, selected, status));
        let area = mouse_area(control);
        if self.open_menu.is_some() {
            area.on_enter(Message::MenuHover(index)).into()
        } else {
            area.into()
        }
    }
    fn tree<'a>(&'a self, dir: DirectoryHandle, depth: usize) -> Column<'a, Message> {
        let mut tree = Column::new().spacing(2);
        let c = theme::colors(self.light);
        if let Some((parent, name)) = &self.vm.creation
            && parent.handle == dir
        {
            tree = tree.push(
                container(
                    text_input(
                        if self.vm.creating_directory {
                            "フォルダ名"
                        } else {
                            "ファイル名.md"
                        },
                        name,
                    )
                    .id("new-file")
                    .size(12)
                    .padding([6, 8])
                    .on_input_maybe(
                        (!self.vm.busy())
                            .then_some(|name| Message::Command(Command::NewName(name))),
                    )
                    .on_submit(Message::Command(Command::CreateFile)),
                )
                .padding(iced::Padding {
                    left: (depth.min(8) * 12) as f32,
                    ..Default::default()
                }),
            );
        }
        if let Some(entries) = self.vm.children.get(&dir) {
            for entry in entries {
                let (label, command, selected) = match entry {
                    Entry::File(file) => (
                        format!(
                            "{}{}",
                            file.name,
                            if self.vm.document.dirty()
                                && self
                                    .vm
                                    .file
                                    .as_ref()
                                    .is_some_and(|current| current.file.handle == file.handle)
                            {
                                "  ●"
                            } else {
                                ""
                            }
                        ),
                        Command::Navigate(Action::OpenFile(file.clone())),
                        self.vm
                            .file
                            .as_ref()
                            .is_some_and(|t| t.file.handle == file.handle),
                    ),
                    Entry::Directory(dir) => (
                        dir.name.clone(),
                        Command::ToggleDirectory(dir.clone()),
                        false,
                    ),
                };
                let mut label = row![text(label).size(12)]
                    .spacing(4)
                    .align_y(iced::Alignment::Center);
                if let Entry::Directory(dir) = entry {
                    let angle = self.tree_angle(dir.handle);
                    let source = format!(
                        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><path d="M6 4l4 4-4 4" transform="rotate({angle} 8 8)" fill="none" stroke="black" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"/></svg>"#
                    );
                    let icon = iced::widget::svg(iced::widget::svg::Handle::from_memory(
                        source.into_bytes(),
                    ))
                    .width(14)
                    .height(14)
                    .style(move |_, _| iced::widget::svg::Style {
                        color: Some(c.muted),
                    });
                    label = row![icon, label]
                        .spacing(4)
                        .align_y(iced::Alignment::Center);
                }
                tree = tree.push(
                    container(
                        button(label)
                            .padding([5, 8])
                            .width(Length::Fill)
                            .on_press_maybe((!self.vm.busy()).then_some(Message::Command(command)))
                            .style(move |_, status| theme::control(c, selected, status)),
                    )
                    .padding(iced::Padding {
                        left: (depth.min(8) * 12) as f32,
                        ..Default::default()
                    }),
                );
                if let Entry::Directory(dir) = entry
                    && self.vm.expanded.contains(&dir.handle)
                {
                    tree = tree.push(self.tree(dir.handle, depth + 1));
                }
            }
            if entries.is_empty() && self.vm.creation.is_none() {
                tree =
                    tree.push(container(text("ファイルなし").size(12).color(c.muted)).padding(10));
            }
        }
        tree
    }
    pub(super) fn view(&self) -> Element<'_, Message> {
        let c = theme::colors(self.light);
        let ready = !self.vm.busy();
        let menus = row![
            self.menu(0, "ファイル"),
            self.menu(1, "書式"),
            self.menu(2, "挿入"),
            self.menu(3, "表示"),
        ]
        .spacing(4)
        .align_y(iced::Alignment::Center);
        let title = format!(
            "{}{}",
            self.vm.name(),
            if self.vm.document.dirty() { " •" } else { "" }
        );
        let title = mouse_area(
            container(text(title).size(13).color(c.muted))
                .center_x(Length::Fill)
                .height(36)
                .center_y(36),
        )
        .on_press(Message::Window(super::window_chrome::Action::Drag))
        .on_double_click(Message::Window(super::window_chrome::Action::Maximize));
        let brand = container(text("norito").size(13).color(c.text))
            .width(64)
            .padding([9, 16]);
        let mut header = row![brand].align_y(iced::Alignment::Center);
        if !cfg!(target_os = "macos") {
            header = header.push(menus);
        }
        header = header.push(title);
        if cfg!(target_os = "windows") {
            header = header
                .push(self.control(
                    "—",
                    Message::Window(super::window_chrome::Action::Minimize),
                    true,
                    false,
                ))
                .push(self.control(
                    "□",
                    Message::Window(super::window_chrome::Action::Maximize),
                    true,
                    false,
                ))
                .push(self.control(
                    "×",
                    Message::Command(Command::Navigate(Action::Exit)),
                    true,
                    false,
                ));
        }
        let header: Element<'_, Message> = header.into();
        let page = responsive(move |size| {
            let editor = self
                .editor
                .view(
                    &theme::theme(self.light),
                    self.vm.can_edit(),
                    &self.features.diagrams,
                    &self.features.images,
                )
                .map(Message::Editor);
            let base: Element<'_, Message> = mouse_area(editor)
                .on_right_press_at(Message::ContextAt)
                .into();
            let Some(point) = self.context else {
                let mut layers = stack![base];
                if !self.composing
                    && self.mode == Mode::Rich
                    && self.vm.can_edit()
                    && !self.editor.table_active()
                    && let Some(anchor) = self.editor.selection_anchor()
                {
                    let bar = row![
                        self.icon_control(
                            super::icons::BOLD,
                            "太字  Ctrl+B",
                            Message::Format(norito_editor_widget::Formatting::Bold),
                            true,
                            false
                        ),
                        self.icon_control(
                            super::icons::ITALIC,
                            "斜体  Ctrl+I",
                            Message::Format(norito_editor_widget::Formatting::Italic),
                            true,
                            false
                        ),
                        self.icon_control(
                            super::icons::CODE,
                            "コード  Ctrl+E",
                            Message::Format(norito_editor_widget::Formatting::Code),
                            true,
                            false
                        ),
                        self.icon_control(
                            super::icons::LINK,
                            "ページリンク",
                            Message::Feature(super::features::Event::Resource(false)),
                            true,
                            false
                        ),
                    ]
                    .spacing(2);
                    layers = layers.push(
                        container(opaque(
                            container(bar)
                                .style(move |_| norito_iced_ext::panel(c.surface, c.line)),
                        ))
                        .padding(iced::Padding {
                            left: anchor.x.clamp(0.0, (size.width - 126.0).max(0.0)),
                            top: (if anchor.y >= 40.0 {
                                anchor.y - 40.0
                            } else {
                                anchor.y + 30.0
                            })
                            .clamp(0.0, (size.height - 40.0).max(0.0)),
                            ..Default::default()
                        }),
                    );
                }
                if !self.composing
                    && self.mode == Mode::Rich
                    && self.vm.can_edit()
                    && let Some((cell, remove_row, remove_column)) = self.editor.table_anchor()
                    && cell.y >= 0.0
                    && cell.y + cell.height <= size.height
                {
                    let top = if cell.y >= 42.0 {
                        cell.y - 42.0
                    } else {
                        cell.y + cell.height + 6.0
                    };
                    layers = layers.push(
                        container(opaque(self.table_toolbar(remove_row, remove_column))).padding(
                            iced::Padding {
                                left: cell.x.clamp(0.0, (size.width - 228.0).max(0.0)),
                                top,
                                ..Default::default()
                            },
                        ),
                    );
                }
                return layers.into();
            };
            let selected = !self.editor.selected_text().is_empty();
            let mut items = vec![
                (
                    "元に戻す",
                    "Ctrl+Z",
                    Message::Command(Command::Undo),
                    self.vm.document.can_undo(),
                ),
                (
                    "やり直す",
                    "Ctrl+Shift+Z",
                    Message::Command(Command::Redo),
                    self.vm.document.can_redo(),
                ),
                (
                    "切り取り",
                    "Ctrl+X",
                    Message::EditMenu(EditAction::Cut),
                    selected,
                ),
                (
                    "コピー",
                    "Ctrl+C",
                    Message::EditMenu(EditAction::Copy),
                    selected,
                ),
                (
                    "貼り付け",
                    "Ctrl+V",
                    Message::EditMenu(EditAction::Paste),
                    true,
                ),
                (
                    "すべて選択",
                    "Ctrl+A",
                    Message::EditMenu(EditAction::SelectAll),
                    true,
                ),
                (
                    "太字",
                    "Ctrl+B",
                    Message::Format(norito_editor_widget::Formatting::Bold),
                    self.vm.can_edit(),
                ),
                (
                    "斜体",
                    "Ctrl+I",
                    Message::Format(norito_editor_widget::Formatting::Italic),
                    self.vm.can_edit(),
                ),
                (
                    "コード",
                    "Ctrl+E",
                    Message::Format(norito_editor_widget::Formatting::Code),
                    self.vm.can_edit(),
                ),
                (
                    "表を挿入…",
                    "Ctrl+Shift+T",
                    Message::Feature(super::features::Event::Open(super::features::Kind::Table)),
                    self.vm.can_edit(),
                ),
                (
                    "リンクを開く",
                    "Ctrl+Enter",
                    Message::Editor(norito_editor_widget::EditorEvent::FollowLink),
                    self.vm.can_edit(),
                ),
            ];
            if self.editor.table_active() {
                use norito_editor_widget::{EditorEvent, TableAction as T};
                items = vec![
                    (
                        "下に行を追加",
                        "Ctrl+Enter",
                        Message::Editor(EditorEvent::Table(T::Row(1))),
                        true,
                    ),
                    (
                        "行を削除",
                        "",
                        Message::Editor(EditorEvent::Table(T::Row(-1))),
                        true,
                    ),
                    (
                        "右に列を追加",
                        "",
                        Message::Editor(EditorEvent::Table(T::Column(1))),
                        true,
                    ),
                    (
                        "列を削除",
                        "",
                        Message::Editor(EditorEvent::Table(T::Column(-1))),
                        true,
                    ),
                    (
                        "左揃え",
                        "",
                        Message::Editor(EditorEvent::Table(T::Align(0))),
                        true,
                    ),
                    (
                        "中央揃え",
                        "",
                        Message::Editor(EditorEvent::Table(T::Align(1))),
                        true,
                    ),
                    (
                        "右揃え",
                        "",
                        Message::Editor(EditorEvent::Table(T::Align(2))),
                        true,
                    ),
                ];
            }
            let mut menu = Column::new().spacing(2);
            for (label, key, message, enabled) in items {
                menu = menu.push(
                    button(
                        row![
                            text(label).size(12),
                            space::horizontal(),
                            text(if cfg!(target_os = "macos") {
                                key.replace("Ctrl+", "⌘")
                            } else {
                                key.into()
                            })
                            .size(11)
                            .color(c.muted)
                        ]
                        .align_y(iced::Alignment::Center),
                    )
                    .padding([6, 10])
                    .width(Length::Fill)
                    .on_press_maybe(enabled.then_some(message))
                    .style(move |_, status| theme::control(c, false, status)),
                );
            }
            let menu = container(menu)
                .padding(4)
                .width(240)
                .style(move |_| norito_iced_ext::panel(c.surface, c.line));
            let menu = container(opaque(menu)).padding(iced::Padding {
                left: point.x.clamp(0.0, (size.width - 240.0).max(0.0)),
                top: point.y.clamp(0.0, (size.height - 370.0).max(0.0)),
                ..Default::default()
            });
            stack![
                base,
                mouse_area(container(space()).width(Length::Fill).height(Length::Fill))
                    .on_press(Message::Context(false))
                    .on_right_press(Message::Context(false)),
                menu
            ]
            .into()
        });
        let page = container(page)
            .max_width(720)
            .width(Length::Fill)
            .height(Length::Fill);
        let page = container(page)
            .padding(iced::Padding {
                top: 28.0,
                right: 40.0,
                bottom: 24.0,
                left: 40.0,
            })
            .center_x(Length::Fill)
            .height(Length::Fill);
        let mut body = row![].height(Length::Fill);
        if self.sidebar {
            let mut sidebar = column![].spacing(8);
            if let Some(root) = &self.vm.root {
                sidebar = sidebar
                    .push(
                        row![
                            button(text(&root.name).size(12).color(c.muted))
                                .padding([4, 8])
                                .on_press(Message::Command(Command::ToggleDirectory(root.clone())))
                                .style(move |_, status| theme::control(c, false, status)),
                            space::horizontal(),
                            self.icon_control(
                                super::icons::FILE_ADD,
                                "新規ファイル  Ctrl+N",
                                Message::Command(Command::Navigate(Action::New)),
                                ready && self.vm.creation.is_none(),
                                false
                            ),
                            self.icon_control(
                                super::icons::FOLDER_ADD,
                                "新規フォルダ  Ctrl+Shift+N",
                                Message::Command(Command::NewDirectory),
                                ready && self.vm.creation.is_none(),
                                false
                            )
                        ]
                        .align_y(iced::Alignment::Center),
                    )
                    .push(scrollable(self.tree(root.handle, 0)).height(Length::Fill));
            } else {
                sidebar = sidebar
                    .push(
                        container(text("フォルダ未選択").size(12).color(c.muted)).padding([4, 10]),
                    )
                    .push(self.control(
                        "フォルダを開く",
                        Message::Command(Command::OpenFolder),
                        ready,
                        false,
                    ));
            }
            body = body.push(
                container(sidebar)
                    .padding([16, 8])
                    .width(200)
                    .height(Length::Fill)
                    .style(move |_| container::Style {
                        background: Some(c.surface.into()),
                        ..Default::default()
                    }),
            );
        }
        body = body.push(page);
        if self.features.outline {
            body = body.push(self.outline_view());
        }
        let body = stack![
            body,
            container(self.icon_control(
                icons::OUTLINE,
                "目次",
                Message::Feature(features::Event::ToggleOutline),
                true,
                self.features.outline,
            ))
            .align_right(Length::Fill)
            .padding(iced::Padding {
                top: 4.0,
                right: 2.0,
                ..Default::default()
            })
        ];
        let status = if self.vm.error || self.vm.busy() {
            self.vm.status.as_str()
        } else {
            ""
        };
        let footer = row![
            self.icon_control(
                icons::CODE,
                "表示モードを切り替え  Ctrl+/",
                Message::ToggleMode,
                !self.composing,
                self.mode == Mode::Source
            ),
            text(status)
                .size(12)
                .color(if self.vm.error { c.accent } else { c.muted }),
            space::horizontal(),
            text(format!("{} 文字", self.editor.character_count()))
                .size(11)
                .color(c.muted)
        ]
        .spacing(16)
        .align_y(iced::Alignment::Center);
        let base: Element<'_, Message> = column![
            header,
            self.search_view(),
            body,
            container(footer).padding([8, 24])
        ]
        .height(Length::Fill)
        .into();
        let base = if cfg!(target_os = "windows") {
            super::window_chrome::resize_frame(base)
        } else {
            base
        };
        let base = if let Some(index) = self.open_menu {
            let actions = [
                MenuAction::FILE,
                MenuAction::FORMAT,
                MenuAction::INSERT,
                MenuAction::VIEW,
            ][index];
            let mut items = column![].spacing(2);
            for action in actions {
                let mut label = row![text(action.to_string()).size(12)].width(Length::Fill);
                if *action == MenuAction::DisplayMode {
                    label = label.push(space::horizontal()).push(text("›").size(12));
                }
                items = items.push(
                    mouse_area(
                        button(label)
                            .width(Length::Fill)
                            .padding([7, 12])
                            .on_press(Message::Menu(*action))
                            .style(move |_, status| theme::control(c, false, status)),
                    )
                    .on_enter(Message::ModeMenu(*action == MenuAction::DisplayMode)),
                );
            }
            let dismiss = container(
                mouse_area(opaque(
                    container(space()).width(Length::Fill).height(Length::Fill),
                ))
                .on_press(Message::MenuClose)
                .on_right_press(Message::MenuClose),
            )
            .padding(iced::Padding {
                top: 36.0,
                ..Default::default()
            });
            let mut panels = row![
                container(items)
                    .width(240)
                    .padding(4)
                    .style(move |_| norito_iced_ext::panel(c.surface, c.line))
            ];
            if index == 3 && self.mode_menu {
                let mut modes = column![].spacing(2);
                for action in MenuAction::MODES {
                    let selected = matches!(
                        (action, self.mode),
                        (MenuAction::Rich, Mode::Rich) | (MenuAction::Source, Mode::Source)
                    );
                    modes = modes.push(
                        button(text(action.to_string()).size(12))
                            .width(Length::Fill)
                            .padding([7, 12])
                            .on_press(Message::Menu(*action))
                            .style(move |_, status| theme::control(c, selected, status)),
                    );
                }
                panels = panels.push(
                    container(modes)
                        .width(160)
                        .padding(4)
                        .style(move |_| norito_iced_ext::panel(c.surface, c.line)),
                );
            }
            stack![
                base,
                dismiss,
                container(opaque(panels)).padding(iced::Padding {
                    top: 36.0,
                    left: 64.0
                        + if index == 0 {
                            0.0
                        } else {
                            76.0 + (index - 1) as f32 * 52.0
                        },
                    ..Default::default()
                })
            ]
            .into()
        } else {
            base
        };
        if self.vm.pending.is_some() {
            let dialog = container(
                column![
                    text("変更を保存しますか？").size(20),
                    text(self.vm.name()).size(14).color(c.muted),
                    text(if self.vm.error {
                        self.vm.status.as_str()
                    } else {
                        ""
                    })
                    .size(13)
                    .color(c.accent),
                    row![
                        self.control(
                            "キャンセル",
                            Message::Command(Command::Cancel),
                            ready,
                            false
                        ),
                        self.control(
                            "保存しない",
                            Message::Command(Command::ConfirmDiscard),
                            ready,
                            false
                        ),
                        self.control("保存", Message::Command(Command::ConfirmSave), ready, true)
                    ]
                    .spacing(12)
                ]
                .spacing(20),
            )
            .padding(28)
            .style(move |_| norito_iced_ext::panel(c.surface, c.line));
            stack![
                base,
                opaque(
                    container(dialog)
                        .center_x(Length::Fill)
                        .center_y(Length::Fill)
                        .style(|_| container::Style {
                            background: Some(iced::Color::from_rgba(0.05, 0.04, 0.03, 0.8).into()),
                            ..Default::default()
                        })
                )
            ]
            .into()
        } else {
            base
        }
    }
}
use super::{features, icons};
