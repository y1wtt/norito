//! The inline table grid and the size picker, placed over a collapsed line.
use super::{Picker, ROW_HEIGHT, TableAction, TableEdit};
use crate::{EditorEvent, MarkdownEditor, view::focus_events};
use iced::{
    Element, Length, Theme,
    alignment::Horizontal,
    widget::{button, column, container, mouse_area, row, text, text_input},
};

/// Widget id of a cell input, for focusing.
pub(super) fn cell_id(row: usize, col: usize) -> String {
    format!("table-{row}-{col}")
}

fn alignment(delimiter: &str) -> Horizontal {
    match (delimiter.starts_with(':'), delimiter.ends_with(':')) {
        (true, true) => Horizontal::Center,
        (false, true) => Horizontal::Right,
        _ => Horizontal::Left,
    }
}

/// Display line, element, and the click event of each covered cell.
type Inline<'a> = (usize, Element<'a, EditorEvent>, Vec<Vec<EditorEvent>>);

impl MarkdownEditor {
    pub(crate) fn table_inline(&self) -> Option<Inline<'_>> {
        if let Some(picker) = &self.table_picker {
            return Some((self.picker_line(picker), picker_view(picker), vec![]));
        }
        let edit = self.table_edit.as_ref()?;
        let line = self.table_line(edit.range.start)?;
        Some((
            line,
            grid_view(edit),
            focus_events(edit.range.start, &edit.cells),
        ))
    }
}

fn picker_view<'a>(picker: &Picker) -> Element<'a, EditorEvent> {
    let mut grid =
        column![text(format!("{} 列 × {} 行", picker.cols, picker.rows)).size(12)].spacing(3);
    for r in 1..=6 {
        let mut cells = row![].spacing(3);
        for c in 1..=6 {
            let selected = r <= picker.rows && c <= picker.cols;
            cells = cells.push(
                mouse_area(
                    button(text(" "))
                        .width(24)
                        .height(24)
                        .padding(0)
                        .style(move |theme: &Theme, _| button::Style {
                            background: Some(
                                if selected {
                                    theme.palette().primary.scale_alpha(0.45)
                                } else {
                                    theme.extended_palette().background.weak.color
                                }
                                .into(),
                            ),
                            border: iced::Border {
                                width: 1.0,
                                color: theme.extended_palette().background.strong.color,
                                ..Default::default()
                            },
                            ..Default::default()
                        })
                        .on_press(EditorEvent::Table(TableAction::Create(r.max(2), c))),
                )
                .on_enter(EditorEvent::Table(TableAction::Hover(r.max(2), c))),
            );
        }
        grid = grid.push(cells);
    }
    container(grid)
        .width(184)
        .padding(12)
        .style(|theme: &Theme| {
            norito_iced_ext::panel(
                theme.palette().background,
                theme.extended_palette().background.strong.color,
            )
        })
        .into()
}

fn grid_view<'a>(edit: &TableEdit) -> Element<'a, EditorEvent> {
    let offset = edit.range.start;
    let mut grid = column![].spacing(0);
    for (r, values) in edit.cells.iter().enumerate() {
        let mut cells = row![].spacing(0);
        for (c, value) in values.iter().enumerate() {
            let input = text_input("", value)
                .align_x(alignment(&edit.alignment[c]))
                .id(cell_id(r, c))
                .size(14)
                .padding([7, 7])
                .on_input(move |v| EditorEvent::Table(TableAction::Cell(r, c, v)))
                .style(move |theme: &Theme, status| {
                    let mut style = text_input::default(theme, status);
                    style.value = theme.palette().text;
                    style.background = if r == 0 {
                        theme.extended_palette().background.weak.color
                    } else {
                        theme.palette().background
                    }
                    .into();
                    style.border.radius = 0.0.into();
                    style.border.width = 1.0;
                    style
                });
            cells = cells.push(
                mouse_area(container(input).height(ROW_HEIGHT).width(Length::Fill))
                    .on_press(EditorEvent::Table(TableAction::Focus(offset, r, c))),
            );
        }
        grid = grid.push(cells);
    }
    container(grid)
        .width(Length::Fill)
        .style(|theme: &Theme| container::Style {
            background: Some(theme.palette().background.into()),
            ..Default::default()
        })
        .into()
}
