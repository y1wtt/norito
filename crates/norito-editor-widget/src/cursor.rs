//! Byte offsets <-> editor line/column positions.
use iced::advanced::text::editor::Position;

pub(crate) fn offset(text: &str, position: Position) -> usize {
    let start: usize = text
        .split_inclusive('\n')
        .take(position.line)
        .map(str::len)
        .sum();
    (start + position.column).min(text.len())
}
pub(crate) fn position(text: &str, mut offset: usize) -> Position {
    offset = offset.min(text.len());
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    let prefix = &text[..offset];
    Position {
        line: prefix.bytes().filter(|b| *b == b'\n').count(),
        column: prefix.rfind('\n').map_or(offset, |n| offset - n - 1),
    }
}
