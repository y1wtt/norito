//! Text helpers with no Markdown knowledge.
use std::ops::Range;

/// A single contiguous change, aligned to UTF-8 boundaries.
pub fn changed_range(old: &str, new: &str) -> (Range<usize>, Range<usize>) {
    let mut prefix = old
        .bytes()
        .zip(new.bytes())
        .take_while(|(a, b)| a == b)
        .count();
    while !old.is_char_boundary(prefix) || !new.is_char_boundary(prefix) {
        prefix -= 1;
    }
    let mut suffix = old[prefix..]
        .bytes()
        .rev()
        .zip(new[prefix..].bytes().rev())
        .take_while(|(a, b)| a == b)
        .count();
    while !old.is_char_boundary(old.len() - suffix) || !new.is_char_boundary(new.len() - suffix) {
        suffix -= 1;
    }
    (prefix..old.len() - suffix, prefix..new.len() - suffix)
}

/// Percent-decode a link fragment. Undecodable input is returned unchanged.
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

/// Number of line breaks before `at`: the zero-based line index of that offset.
pub fn line_index(text: &str, at: usize) -> usize {
    text.as_bytes()[..at]
        .iter()
        .filter(|b| **b == b'\n')
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unicode_diff_and_fragments_are_valid() {
        let (old, new) = changed_range("あいう", "あえう");
        assert_eq!(&"あいう"[old], "い");
        assert_eq!(&"あえう"[new], "え");
        assert_eq!(decode_fragment("%E8%A6%8B%E5%87%BA%E3%81%97"), "見出し");
        assert_eq!(decode_fragment("%FF"), "%FF");
        assert_eq!(line_index("a\nb\nc", 4), 2);
    }
}
