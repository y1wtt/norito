//! Clipboard export runs on a worker; resource paths remain behind the OS port.
use base64::{Engine, engine::general_purpose::STANDARD};
use norito_platform::{ClipboardFragment, DocumentResources, FileHandle};
use std::collections::HashMap;

pub fn fragment(
    markdown: String,
    file: Option<FileHandle>,
    resources: &dyn DocumentResources,
) -> ClipboardFragment {
    let structure = norito_model::structure::Structure::parse(&markdown);
    let mut images = HashMap::new();
    if let Some(file) = file {
        for link in structure.links.iter().filter(|l| l.image) {
            if let Ok(bytes) = resources.image_bytes(file, &link.destination)
                && let Some(mime) = image_mime(&bytes)
            {
                images.insert(link.destination.clone(), data_uri(mime, &bytes));
            }
        }
    }
    let mut diagrams = HashMap::new();
    for diagram in structure.diagrams() {
        if let Ok(rendered) = norito_render::svg(&diagram) {
            diagrams.insert(
                diagram,
                norito_model::clipboard::EmbeddedFigure {
                    url: data_uri("image/svg+xml", rendered.svg.as_bytes()),
                    extent: rendered.extent,
                },
            );
        }
    }
    ClipboardFragment {
        text: markdown.clone(),
        html: norito_model::clipboard::html_with_assets(&markdown, &images, &diagrams),
        markdown,
    }
}
fn data_uri(mime: &str, bytes: &[u8]) -> String {
    format!("data:{mime};base64,{}", STANDARD.encode(bytes))
}
fn image_mime(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        Some("image/jpeg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        Some("image/webp")
    } else if is_svg(bytes) {
        Some("image/svg+xml")
    } else {
        None
    }
}

pub fn is_svg(bytes: &[u8]) -> bool {
    std::str::from_utf8(bytes).is_ok_and(|s| {
        s.trim_start().starts_with("<svg")
            || (s.trim_start().starts_with("<?xml") && s.contains("<svg"))
    })
}

pub fn embedded_image(url: &str) -> Option<norito_platform::Result<Vec<u8>>> {
    if !url.starts_with("data:image/") {
        return None;
    }
    Some(
        (|| {
            let (header, value) = url.split_once(',').ok_or("画像データが不正です。")?;
            if !header.ends_with(";base64") || value.len() > 32 * 1024 * 1024 {
                return Err("画像データの形式またはサイズが不正です。");
            }
            let bytes = STANDARD
                .decode(value)
                .map_err(|_| "画像データが不正です。")?;
            image_mime(&bytes).ok_or("画像形式が未対応です。")?;
            Ok(bytes)
        })()
        .map_err(|message| {
            norito_platform::PlatformError::new(norito_platform::ErrorKind::Unsupported, message)
        }),
    )
}

#[test]
fn pasted_embedded_images_work_without_a_saved_document() {
    let bytes = b"\x89PNG\r\n\x1a\nfixture";
    assert_eq!(
        embedded_image(&data_uri("image/png", bytes))
            .unwrap()
            .unwrap(),
        bytes
    );
    let svg = b"<svg xmlns=\"http://www.w3.org/2000/svg\"><path d=\"M0 0\"/></svg>";
    assert_eq!(
        embedded_image(&data_uri("image/svg+xml", svg))
            .unwrap()
            .unwrap(),
        svg
    );
    assert!(
        embedded_image("data:image/png;base64,invalid")
            .unwrap()
            .is_err()
    );
    assert!(embedded_image("assets/image.png").is_none());
}

#[test]
fn copied_figures_are_embedded_and_markdown_stays_editable() {
    let resources = norito_platform_native::NativePlatform::default();
    let source = "```mermaid\nflowchart LR\nA-->B\n```\n\n$x^2$";
    let result = fragment(source.into(), None, &resources);
    assert_eq!(result.markdown, source);
    assert_eq!(result.html.matches("data:image/svg+xml;base64,").count(), 2);
    assert!(!result.html.contains("language-mermaid"));
    assert!(result.html.contains("vertical-align:-"));
}

#[test]
fn copied_relative_image_contains_its_bytes_and_failure_preserves_source() {
    use norito_platform::*;
    struct Images;
    impl DocumentResources for Images {
        fn image_bytes(&self, _: FileHandle, destination: &str) -> Result<Vec<u8>> {
            if destination == "assets/image.png" {
                Ok(b"\x89PNG\r\n\x1a\nfixture".to_vec())
            } else {
                Err(PlatformError::new(ErrorKind::Io, "missing"))
            }
        }
        fn import_attachment(&self, _: FileHandle, _: std::path::PathBuf) -> Result<ResourceLink> {
            unreachable!()
        }
        fn resolve_page(&self, _: FileHandle, _: &str) -> Result<FileRef> {
            unreachable!()
        }
    }
    let source =
        "![画像](assets/image.png) ![missing](missing.png)\n\n```mermaid\ninvalid diagram\n```";
    let result = fragment(source.into(), Some(FileHandle(1)), &Images);
    assert_eq!(result.markdown, source);
    assert!(result.html.contains("data:image/png;base64,iVBORw0KGg"));
    assert!(!result.html.contains("assets/image.png"));
    assert!(result.html.contains("missing.png"));
    assert!(result.html.contains("invalid diagram"));
}
