//! One document: canonical source, its on-disk encoding, saved snapshot and history.
use crate::history::History;

const BOM: [u8; 3] = [0xef, 0xbb, 0xbf];

#[derive(Debug, Clone)]
pub struct Document {
    pub source: String,
    pub revision: u64,
    saved: String,
    history: History<String>,
    bom: bool,
}

impl Document {
    pub fn new(source: String) -> Self {
        Self {
            saved: source.clone(),
            source,
            revision: 0,
            history: History::default(),
            bom: false,
        }
    }
    /// Only UTF-8 (optionally with BOM) is accepted; invalid bytes are never replaced.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        let bom = bytes.starts_with(&BOM);
        let source = String::from_utf8(bytes[if bom { BOM.len() } else { 0 }..].to_vec())
            .map_err(|_| "UTF-8のMarkdownファイルを選択してください。".to_string())?;
        Ok(Self {
            bom,
            ..Self::new(source)
        })
    }
    pub fn bytes(&self) -> Vec<u8> {
        let mut bytes = if self.bom { BOM.to_vec() } else { vec![] };
        bytes.extend_from_slice(self.source.as_bytes());
        bytes
    }
    pub fn edit(&mut self, source: String) {
        if source != self.source {
            self.history
                .record(std::mem::replace(&mut self.source, source));
            self.revision += 1;
        }
    }
    pub fn undo(&mut self) {
        if self.history.undo(&mut self.source) {
            self.revision += 1;
        }
    }
    pub fn redo(&mut self) {
        if self.history.redo(&mut self.source) {
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
        self.history.can_undo()
    }
    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
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
