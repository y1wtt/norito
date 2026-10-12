use super::*;
use iced::widget::{button, column, container, mouse_area, row, text, text_input};
use norito_model::structure::{Structure, table_markdown};

#[derive(Debug, Clone)]
pub enum TableAction {
    Focus(usize, usize, usize),
    Cell(usize, usize, String),
    Move(bool),
    Row(i32),
    Column(i32),
    Align(u8),
    Create(usize, usize),
    Hover(usize, usize),
}
pub(super) struct TableEdit {
    range: Range<usize>,
    cells: Vec<Vec<String>>,
    alignment: Vec<String>,
    row: usize,
    col: usize,
}
impl TableEdit {
    pub(super) fn markdown(&self) -> String {
        let mut result = table_markdown(&self.cells);
        let start = result.find('\n').unwrap() + 1;
        let end = start + result[start..].find('\n').unwrap();
        result.replace_range(start..end, &format!("| {} |", self.alignment.join(" | ")));
        result
    }
}
impl MarkdownEditor {
    pub(super) fn validate_table(&mut self, source: &str) {
        if self
            .table_edit
            .as_ref()
            .is_some_and(|t| source.get(t.range.clone()) != Some(t.markdown().as_str()))
        {
            self.table_edit = None;
        }
    }
    pub fn table_active(&self) -> bool {
        self.table_edit.is_some()
    }
    /// Selected cell bounds in editor coordinates, and whether deletion is available.
    pub fn table_anchor(&self) -> Option<(iced::Rectangle, bool, bool)> {
        let edit = self.table_edit.as_ref()?;
        let cell = self
            .projection
            .cells
            .iter()
            .find(|c| c.source.start == edit.range.start && c.style.table)?;
        let line = self.projection.text[..cell.display.start]
            .bytes()
            .filter(|b| *b == b'\n')
            .count();
        let mut bounds = self.content.line_bounds(line)?;
        bounds.width /= edit.alignment.len() as f32;
        bounds.x += bounds.width * edit.col as f32;
        bounds.y += 34.0 * edit.row as f32 + norito_iced_ext::EDITOR_PADDING.top;
        bounds.height = 34.0;
        Some((
            bounds,
            edit.row > 0 && edit.cells.len() > 2,
            edit.alignment.len() > 1,
        ))
    }
    pub fn table_focus_id(&self) -> Option<String> {
        self.table_edit
            .as_ref()
            .map(|t| format!("table-{}-{}", t.row, t.col))
    }
    pub fn open_table(&mut self) {
        let selection = self.source_selection();
        if let Some(table) = Structure::parse(&self.projection.source)
            .tables
            .iter()
            .find(|t| t.range.contains(&selection.start))
        {
            self.table_action(TableAction::Focus(table.range.start, 0, 0));
        } else {
            self.table_picker = Some((selection, 2, 2));
        }
    }
    pub fn table_action(&mut self, action: TableAction) -> Option<String> {
        match action {
            TableAction::Hover(r, c) => {
                if let Some((_, rows, cols)) = &mut self.table_picker {
                    *rows = r;
                    *cols = c;
                }
                return None;
            }
            TableAction::Focus(offset, row, col) => {
                self.table_picker = None;
                if let Some(edit) = &mut self.table_edit
                    && edit.range.start == offset
                {
                    edit.row = row.min(edit.cells.len() - 1);
                    edit.col = col.min(edit.alignment.len() - 1);
                    return None;
                }
                let table = Structure::parse(&self.projection.source)
                    .tables
                    .into_iter()
                    .find(|t| t.range.start == offset)?;
                let alignment = self.projection.source[table.range.clone()]
                    .lines()
                    .nth(1)
                    .unwrap_or("")
                    .trim()
                    .trim_matches('|')
                    .split('|')
                    .map(|s| s.trim().to_owned())
                    .collect::<Vec<_>>();
                let mut alignment = alignment;
                alignment.resize(table.cells[0].len(), "---".into());
                alignment.truncate(table.cells[0].len());
                self.active = None;
                self.projection = Projection::editable(&self.projection.source, None);
                self.reset_input(offset..offset);
                self.table_edit = Some(TableEdit {
                    row: row.min(table.cells.len() - 1),
                    col: col.min(alignment.len() - 1),
                    range: table.range,
                    cells: table.cells,
                    alignment,
                });
                return None;
            }
            TableAction::Create(rows, cols) => {
                let (range, _, _) = self.table_picker.take()?;
                let markdown = table_markdown(&vec![
                    vec![String::new(); cols.clamp(1, 8)];
                    rows.clamp(2, 12)
                ]);
                let source = self.insert_markdown(range, &markdown, true);
                let end = self.pending_selection.as_ref()?.end;
                let offset = end - markdown.len();
                self.pending_selection = None;
                self.active = None;
                self.projection = Projection::editable(&source, None);
                self.reset_input(offset..offset);
                self.table_action(TableAction::Focus(offset, 0, 0));
                return Some(source);
            }
            _ => {}
        }
        let edit = self.table_edit.as_mut()?;
        let columns = edit.alignment.len();
        match action {
            TableAction::Cell(row, col, value) => {
                *edit.cells.get_mut(row)?.get_mut(col)? = value;
                edit.row = row;
                edit.col = col;
            }
            TableAction::Move(back) => {
                let at = edit.row * columns + edit.col;
                let next = if back { at.saturating_sub(1) } else { at + 1 };
                edit.row = next / columns;
                edit.col = next % columns;
                if edit.row == edit.cells.len() {
                    edit.cells.push(vec![String::new(); columns]);
                } else {
                    return None;
                }
            }
            TableAction::Row(delta) => {
                if delta > 0 {
                    edit.row += 1;
                    edit.cells.insert(edit.row, vec![String::new(); columns]);
                } else if edit.row > 0 && edit.cells.len() > 2 {
                    edit.cells.remove(edit.row);
                    edit.row = edit.row.min(edit.cells.len() - 1);
                } else {
                    return None;
                }
            }
            TableAction::Column(delta) => {
                if delta > 0 {
                    edit.col += 1;
                    edit.alignment.insert(edit.col, "---".into());
                    for row in &mut edit.cells {
                        row.insert(edit.col, String::new());
                    }
                } else if columns > 1 {
                    edit.alignment.remove(edit.col);
                    for row in &mut edit.cells {
                        row.remove(edit.col);
                    }
                    edit.col = edit.col.min(columns - 2);
                } else {
                    return None;
                }
            }
            TableAction::Align(n) => {
                edit.alignment[edit.col] = match n {
                    1 => ":---:",
                    2 => "---:",
                    _ => ":---",
                }
                .into()
            }
            _ => return None,
        }
        let markdown = edit.markdown();
        let mut source = self.projection.source.clone();
        source.replace_range(edit.range.clone(), &markdown);
        edit.range.end = edit.range.start + markdown.len();
        self.pending_selection = None;
        self.active = None;
        Some(source)
    }
    pub(super) fn table_inline(
        &self,
    ) -> Option<(usize, Element<'_, EditorEvent>, Vec<Vec<EditorEvent>>)> {
        if let Some((range, rows, cols)) = &self.table_picker {
            let at = self.projection.display_position(range.start);
            let line = self.projection.text[..at]
                .bytes()
                .filter(|b| *b == b'\n')
                .count();
            let mut grid = column![text(format!("{} 列 × {} 行", cols, rows)).size(12)].spacing(3);
            for r in 1..=6 {
                let mut cells = row![].spacing(3);
                for c in 1..=6 {
                    let selected = r <= *rows && c <= *cols;
                    cells = cells.push(
                        mouse_area(
                            button(text(" "))
                                .width(24)
                                .height(24)
                                .padding(0)
                                .style(move |theme: &Theme, _| iced::widget::button::Style {
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
            return Some((
                line,
                container(grid)
                    .width(184)
                    .padding(12)
                    .style(|theme: &Theme| {
                        norito_iced_ext::panel(
                            theme.palette().background,
                            theme.extended_palette().background.strong.color,
                        )
                    })
                    .into(),
                vec![],
            ));
        }
        let edit = self.table_edit.as_ref()?;
        let cell = self
            .projection
            .cells
            .iter()
            .find(|c| c.source.start == edit.range.start && c.style.table)?;
        let line = self.projection.text[..cell.display.start]
            .bytes()
            .filter(|b| *b == b'\n')
            .count();
        let mut grid = column![].spacing(0);
        for (r, values) in edit.cells.iter().enumerate() {
            let mut cells = row![].spacing(0);
            for (c, value) in values.iter().enumerate() {
                let offset = edit.range.start;
                let align = &edit.alignment[c];
                let input = text_input("", value)
                    .align_x(if align.ends_with(':') {
                        if align.starts_with(':') {
                            iced::alignment::Horizontal::Center
                        } else {
                            iced::alignment::Horizontal::Right
                        }
                    } else {
                        iced::alignment::Horizontal::Left
                    })
                    .id(format!("table-{r}-{c}"))
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
                    mouse_area(container(input).height(34).width(Length::Fill))
                        .on_press(EditorEvent::Table(TableAction::Focus(offset, r, c))),
                );
            }
            grid = grid.push(cells);
        }
        Some((
            line,
            container(grid)
                .width(Length::Fill)
                .style(|theme: &Theme| container::Style {
                    background: Some(theme.palette().background.into()),
                    ..Default::default()
                })
                .into(),
            edit.cells
                .iter()
                .enumerate()
                .map(|(r, cells)| {
                    cells
                        .iter()
                        .enumerate()
                        .map(|(c, _)| {
                            EditorEvent::Table(TableAction::Focus(edit.range.start, r, c))
                        })
                        .collect()
                })
                .collect(),
        ))
    }
}
