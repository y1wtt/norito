//! Text painted from cosmic-text buffers through `fill_raw`, clipped to a
//! rectangle on every renderer.
use iced::advanced::graphics::text::{self, Raw, cosmic_text};
use iced::{Color, Font, Point, Rectangle, font};
use std::sync::Arc;

/// Declared size of painted buffers. tiny-skia clips raw text to its layer
/// only when the buffer exceeds that layer; partially visible lines and
/// overflowing labels paint outside any tighter size.
pub(crate) const UNBOUNDED: f32 = 1.0e6;

/// Paint each buffer at its position, inside `clip` only. The caller keeps the
/// buffers alive until the frame is presented: renderers hold weak references.
pub(crate) fn fill<'a, R>(
    renderer: &mut R,
    buffers: impl IntoIterator<Item = (&'a Arc<cosmic_text::Buffer>, Point)>,
    color: Color,
    clip: Rectangle,
) where
    R: iced::advanced::Renderer + text::Renderer,
{
    renderer.with_layer(clip, |renderer| {
        for (buffer, position) in buffers {
            renderer.fill_raw(Raw {
                buffer: Arc::downgrade(buffer),
                position,
                color,
                clip_bounds: clip,
            });
        }
    });
}

/// Cosmic-text attributes for `font`. An oblique face is synthesized:
/// families without one, such as Yu Gothic UI, otherwise render upright.
pub(crate) fn attributes(font: Font) -> cosmic_text::Attrs<'static> {
    let attrs = text::to_attributes(font);
    if font.style == font::Style::Oblique {
        attrs
            .style(cosmic_text::Style::Normal)
            .cache_key_flags(cosmic_text::CacheKeyFlags::FAKE_ITALIC)
    } else {
        attrs
    }
}

/// A shaped paragraph, wrapped at `width` when given.
pub(crate) fn label(
    content: &str,
    font: Font,
    size: f32,
    line_height: f32,
    width: Option<f32>,
) -> cosmic_text::Buffer {
    let mut font_system = text::font_system().write().expect("Write font system");
    let font_system = font_system.raw();
    let mut buffer =
        cosmic_text::Buffer::new(font_system, cosmic_text::Metrics::new(size, line_height));
    buffer.set_wrap(
        font_system,
        if width.is_some() {
            cosmic_text::Wrap::WordOrGlyph
        } else {
            cosmic_text::Wrap::None
        },
    );
    buffer.set_size(
        font_system,
        Some(width.unwrap_or(UNBOUNDED)),
        Some(UNBOUNDED),
    );
    buffer.set_text(
        font_system,
        content,
        &attributes(font),
        cosmic_text::Shaping::Advanced,
        None,
    );
    buffer.shape_until_scroll(font_system, false);
    buffer
}

/// The widest laid-out line of `buffer`.
pub(crate) fn width(buffer: &cosmic_text::Buffer) -> f32 {
    buffer
        .layout_runs()
        .map(|run| run.line_w)
        .fold(0.0, f32::max)
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::Size;

    #[test]
    fn aligned_and_overflowing_labels_stay_inside_their_clip() {
        // Cells partially scrolled above the viewport, as in a table decoration.
        let viewport = Rectangle::new(Point::new(0.0, 20.0), Size::new(250.0, 80.0));
        let cells = [
            Rectangle::new(Point::new(30.0, 8.0), Size::new(90.0, 34.0)),
            Rectangle::new(Point::new(120.0, 8.0), Size::new(90.0, 34.0)),
        ];
        let labels = ["Center", "Overflowing cell text"]
            .map(|content| Arc::new(label(content, Font::DEFAULT, 14.0, 19.6, None)));
        let mut snapshots = Vec::new();
        for damage in [
            Rectangle::new(Point::ORIGIN, Size::new(250.0, 100.0)),
            viewport,
        ] {
            let mut renderer = iced_tiny_skia::Renderer::new(Font::DEFAULT, 14.0.into());
            for (cell, label) in cells.iter().zip(&labels) {
                let inner = cell.shrink(7.0);
                let x = if Arc::ptr_eq(label, &labels[0]) {
                    inner.center_x() - width(label) / 2.0
                } else {
                    inner.x
                };
                let clip = cell.intersection(&viewport).unwrap();
                fill(
                    &mut renderer,
                    [(label, Point::new(x, inner.y))],
                    Color::BLACK,
                    clip,
                );
            }
            let mut pixmap = tiny_skia::Pixmap::new(250, 100).unwrap();
            pixmap.fill(tiny_skia::Color::WHITE);
            let mut mask = tiny_skia::Mask::new(250, 100).unwrap();
            renderer.draw(
                &mut pixmap.as_mut(),
                &mut mask,
                &iced::advanced::graphics::Viewport::with_physical_size(Size::new(250, 100), 1.0),
                &[damage],
                Color::WHITE,
            );
            assert!(
                pixmap.pixels().iter().any(|p| p.red() != 255),
                "test must render ink"
            );
            for y in 0..100 {
                for x in 0..250 {
                    let point = Point::new(x as f32 + 0.5, y as f32 + 0.5);
                    if !cells
                        .iter()
                        .any(|cell| cell.contains(point) && viewport.contains(point))
                    {
                        assert_eq!(
                            pixmap.pixel(x, y).unwrap().red(),
                            255,
                            "ink leaked at {x},{y}"
                        );
                    }
                }
            }
            snapshots.push(pixmap.data().to_vec());
        }
        assert_eq!(
            snapshots[0], snapshots[1],
            "damage rectangles must not change the output"
        );
    }

    #[test]
    fn oblique_font_sets_synthetic_slant_for_japanese_glyphs() {
        let attrs = attributes(Font {
            family: font::Family::Name("Yu Gothic UI"),
            style: font::Style::Oblique,
            ..Font::DEFAULT
        });
        assert!(
            attrs
                .cache_key_flags
                .contains(cosmic_text::CacheKeyFlags::FAKE_ITALIC)
        );
        assert_eq!(attrs.style, cosmic_text::Style::Normal);
    }
}
