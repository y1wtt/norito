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
