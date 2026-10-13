//! Small shared controls: outline icons, icon buttons with tooltips, text buttons.
use crate::{App, Message, theme};
use iced::{
    Color, Element,
    widget::{Svg, button, container, svg, text, tooltip},
};

pub(crate) fn outline_icon<'a>(path: &str, size: f32, color: Color) -> Svg<'a, iced::Theme> {
    icon(
        path,
        r#"fill="none" stroke="black" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round""#,
        size,
        color,
    )
}

/// A solid shape, such as the unsaved-changes dot.
pub(crate) fn filled_icon<'a>(path: &str, size: f32, color: Color) -> Svg<'a, iced::Theme> {
    icon(path, r#"fill="black""#, size, color)
}

fn icon<'a>(path: &str, paint: &str, size: f32, color: Color) -> Svg<'a, iced::Theme> {
    let source = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20"><path d="{path}" {paint}/></svg>"#
    );
    svg(svg::Handle::from_memory(source.into_bytes()))
        .width(size)
        .height(size)
        .style(move |_, _| svg::Style { color: Some(color) })
}

/// Shortens `name` to about `em` font-size units; full-width characters count as 1.
pub(crate) fn ellipsize(name: &str, em: f32) -> String {
    let width = |c: char| if c.is_ascii() { 0.58 } else { 1.0 };
    if name.chars().map(width).sum::<f32>() <= em {
        return name.into();
    }
    let mut used = width('…');
    let mut short = String::new();
    for c in name.chars() {
        used += width(c);
        if used > em {
            break;
        }
        short.push(c);
    }
    short + "…"
}

/// A themed bordered panel, for menus, tooltips and toolbars.
pub(crate) fn panel(c: theme::Colors) -> impl Fn(&iced::Theme) -> container::Style {
    move |_| norito_iced_ext::panel(c.surface, c.line)
}

impl App {
    pub(crate) fn icon_control<'a>(
        &self,
        path: &str,
        label: &'a str,
        message: Message,
        enabled: bool,
        selected: bool,
    ) -> Element<'a, Message> {
        let c = theme::colors(self.light);
        let color = if !enabled {
            c.muted.scale_alpha(0.45)
        } else if selected {
            c.accent
        } else {
            c.text
        };
        tooltip(
            button(outline_icon(path, 18.0, color))
                .padding(6)
                .width(30)
                .height(30)
                .on_press_maybe(enabled.then_some(message))
                .style(move |_, status| theme::control(c, selected, status)),
            container(text(label).size(11))
                .padding([5, 8])
                .style(panel(c)),
            tooltip::Position::Bottom,
        )
        .into()
    }
    pub(crate) fn control<'a>(
        &self,
        label: &'a str,
        message: Message,
        enabled: bool,
        selected: bool,
    ) -> Element<'a, Message> {
        let c = theme::colors(self.light);
        button(text(label).size(12))
            .padding([8, 12])
            .on_press_maybe(enabled.then_some(message))
            .style(move |_, status| theme::control(c, selected, status))
            .into()
    }
}
