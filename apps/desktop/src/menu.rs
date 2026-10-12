//! Menu items, what they do, and the in-app menu bar state.
use crate::{Message, features};
use norito_editor_widget::{Formatting, Mode};
use norito_viewmodel::{Action, Command};
use std::fmt;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    Table,
    CodeBlock,
    Mermaid,
    Image,
    PageLink,
    Outline,
    NewDirectory,
    New,
    Open,
    Folder,
    Save,
    SaveAs,
    Refresh,
    Bold,
    Italic,
    Code,
    Paragraph,
    H1,
    H2,
    H3,
    Bullet,
    Quote,
    Strike,
    Find,
    DisplayMode,
    Rich,
    Source,
    Sidebar,
    Theme,
}
impl MenuAction {
    pub const INSERT: &'static [Self] = &[
        Self::Table,
        Self::CodeBlock,
        Self::Mermaid,
        Self::Image,
        Self::PageLink,
    ];
    pub const FILE: &'static [Self] = &[
        Self::New,
        Self::NewDirectory,
        Self::Open,
        Self::Folder,
        Self::Save,
        Self::SaveAs,
        Self::Refresh,
    ];
    pub const FORMAT: &'static [Self] = &[
        Self::Paragraph,
        Self::H1,
        Self::H2,
        Self::H3,
        Self::Bold,
        Self::Italic,
        Self::Code,
        Self::Bullet,
        Self::Quote,
        Self::Strike,
    ];
    pub const VIEW: &'static [Self] = &[
        Self::DisplayMode,
        Self::Outline,
        Self::Sidebar,
        Self::Theme,
        Self::Find,
    ];
    pub const MODES: &'static [Self] = &[Self::Rich, Self::Source];
}
impl fmt::Display for MenuAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Table => "表…  Ctrl+Shift+T",
            Self::CodeBlock => "コードブロック…",
            Self::Mermaid => "Mermaid…",
            Self::Image => "画像…",
            Self::PageLink => "ページ間リンク…",
            Self::Outline => "目次・文書ナビゲーション",
            Self::New => "新規",
            Self::NewDirectory => "新規フォルダ  Ctrl+Shift+N",
            Self::Open => "ファイルを開く…",
            Self::Folder => "フォルダを開く…",
            Self::Save => "保存",
            Self::SaveAs => "名前を付けて保存…",
            Self::Refresh => "フォルダを更新",
            Self::Bold => "太字",
            Self::Italic => "斜体",
            Self::Code => "インラインコード",
            Self::Paragraph => "標準の段落",
            Self::H1 => "見出し1",
            Self::H2 => "見出し2",
            Self::H3 => "見出し3",
            Self::Bullet => "箇条書き",
            Self::Quote => "引用  Ctrl+Shift+Q",
            Self::Strike => "取り消し線  Ctrl+Shift+X",
            Self::Find => "検索・置換…  Ctrl+F",
            Self::Rich => "WYSIWYG",
            Self::Source => "コード",
            Self::DisplayMode => "表示モード",
            Self::Sidebar => "サイドバーを切り替え",
            Self::Theme => "明色 / 暗色を切り替え",
        })
    }
}

impl MenuAction {
    /// What choosing this item does. `DisplayMode` only opens its submenu.
    pub fn message(self) -> Option<Message> {
        use MenuAction as M;
        use features::{Event as F, Kind};
        let command = |c| Some(Message::Command(c));
        let format = |f| Some(Message::Format(f));
        match self {
            M::DisplayMode => None,
            M::Table => Some(Message::Feature(F::Open(Kind::Table))),
            M::CodeBlock => Some(Message::Feature(F::Open(Kind::Code))),
            M::Mermaid => Some(Message::Feature(F::Open(Kind::Mermaid))),
            M::Image => Some(Message::Feature(F::Resource(true))),
            M::PageLink => Some(Message::Feature(F::Resource(false))),
            M::Outline => Some(Message::Feature(F::ToggleOutline)),
            M::Find => Some(Message::Feature(F::Find)),
            M::New => command(Command::Navigate(Action::New)),
            M::NewDirectory => command(Command::NewDirectory),
            M::Open => command(Command::Navigate(Action::Open)),
            M::Folder => command(Command::OpenFolder),
            M::Save => command(Command::Save(false)),
            M::SaveAs => command(Command::Save(true)),
            M::Refresh => command(Command::Refresh),
            M::Rich => Some(Message::Mode(Mode::Rich)),
            M::Source => Some(Message::Mode(Mode::Source)),
            M::Theme => Some(Message::Theme),
            M::Sidebar => Some(Message::Sidebar),
            M::Bold => format(Formatting::Bold),
            M::Italic => format(Formatting::Italic),
            M::Code => format(Formatting::Code),
            M::Paragraph => format(Formatting::Heading(0)),
            M::H1 => format(Formatting::Heading(1)),
            M::H2 => format(Formatting::Heading(2)),
            M::H3 => format(Formatting::Heading(3)),
            M::Bullet => format(Formatting::Bullet),
            M::Quote => format(Formatting::Quote),
            M::Strike => format(Formatting::Strike),
        }
    }
}

/// In-app menu bar: which top-level menu is open, and its display-mode submenu.
#[derive(Debug, Default)]
pub struct MenuBar {
    pub open: Option<usize>,
    pub modes: bool,
}

impl MenuBar {
    pub const MENUS: [(&'static str, &'static [MenuAction]); 4] = [
        ("ファイル", MenuAction::FILE),
        ("書式", MenuAction::FORMAT),
        ("挿入", MenuAction::INSERT),
        ("表示", MenuAction::VIEW),
    ];
    pub fn toggle(&mut self, index: usize) {
        self.modes = false;
        self.open = if self.open == Some(index) {
            None
        } else {
            Some(index)
        };
    }
    /// Once a menu is open, hovering another title switches to it.
    pub fn hover(&mut self, index: usize) {
        if self.open != Some(index) {
            self.modes = false;
        }
        if self.open.is_some() {
            self.open = Some(index);
        }
    }
    pub fn close(&mut self) {
        self.open = None;
        self.modes = false;
    }
}
