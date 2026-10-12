//! Inline table editing: a grid model over one table's source range, and the
//! size picker for inserting a new table. `view` draws both.
mod view;
use crate::MarkdownEditor;
use norito_model::{compose, structure::Structure, text::line_index};
use std::ops::Range;

/// Height of one rendered table row.
pub(crate) const ROW_HEIGHT: f32 = 34.0;

#[derive(Debug, Clone)]
pub enum TableAction {
    /// Table offset, row, column.
    Focus(usize, usize, usize),
    Cell(usize, usize, String),
    /// Next cell, or previous when true. Past the last cell adds a row.
    Move(bool),
    Row(i32),
    Column(i32),
    /// 0 left, 1 center, 2 right.
    Align(u8),
    Create(usize, usize),
    Hover(usize, usize),
}

/// The size picker shown where a new table will go.
pub(crate) struct Picker {
    range: Range<usize>,
    rows: usize,
    cols: usize,
}

/// One table being edited. `range` tracks its source as edits change the length.
pub(crate) struct TableEdit {
    range: Range<usize>,
    cells: Vec<Vec<String>>,
    /// Delimiter row cells, kept verbatim (`---`, `:---:`, ...).
    alignment: Vec<String>,
    row: usize,
    col: usize,
}

impl TableEdit {
    fn markdown(&self) -> String {
        compose::table(&self.cells, &self.alignment)
    }
    fn columns(&self) -> usize {
        self.alignment.len()
    }
    /// Apply a grid action. False when only the focus moved, or nothing happened.
    fn apply(&mut self, action: TableAction) -> bool {
        let columns = self.columns();
        match action {
            TableAction::Cell(row, col, value) => {
                let Some(cell) = self.cells.get_mut(row).and_then(|r| r.get_mut(col)) else {
                    return false;
                };
                *cell = value;
                self.row = row;
                self.col = col;
            }
            TableAction::Move(back) => {
                let at = self.row * columns + self.col;
                let next = if back { at.saturating_sub(1) } else { at + 1 };
                self.row = next / columns;
                self.col = next % columns;
                if self.row < self.cells.len() {
                    return false;
                }
                self.cells.push(vec![String::new(); columns]);
            }
            TableAction::Row(delta) => {
                if delta > 0 {
                    self.row += 1;
                    self.cells.insert(self.row, vec![String::new(); columns]);
                } else if self.can_remove_row() {
                    self.cells.remove(self.row);
                    self.row = self.row.min(self.cells.len() - 1);
                } else {
                    return false;
                }
            }
            TableAction::Column(delta) => {
                if delta > 0 {
                    self.col += 1;
                    self.alignment.insert(self.col, "---".into());
                    for row in &mut self.cells {
                        row.insert(self.col, String::new());
                    }
                } else if self.can_remove_column() {
                    self.alignment.remove(self.col);
                    for row in &mut self.cells {
                        row.remove(self.col);
                    }
                    self.col = self.col.min(columns - 2);
                } else {
                    return false;
                }
            }
            TableAction::Align(n) => {
                self.alignment[self.col] = match n {
                    1 => ":---:",
                    2 => "---:",
                    _ => ":---",
                }
                .into()
            }
            TableAction::Focus(..) | TableAction::Create(..) | TableAction::Hover(..) => {
                return false;
            }
        }
        true
    }
    /// The header row stays, and so does at least one body row.
    fn can_remove_row(&self) -> bool {
        self.row > 0 && self.cells.len() > 2
    }
    fn can_remove_column(&self) -> bool {
        self.columns() > 1
    }
}

