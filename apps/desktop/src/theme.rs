//! Kaga Bocha palette, adapted from the user's supplied design-system archive.
use iced::{Background, Border, Color, Theme, widget::button};
use norito_iced_ext::oklch;

#[derive(Clone, Copy)]
pub struct Colors {
    pub ground: Color,
    pub surface: Color,
    pub line: Color,
    pub text: Color,
    pub muted: Color,
    pub accent: Color,
    pub success: Color,
}
pub fn colors(light: bool) -> Colors {
    if light {
        Colors {
            ground: oklch(0.975, 0.010, 80.0),
            surface: oklch(0.945, 0.020, 80.0),
            line: oklch(0.865, 0.038, 78.0),
            text: oklch(0.330, 0.060, 45.0),
            muted: oklch(0.470, 0.100, 48.0),
            accent: oklch(0.630, 0.110, 55.0),
            success: oklch(0.630, 0.110, 135.0),
        }
    } else {
        Colors {
            ground: oklch(0.190, 0.014, 55.0),
            surface: oklch(0.245, 0.018, 58.0),
            line: oklch(0.330, 0.024, 60.0),
            text: oklch(0.900, 0.018, 80.0),
            muted: oklch(0.640, 0.030, 70.0),
            accent: oklch(0.720, 0.110, 58.0),
            success: oklch(0.740, 0.110, 135.0),
        }
    }
}
pub fn theme(light: bool) -> Theme {
    let c = colors(light);
    Theme::custom_with_fn(
        if light {
            "Kaga Bocha Light"
        } else {
            "Kaga Bocha Dark"
        },
        iced::theme::Palette {
            background: c.ground,
            text: c.text,
            primary: c.accent,
            success: c.success,
            warning: c.accent,
            danger: c.accent,
        },
        move |palette| {
            use iced::theme::palette::{Extended, Pair};
            let mut extended = Extended::generate(palette);
            extended.background.base = Pair {
                color: c.ground,
                text: c.text,
            };
            extended.background.weak = Pair {
                color: c.surface,
                text: c.text,
            };
            extended.background.weakest = extended.background.weak;
            extended.background.weaker = extended.background.weak;
            extended.background.strong = Pair {
                color: c.line,
                text: c.text,
            };
            extended.background.neutral = extended.background.strong;
            extended.background.stronger = extended.background.strong;
            extended.background.strongest = Pair {
                color: c.line,
                text: if light {
                    oklch(0.225, 0.018, 60.0)
                } else {
                    oklch(0.955, 0.016, 82.0)
                },
            };
            extended
        },
    )
}
pub fn control(c: Colors, selected: bool, status: button::Status) -> button::Style {
    let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
    button::Style {
        background: (selected || hovered).then_some(Background::Color(c.line)),
        text_color: if matches!(status, button::Status::Disabled) {
            c.muted
        } else if selected {
            c.accent
        } else {
            c.text
        },
        border: Border {
            width: 0.0,
            color: c.line,
            radius: 4.0.into(),
        },
        ..Default::default()
    }
}
