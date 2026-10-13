//! Reusable GUI extension points. No application or Markdown dependencies.
//! Built on Iced's public API only.
pub mod editor_decorations;
pub mod events;
mod ink;
pub mod inline_editor;
pub mod rich_editor;
pub mod styled_text;
use iced::{Background, Border, Color, widget::container};

/// Convert design-token OKLCH into the sRGB color used by Iced.
pub fn oklch(l: f32, c: f32, hue: f32) -> Color {
    let a = c * hue.to_radians().cos();
    let b = c * hue.to_radians().sin();
    let ll = (l + 0.39633778 * a + 0.21580376 * b).powi(3);
    let mm = (l - 0.105561346 * a - 0.06385417 * b).powi(3);
    let ss = (l - 0.08948418 * a - 1.2914855 * b).powi(3);
    fn gamma(v: f32) -> f32 {
        (if v <= 0.0031308 {
            12.92 * v
        } else {
            1.055 * v.powf(1.0 / 2.4) - 0.055
        })
        .clamp(0.0, 1.0)
    }
    Color::from_rgb(
        gamma(4.0767417 * ll - 3.3077116 * mm + 0.23096994 * ss),
        gamma(-1.268438 * ll + 2.6097574 * mm - 0.3413194 * ss),
        gamma(-0.0041960863 * ll - 0.7034186 * mm + 1.7076147 * ss),
    )
}

pub fn panel(background: Color, border: Color) -> container::Style {
    container::Style {
        background: Some(Background::Color(background)),
        border: Border {
            color: border,
            width: 1.0,
            radius: 0.0.into(),
        },
        ..Default::default()
    }
}
pub const EDITOR_PADDING: iced::Padding = iced::Padding {
    top: 8.0,
    right: 16.0,
    bottom: 8.0,
    left: 16.0,
};
