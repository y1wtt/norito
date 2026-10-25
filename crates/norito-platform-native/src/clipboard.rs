use clipboard_win::{Clipboard, raw};
use norito_platform::*;
mod html;

pub struct NativeClipboard;
fn failure(error: impl std::fmt::Display) -> PlatformError {
    PlatformError {
        kind: ErrorKind::Io,
        message: format!("クリップボード: {error}"),
    }
}
fn format(name: &str) -> Result<u32> {
    raw::register_format(name)
        .map(u32::from)
        .ok_or_else(|| failure("形式を登録できません。"))
}
impl ClipboardStore for NativeClipboard {
    fn write_fragment(&self, value: &ClipboardFragment) -> Result<()> {
        let md = format("Norito.Markdown")?;
        let html = format("HTML Format")?;
        let _clipboard = Clipboard::new_attempts(10).map_err(failure)?;
        raw::set_string(&value.text).map_err(failure)?;
        raw::set_without_clear(md, value.markdown.as_bytes()).map_err(failure)?;
        raw::set_without_clear(html, &self::html::encode(&value.html)).map_err(failure)
    }
    fn read_fragment(&self) -> Result<Option<String>> {
        let md = format("Norito.Markdown")?;
        let _clipboard = Clipboard::new_attempts(10).map_err(failure)?;
        let mut data = Vec::new();
        if raw::is_format_avail(md) {
            raw::get_vec(md, &mut data).map_err(failure)?;
        } else {
            let html = format("HTML Format")?;
            if raw::is_format_avail(html)
                && raw::get_vec(html, &mut data).is_ok()
                && let Some(html) = self::html::decode(&data)
            {
                let markdown = norito_model::html_import::markdown(html.trim_end_matches('\0'));
                if !markdown.is_empty() {
                    return Ok(Some(markdown));
                }
            }
            data.clear();
            if raw::is_format_avail(clipboard_win::formats::CF_UNICODETEXT) {
                raw::get_string(&mut data).map_err(failure)?;
            } else {
                return Ok(None);
            }
        }
        String::from_utf8(data)
            .map(|s| Some(s.trim_end_matches('\0').to_owned()))
            .map_err(failure)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Opt-in because this uses and replaces the real Windows clipboard.
    #[test]
    #[ignore = "writes the system clipboard; run explicitly during desktop QA"]
    fn windows_clipboard_formats_and_external_plain_text_round_trip() {
        let store = NativeClipboard;
        let fragment = ClipboardFragment {
            text: "日本語 👩‍💻\r\n本文".into(),
            markdown: "**日本語 👩‍💻**\r\n本文".into(),
            html: "<p><strong>日本語 👩‍💻</strong><br>本文</p>".into(),
        };
        store.write_fragment(&fragment).unwrap();
        assert_eq!(
            store.read_fragment().unwrap().as_deref(),
            Some(fragment.markdown.as_str())
        );
        {
            let _clipboard = Clipboard::new_attempts(10).unwrap();
            let mut text = Vec::new();
            raw::get_string(&mut text).unwrap();
            assert_eq!(String::from_utf8(text).unwrap(), fragment.text);
            let mut html = Vec::new();
            raw::get_html(format("HTML Format").unwrap(), &mut html).unwrap();
            assert_eq!(String::from_utf8(html).unwrap(), fragment.html);
            raw::set_string("external 日本語\r\n**bold**").unwrap();
        }
        assert_eq!(
            store.read_fragment().unwrap().as_deref(),
            Some("external 日本語\r\n**bold**")
        );
        {
            let _clipboard = Clipboard::new_attempts(10).unwrap();
            raw::set_string("plain fallback").unwrap();
            raw::set_html(
                format("HTML Format").unwrap(),
                "<p><strong>外部</strong> <em>斜体</em></p>",
            )
            .unwrap();
        }
        assert_eq!(
            store.read_fragment().unwrap().as_deref(),
            Some("**外部** *斜体*")
        );
        // Exercise large all-selection exports through the actual OS formats.
        // The old writer only appeared correct for small payloads.
        for size in [1000, 10000, 37954, 100000] {
            let large = ClipboardFragment {
                text: "本文 👩‍💻\r\n".repeat(size),
                markdown: "**本文 👩‍💻**\r\n".repeat(size),
                html: format!("<p>{}</p>", "<strong>本文 👩‍💻</strong>".repeat(size)),
            };
            store.write_fragment(&large).unwrap();
            assert_eq!(
                store.read_fragment().unwrap().as_deref(),
                Some(large.markdown.as_str())
            );
            let _clipboard = Clipboard::new_attempts(10).unwrap();
            let mut plain = Vec::new();
            raw::get_string(&mut plain).unwrap();
            assert_eq!(plain, large.text.as_bytes());
            let mut fragment = Vec::new();
            raw::get_html(format("HTML Format").unwrap(), &mut fragment).unwrap();
            assert_eq!(fragment, large.html.as_bytes());
            let mut wire = Vec::new();
            raw::get_vec(format("HTML Format").unwrap(), &mut wire).unwrap();
            assert_eq!(html::decode(&wire), Some(large.html.as_str()));
        }
        {
            let _clipboard = Clipboard::new_attempts(10).unwrap();
            raw::set_string("malformed HTML fallback").unwrap();
            raw::set_without_clear(format("HTML Format").unwrap(), b"Version:0.9\r\nStartFragment:0000038141\r\nEndFragment:0000038095\r\n<html>bad</html>\0").unwrap();
        }
        assert_eq!(
            store.read_fragment().unwrap().as_deref(),
            Some("malformed HTML fallback")
        );
    }
}
