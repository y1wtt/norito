//! Continuous WYSIWYG editing, projected onto the loss-preserving Markdown model.
//!
//! The widget owns no document. It shows a `Projection` of the source it is
//! given and turns input into replacement sources; the caller applies them
//! through the view model and hands the document back with `synchronize`.
//!
//! - `input`: selection, typing, and keeping the text editor in step with the projection.
//! - `autopair`: generated closing delimiters.
//! - `commands`: formatting and insertion.
//! - `hit`: screen-space queries (click targets, selection anchor).
//! - `style`: typography as per-line highlight spans, cached.
//! - `view`: the text editor with block decorations.
//! - `keymap`: editor key bindings.
//! - `tables`: inline table editing and the insertion picker.
//! - `syntax`, `mermaid`: code-block colors.
mod autopair;
mod commands;
mod cursor;
mod hit;
mod input;
mod keymap;
mod mermaid;
mod style;
mod syntax;
mod tables;
#[cfg(test)]
mod tests;
mod view;

use iced::widget::{image, svg, text_editor};
use norito_iced_ext::rich_editor;
use norito_model::{Document, projection::Projection, structure::Diagram, structure::Structure};
use std::{borrow::Cow, cell::RefCell, collections::HashMap, ops::Range, sync::Arc};
pub use tables::TableAction;

#[derive(Debug, Clone)]
pub enum EditorEvent {
    Action(text_editor::Action),
    Format(Formatting),
    Undo,
    Redo,
    ContextMenu,
    FollowLink,
    EditCode(usize),
    InsertTable,
    EditTable(usize),
    Table(TableAction),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Rich,
    Source,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClickTarget {
    Link { source: usize, destination: String },
    Task { source: usize },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Formatting {
    Bold,
    Italic,
    Code,
    Heading(u8),
    Bullet,
    Quote,
    Strike,
}
/// Rendered media shown in place of collapsed blocks. Missing entries render as pending.
#[derive(Default)]
pub struct Media {
    pub diagrams: HashMap<Diagram, Result<svg::Handle, String>>,
    pub images: HashMap<String, Result<image::Handle, String>>,
}

pub struct MarkdownEditor {
    content: rich_editor::Content,
    projection: Projection,
    revision: u64,
    mode: Mode,
    /// Source selection to restore after the caller applies the returned edit.
    pending_selection: Option<Range<usize>>,
    /// Source offset whose construct shows its delimiters.
    active: Option<usize>,
    auto_closers: autopair::AutoClosers,
    table_edit: Option<tables::TableEdit>,
    table_picker: Option<tables::Picker>,
    syntax_cache: syntax::Cache,
    code_colors: Vec<Option<iced::Color>>,
    style_cache: RefCell<Option<style::Cache>>,
    structure_cache: RefCell<Option<(String, Arc<Structure>)>>,
}

impl MarkdownEditor {
    pub fn new(document: &Document) -> Self {
        let projection = Projection::editable(&document.source, None);
        let mut syntax_cache = syntax::Cache::default();
        let code_colors = syntax_cache.colors(&document.source);
        Self {
            content: rich_editor::Content::with_text(&projection.text),
            projection,
            revision: document.revision,
            mode: Mode::Rich,
            pending_selection: None,
            active: None,
            auto_closers: Default::default(),
            table_edit: None,
            table_picker: None,
            syntax_cache,
            code_colors,
            style_cache: Default::default(),
            structure_cache: Default::default(),
        }
    }
    /// The text in the editor: the projection, or the source in source mode.
    pub fn displayed(&self) -> &str {
        if self.mode == Mode::Rich {
            &self.projection.text
        } else {
            &self.projection.source
        }
    }
    pub fn character_count(&self) -> usize {
        self.projection.source.chars().count()
    }
    /// The projection matching what is displayed: every byte in source mode.
    fn shown(&self) -> Cow<'_, Projection> {
        if self.mode == Mode::Rich {
            Cow::Borrowed(&self.projection)
        } else {
            Cow::Owned(Projection::raw(&self.projection.source))
        }
    }
    /// Collapsed Mermaid blocks: display line, source offset and the diagram to draw.
    fn diagram_slots(&self) -> Vec<(usize, usize, Diagram)> {
        self.structure()
            .code
            .iter()
            .filter(|code| code.is_mermaid())
            .filter_map(|code| {
                let line = self
                    .projection
                    .block_line(code.range.start, |s| s.diagram)?;
                Some((
                    line,
                    code.range.start,
                    Diagram::Mermaid(code.content.clone()),
                ))
            })
            .collect()
    }
    fn structure(&self) -> Arc<Structure> {
        let mut cache = self.structure_cache.borrow_mut();
        if let Some((source, structure)) = cache.as_ref()
            && source == &self.projection.source
        {
            return structure.clone();
        }
        let structure = Arc::new(Structure::parse(&self.projection.source));
        *cache = Some((self.projection.source.clone(), structure.clone()));
        structure
    }
}
