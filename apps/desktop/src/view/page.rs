//! The document page: the editor, plus the selection format bar, the table
//! toolbar and the context menu floating over it.
use super::controls::panel;
use crate::{App, EditAction, Message, features, icons, theme};
use iced::{
    Element, Length, Padding, Size,
    widget::{Column, button, container, mouse_area, opaque, responsive, row, space, stack, text},
};
use norito_editor_widget::{EditorEvent, Formatting, Mode, TableAction as T};
use norito_viewmodel::Command;

const FORMAT_BAR_WIDTH: f32 = 126.0;
const TABLE_BAR_WIDTH: f32 = 228.0;
const MENU_WIDTH: f32 = 240.0;
const MENU_HEIGHT: f32 = 370.0;

/// Padding that places a `width` x `height` overlay at (`x`, `y`), kept inside `size`.
fn place(size: Size, x: f32, y: f32, width: f32, height: f32) -> Padding {
    Padding {
        left: x.clamp(0.0, (size.width - width).max(0.0)),
        top: y.clamp(0.0, (size.height - height).max(0.0)),
        ..Default::default()
    }
}
fn table(action: T) -> Message {
    Message::Editor(EditorEvent::Table(action))
}

impl App {
    pub(super) fn page_view(&self) -> Element<'_, Message> {
        let page = responsive(move |size| {
            let editor = self
                .editor
                .view(
                    &theme::theme(self.light),
                    self.vm.can_edit(),
                    &self.features.media,
                )
                .map(Message::Editor);
            let base: Element<'_, Message> = mouse_area(editor)
                .on_right_press_at(Message::ContextAt)
                .into();
            match self.context {
                Some(point) => self.context_menu(base, size, point),
                None => self.toolbars(base, size),
            }
        });
        let page = container(page)
            .max_width(720)
            .width(Length::Fill)
            .height(Length::Fill);
        container(page)
            .padding(Padding {
                top: 28.0,
                right: 40.0,
                bottom: 24.0,
                left: 40.0,
            })
            .center_x(Length::Fill)
            .height(Length::Fill)
            .into()
    }
    /// The format bar above a selection and the toolbar above the edited table cell.
    fn toolbars<'a>(&'a self, base: Element<'a, Message>, size: Size) -> Element<'a, Message> {
        let c = theme::colors(self.light);
        let mut layers = stack![base];
        let shown = !self.composing && self.mode == Mode::Rich && self.vm.can_edit();
        if shown
            && !self.editor.table_active()
            && let Some(anchor) = self.editor.selection_anchor()
        {
            let control =
                |icon, label, message| self.icon_control(icon, label, message, true, false);
            let bar = row![
                control(
                    icons::BOLD,
                    "太字  Ctrl+B",
                    Message::Format(Formatting::Bold)
                ),
                control(
                    icons::ITALIC,
                    "斜体  Ctrl+I",
                    Message::Format(Formatting::Italic)
                ),
                control(
                    icons::CODE,
                    "コード  Ctrl+E",
                    Message::Format(Formatting::Code)
                ),
                control(
                    icons::LINK,
                    "ページリンク",
                    Message::Feature(features::Event::Resource(false))
                ),
            ]
            .spacing(2);
            let y = if anchor.y >= 40.0 {
                anchor.y - 40.0
            } else {
                anchor.y + 30.0
            };
            layers = layers.push(
                container(opaque(container(bar).style(panel(c)))).padding(place(
                    size,
                    anchor.x,
                    y,
                    FORMAT_BAR_WIDTH,
                    40.0,
                )),
            );
        }
        if shown
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
                container(opaque(self.table_toolbar(remove_row, remove_column))).padding(Padding {
                    top,
                    ..place(size, cell.x, 0.0, TABLE_BAR_WIDTH, 0.0)
                }),
            );
        }
        layers.into()
    }
    fn table_toolbar(&self, remove_row: bool, remove_column: bool) -> Element<'_, Message> {
        let c = theme::colors(self.light);
        let actions = [
            (
                "下に行を追加",
                "M3 3h14v8H3z M3 7h14 M10 13v6 M7 16h6",
                T::Row(1),
                true,
            ),
            (
                "行を削除",
                "M3 3h14v8H3z M3 7h14 M7 16h6",
                T::Row(-1),
                remove_row,
            ),
            (
                "右に列を追加",
                "M2 3h8v14H2z M6 3v14 M12 10h6 M15 7v6",
                T::Column(1),
                true,
            ),
            (
                "列を削除",
                "M2 3h8v14H2z M6 3v14 M12 10h6",
                T::Column(-1),
                remove_column,
            ),
            (
                "左揃え",
                "M3 4h14 M3 8h9 M3 12h14 M3 16h9",
                T::Align(0),
                true,
            ),
            (
                "中央揃え",
                "M3 4h14 M6 8h8 M3 12h14 M6 16h8",
                T::Align(1),
                true,
            ),
            (
                "右揃え",
                "M3 4h14 M8 8h9 M3 12h14 M8 16h9",
                T::Align(2),
                true,
            ),
        ];
        let mut bar = row![].spacing(2);
        for (label, path, action, enabled) in actions {
            bar = bar.push(self.icon_control(path, label, table(action), enabled, false));
        }
        container(bar).padding(3).style(panel(c)).into()
    }
    /// Edit commands at `point`, or table commands while a table is edited.
    fn context_menu<'a>(
        &'a self,
        base: Element<'a, Message>,
        size: Size,
        point: iced::Point,
    ) -> Element<'a, Message> {
        let c = theme::colors(self.light);
        let editable = self.vm.can_edit();
        let selected = !self.editor.selected_text().is_empty();
        let document = &self.vm.session.document;
        let items = if self.editor.table_active() {
            vec![
                ("下に行を追加", "Ctrl+Enter", table(T::Row(1)), true),
                ("行を削除", "", table(T::Row(-1)), true),
                ("右に列を追加", "", table(T::Column(1)), true),
                ("列を削除", "", table(T::Column(-1)), true),
                ("左揃え", "", table(T::Align(0)), true),
                ("中央揃え", "", table(T::Align(1)), true),
                ("右揃え", "", table(T::Align(2)), true),
            ]
        } else {
            vec![
                (
                    "元に戻す",
                    "Ctrl+Z",
                    Message::Command(Command::Undo),
                    document.can_undo(),
                ),
                (
                    "やり直す",
                    "Ctrl+Shift+Z",
                    Message::Command(Command::Redo),
                    document.can_redo(),
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
                    Message::Format(Formatting::Bold),
                    editable,
                ),
                (
                    "斜体",
                    "Ctrl+I",
                    Message::Format(Formatting::Italic),
                    editable,
                ),
                (
                    "コード",
                    "Ctrl+E",
                    Message::Format(Formatting::Code),
                    editable,
                ),
                (
                    "表を挿入…",
                    "Ctrl+Shift+T",
                    Message::Feature(features::Event::Open(features::Kind::Table)),
                    editable,
                ),
                (
                    "リンクを開く",
                    "Ctrl+Enter",
                    Message::Editor(EditorEvent::FollowLink),
                    editable,
                ),
            ]
        };
        let mut menu = Column::new().spacing(2);
        for (label, key, message, enabled) in items {
            let key = if cfg!(target_os = "macos") {
                key.replace("Ctrl+", "⌘")
            } else {
                key.into()
            };
            menu = menu.push(
                button(
                    row![
                        text(label).size(12),
                        space::horizontal(),
                        text(key).size(11).color(c.muted)
                    ]
                    .align_y(iced::Alignment::Center),
                )
                .padding([6, 10])
                .width(Length::Fill)
                .on_press_maybe(enabled.then_some(message))
                .style(move |_, status| theme::control(c, false, status)),
            );
        }
        let menu = container(menu).padding(4).width(MENU_WIDTH).style(panel(c));
        stack![
            base,
            mouse_area(container(space()).width(Length::Fill).height(Length::Fill))
                .on_press(Message::Context(false))
                .on_right_press(Message::Context(false)),
            container(opaque(menu)).padding(place(size, point.x, point.y, MENU_WIDTH, MENU_HEIGHT))
        ]
        .into()
    }
}
