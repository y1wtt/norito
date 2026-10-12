//! GUI-independent document state. Source text is the canonical representation.
pub mod projection;
pub mod structure;
/// Shared parser extensions keep navigation and editable projection consistent.
pub fn markdown_options() -> pulldown_cmark::Options {
    use pulldown_cmark::Options as O;
    O::ENABLE_TABLES
        | O::ENABLE_MATH
        | O::ENABLE_FOOTNOTES
        | O::ENABLE_TASKLISTS
        | O::ENABLE_STRIKETHROUGH
        | O::ENABLE_YAML_STYLE_METADATA_BLOCKS
        | O::ENABLE_GFM
}

pub fn decode_fragment(fragment: &str) -> String {
    let mut decoded = Vec::new();
    let bytes = fragment.as_bytes();
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'%'
            && let Some(pair) = bytes.get(at + 1..at + 3)
            && let Ok(hex) = std::str::from_utf8(pair)
            && let Ok(value) = u8::from_str_radix(hex, 16)
        {
            decoded.push(value);
            at += 3;
        } else {
            decoded.push(bytes[at]);
            at += 1;
        }
    }
    String::from_utf8(decoded).unwrap_or_else(|_| fragment.into())
}
#[derive(Debug, Clone)]
pub struct Document {
    pub source: String,
    pub revision: u64,
    saved: String,
    undo: Vec<String>,
    redo: Vec<String>,
    bom: bool,
}

impl Document {
    pub fn new(source: String) -> Self {
        Self {
            saved: source.clone(),
            source,
            revision: 0,
            undo: vec![],
            redo: vec![],
            bom: false,
        }
    }
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        let bom = bytes.starts_with(&[0xef, 0xbb, 0xbf]);
        let source = String::from_utf8(bytes[if bom { 3 } else { 0 }..].to_vec())
            .map_err(|_| "UTF-8のMarkdownファイルを選択してください。".to_string())?;
        Ok(Self {
            bom,
            ..Self::new(source)
        })
    }
    pub fn bytes(&self) -> Vec<u8> {
        let mut bytes = if self.bom {
            vec![0xef, 0xbb, 0xbf]
        } else {
            vec![]
        };
        bytes.extend_from_slice(self.source.as_bytes());
        bytes
    }
    pub fn edit(&mut self, source: String) {
        if source != self.source {
            self.undo.push(std::mem::replace(&mut self.source, source));
            // Bounded full snapshots for this prototype; transactions will replace these.
            if self.undo.len() > 100 {
                self.undo.remove(0);
            }
            self.redo.clear();
            self.revision += 1;
        }
    }
    pub fn undo(&mut self) {
        if let Some(source) = self.undo.pop() {
            self.redo.push(std::mem::replace(&mut self.source, source));
            self.revision += 1;
        }
    }
    pub fn redo(&mut self) {
        if let Some(source) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.source, source));
            self.revision += 1;
        }
    }
    pub fn dirty(&self) -> bool {
        self.source != self.saved
    }
    pub fn mark_saved(&mut self, snapshot: String) {
        self.saved = snapshot;
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_bom_and_mixed_line_endings() {
        let input = "\u{feff}# 茶\r\n\n**原文**  \r\n".as_bytes();
        assert_eq!(Document::from_bytes(input).unwrap().bytes(), input);
        assert!(Document::from_bytes(&[0xff]).is_err());
    }
    #[test]
    fn save_snapshot_does_not_mark_later_edits_clean() {
        let mut d = Document::new("a".into());
        d.edit("b".into());
        let saving = d.source.clone();
        d.edit("c".into());
        d.mark_saved(saving);
        assert!(d.dirty());
        d.undo();
        assert!(!d.dirty());
        d.redo();
        assert_eq!(d.source, "c");
    }
}
