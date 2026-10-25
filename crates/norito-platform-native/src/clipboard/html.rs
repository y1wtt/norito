//! Windows CF_HTML uses UTF-8 byte offsets, not character counts.
const PREFIX: &str = "<html>\r\n<body>\r\n<!--StartFragment-->";
const SUFFIX: &str = "<!--EndFragment-->\r\n</body>\r\n</html>";

pub(super) fn encode(fragment: &str) -> Vec<u8> {
    fn header(start: usize, end: usize, fragment_start: usize, fragment_end: usize) -> String {
        format!(
            "Version:0.9\r\nStartHTML:{start:010}\r\nEndHTML:{end:010}\r\nStartFragment:{fragment_start:010}\r\nEndFragment:{fragment_end:010}\r\n"
        )
    }
    let start = header(0, 0, 0, 0).len();
    let fragment_start = start + PREFIX.len();
    let fragment_end = fragment_start + fragment.len();
    let end = fragment_end + SUFFIX.len();
    let mut data = header(start, end, fragment_start, fragment_end).into_bytes();
    data.extend_from_slice(PREFIX.as_bytes());
    data.extend_from_slice(fragment.as_bytes());
    data.extend_from_slice(SUFFIX.as_bytes());
    data.push(0);
    data
}

pub(super) fn decode(data: &[u8]) -> Option<&str> {
    let text = std::str::from_utf8(data).ok()?.trim_end_matches('\0');
    // Only parse the transport header, never metadata-looking text in the body.
    let header_end = text.find('<')?;
    let header = text.get(..header_end)?;
    let offset = |name: &str| -> Option<usize> {
        header.lines().find_map(|line| {
            let (key, value) = line.split_once(':')?;
            (key == name).then(|| value.trim().parse().ok()).flatten()
        })
    };
    let start = offset("StartFragment")?;
    let end = offset("EndFragment")?;
    if start < header_end || start > end {
        return None;
    }
    text.get(start..end)
}

#[cfg(test)]
mod tests {
    use super::*;
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/components.rs"
    ));

    fn assert_wire_format(fragment: &str) {
        let data = encode(fragment);
        let text = std::str::from_utf8(&data).unwrap();
        let fields: Vec<usize> = text
            .lines()
            .skip(1)
            .take(4)
            .map(|line| line.split_once(':').unwrap().1.parse().unwrap())
            .collect();
        let [start, end, first, last] = fields[..] else {
            panic!("missing offsets")
        };
        // Independent checks against the actual serialized markers and bytes.
        assert_eq!(start, text.find("<html>").unwrap());
        assert_eq!(
            first,
            text.find("<!--StartFragment-->").unwrap() + "<!--StartFragment-->".len()
        );
        assert_eq!(last, text.rfind("<!--EndFragment-->").unwrap());
        assert_eq!(end, data.len() - 1);
        assert!(start <= first && first <= last && last <= end);
        assert_eq!(&data[first..last], fragment.as_bytes());
        assert_eq!(data[end], 0);
        assert_eq!(decode(&data), Some(fragment));
    }

    #[test]
    fn all_components_and_whole_document_have_valid_clipboard_offsets() {
        for (_, markdown) in COMPONENTS {
            assert_wire_format(&norito_model::clipboard::html_fragment(markdown));
        }
        let whole = COMPONENTS
            .iter()
            .map(|(_, source)| *source)
            .collect::<Vec<_>>()
            .join("\n\n");
        assert_wire_format(&norito_model::clipboard::html_fragment(&whole));
    }

    #[test]
    fn offsets_handle_empty_unicode_and_decimal_boundaries() {
        for size in [0, 1, 9, 99, 999, 1000, 9999, 10000, 37954, 99999, 100000] {
            assert_wire_format(&"x".repeat(size));
            assert_wire_format(&format!(
                "<p>日本語 👩‍💻 e\u{301}\r\n{}</p>",
                "あ".repeat(size)
            ));
        }
        assert_wire_format(&format!(
            "<img src=\"data:image/svg+xml;base64,{}\">",
            "ABCD".repeat(10000)
        ));
    }

    #[test]
    fn invalid_offsets_never_expose_header_or_panic() {
        let valid = String::from_utf8(encode("<p>日本語</p>")).unwrap();
        for value in ["0000000000", "9999999999", "0000000145", "-000000001"] {
            let broken = valid.replace(
                "StartFragment:0000000141",
                &format!("StartFragment:{value}"),
            );
            assert_eq!(decode(broken.as_bytes()), None);
        }
        assert_eq!(decode(b"Version:0.9\r\nStartFragment:0000038141\r\nEndFragment:0000038095\r\n<html>bad</html>"), None);
        assert_eq!(decode(b"<p>unframed</p>"), None);
        assert_eq!(decode(&[255]), None);
    }
}