impl MarkdownEditor {
    pub(crate) fn close_table(&mut self) {
        self.table_edit = None;
        self.table_picker = None;
    }
    /// Drop the table editor when its table no longer matches the source.
    pub(crate) fn validate_table(&mut self, source: &str) {
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
    /// Display line of the collapsed table at `source`.
    fn table_line(&self, source: usize) -> Option<usize> {
        self.projection.block_line(source, |s| s.table)
    }
    /// Selected cell bounds in editor coordinates, and whether row / column deletion is available.
    pub fn table_anchor(&self) -> Option<(iced::Rectangle, bool, bool)> {
        let edit = self.table_edit.as_ref()?;
        let mut bounds = self
            .content
            .line_bounds(self.table_line(edit.range.start)?)?;
        bounds.width /= edit.columns() as f32;
        bounds.x += bounds.width * edit.col as f32;
        bounds.y += ROW_HEIGHT * edit.row as f32 + norito_iced_ext::EDITOR_PADDING.top;
        bounds.height = ROW_HEIGHT;
        Some((bounds, edit.can_remove_row(), edit.can_remove_column()))
    }
    pub fn table_focus_id(&self) -> Option<String> {
        self.table_edit
            .as_ref()
            .map(|t| view::cell_id(t.row, t.col))
    }
    /// Edit the table at the caret, or offer the size picker to insert one.
    pub fn open_table(&mut self) {
        let selection = self.source_selection();
        if let Some(table) = Structure::parse(&self.projection.source)
            .tables
            .iter()
            .find(|t| t.range.contains(&selection.start))
        {
            self.table_action(TableAction::Focus(table.range.start, 0, 0));
        } else {
            self.table_picker = Some(Picker {
                range: selection,
                rows: 2,
                cols: 2,
            });
        }
    }
    /// Returns the new source when the action changed the table.
    pub fn table_action(&mut self, action: TableAction) -> Option<String> {
        match action {
            TableAction::Hover(rows, cols) => {
                if let Some(picker) = &mut self.table_picker {
                    picker.rows = rows;
                    picker.cols = cols;
                }
                None
            }
            TableAction::Focus(offset, row, col) => {
                self.focus_table(offset, row, col);
                None
            }
            TableAction::Create(rows, cols) => {
                let picker = self.table_picker.take()?;
                let cells = vec![vec![String::new(); cols.clamp(1, 8)]; rows.clamp(2, 12)];
                let markdown = compose::table(&cells, &[]);
                let source = self.insert_markdown(picker.range, &markdown, true);
                let offset = self.pending_selection.take()?.end - markdown.len();
                self.projection = norito_model::projection::Projection::editable(&source, None);
                self.active = None;
                self.reset_input(offset..offset);
                self.focus_table(offset, 0, 0);
                Some(source)
            }
            action => {
                let edit = self.table_edit.as_mut()?;
                if !edit.apply(action) {
                    return None;
                }
                let markdown = edit.markdown();
                let mut source = self.projection.source.clone();
                source.replace_range(edit.range.clone(), &markdown);
                edit.range.end = edit.range.start + markdown.len();
                self.pending_selection = None;
                self.active = None;
                Some(source)
            }
        }
    }
    fn focus_table(&mut self, offset: usize, row: usize, col: usize) {
        self.table_picker = None;
        if let Some(edit) = &mut self.table_edit
            && edit.range.start == offset
        {
            edit.row = row.min(edit.cells.len() - 1);
            edit.col = col.min(edit.columns() - 1);
            return;
        }
        let Some(table) = Structure::parse(&self.projection.source)
            .tables
            .into_iter()
            .find(|t| t.range.start == offset)
        else {
            return;
        };
        let columns = table.cells[0].len();
        let mut alignment: Vec<_> = self.projection.source[table.range.clone()]
            .lines()
            .nth(1)
            .unwrap_or("")
            .trim()
            .trim_matches('|')
            .split('|')
            .map(|s| s.trim().to_owned())
            .collect();
        alignment.resize(columns, "---".into());
        self.reveal(None);
        self.reset_input(offset..offset);
        self.table_edit = Some(TableEdit {
            row: row.min(table.cells.len() - 1),
            col: col.min(columns - 1),
            range: table.range,
            cells: table.cells,
            alignment,
        });
    }
    /// Display line of the picker anchor.
    fn picker_line(&self, picker: &Picker) -> usize {
        let at = self.projection.display_position(picker.range.start);
        line_index(&self.projection.text, at)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn edit() -> TableEdit {
        TableEdit {
            range: 0..0,
            cells: vec![vec!["a".into(), "b".into()], vec!["c".into(), "d".into()]],
            alignment: vec![":---".into(), "---:".into()],
            row: 0,
            col: 0,
        }
    }
    #[test]
    fn grid_actions_keep_header_and_alignment() {
        let mut t = edit();
        assert!(!t.apply(TableAction::Move(false)));
        assert_eq!((t.row, t.col), (0, 1));
        t.row = 1;
        assert!(t.apply(TableAction::Move(false)));
        assert_eq!((t.row, t.col, t.cells.len()), (2, 0, 3));
        assert!(t.apply(TableAction::Row(-1)));
        assert!(!t.apply(TableAction::Row(-1)));
        assert!(t.apply(TableAction::Column(1)));
        assert_eq!(t.alignment, [":---", "---", "---:"]);
        assert!(t.markdown().contains("| :--- | --- | ---: |"));
    }
}
