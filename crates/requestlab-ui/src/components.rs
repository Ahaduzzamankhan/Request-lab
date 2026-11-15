//! Reusable interface primitives.
//!
//! Every helper returns a styled [`Div`] so the caller can attach its own event
//! handlers. Nothing here renders a control that has no behaviour attached.

use gpui::prelude::*;
use gpui::{div, px, CursorStyle, Div, SharedString};

/// Interaction helpers for elements that are not addressable by id.
///
/// GPUI 0.2.2 only exposes `on_click` and `overflow_scroll` on `Stateful`
/// elements (`Div::id`), which would force a synthetic id at every call site.
/// These helpers attach the same handlers and styles directly to the element's
/// interactivity state; the renderer still inserts a hitbox because the click
/// listener list is non-empty.
pub trait InteractiveExt: gpui::InteractiveElement + gpui::Styled + Sized {
    fn on_click(
        self,
        listener: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
    ) -> Self {
        let mut this = self;
        this.interactivity().on_click(listener);
        this
    }

    fn overflow_scroll(mut self) -> Self {
        self.style().overflow.x = Some(gpui::Overflow::Scroll);
        self.style().overflow.y = Some(gpui::Overflow::Scroll);
        self
    }

    fn overflow_y_scroll(mut self) -> Self {
        self.style().overflow.y = Some(gpui::Overflow::Scroll);
        self
    }
}

impl<T: gpui::InteractiveElement + gpui::Styled> InteractiveExt for T {}
use requestlab_models::HttpMethod;

use crate::theme::{radius, space, text, with_alpha, Theme};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonVariant {
    Primary,
    Secondary,
    Ghost,
    Danger,
}

impl ButtonVariant {
    fn colors(self, theme: &Theme) -> (gpui::Rgba, gpui::Rgba, gpui::Rgba) {
        match self {
            ButtonVariant::Primary => (theme.accent, theme.background, theme.accent),
            ButtonVariant::Secondary => (theme.elevated, theme.text_primary, theme.border),
            ButtonVariant::Ghost => (theme.surface, theme.text_secondary, theme.border),
            ButtonVariant::Danger => (theme.error, theme.background, theme.error),
        }
    }
}

/// A standard button.
pub fn button(theme: &Theme, label: impl Into<SharedString>, variant: ButtonVariant) -> Div {
    let (background, foreground, border) = variant.colors(theme);
    let hover = theme.elevated;
    let is_primary = variant == ButtonVariant::Primary;
    div()
        .flex()
        .items_center()
        .justify_center()
        .h(px(27.0))
        .px(space::md())
        .gap(space::sm())
        .rounded(radius::md())
        .bg(background)
        .text_color(foreground)
        .text_size(text::body())
        .font_weight(gpui::FontWeight::MEDIUM)
        .border_1()
        .border_color(border)
        .cursor(CursorStyle::PointingHand)
        .hover(move |style| {
            if is_primary {
                style.opacity(0.9)
            } else {
                style.bg(hover)
            }
        })
        .child(label.into())
}

/// A compact square button for an icon or single glyph.
pub fn icon_button(theme: &Theme, glyph: impl Into<SharedString>) -> Div {
    div()
        .flex()
        .items_center()
        .justify_center()
        .size(px(24.0))
        .rounded(radius::sm())
        .text_color(theme.text_secondary)
        .text_size(text::body())
        .cursor(CursorStyle::PointingHand)
        .hover(move |style| style.bg(theme.hover).text_color(theme.text_primary))
        .child(glyph.into())
}

/// A tab inside a tab bar.
pub fn tab_button(
    theme: &Theme,
    label: impl Into<SharedString>,
    selected: bool,
    count: Option<usize>,
) -> Div {
    let label: SharedString = label.into();
    let color = if selected {
        theme.text_primary
    } else {
        theme.text_secondary
    };
    let mut element = div()
        .flex()
        .items_center()
        .gap(space::xs())
        .px(space::md())
        .py(px(7.0))
        .rounded(radius::sm())
        .text_size(text::body())
        .text_color(color)
        .cursor(CursorStyle::PointingHand)
        .when(selected, |style| style.bg(theme.active))
        .hover(move |style| style.bg(theme.hover))
        .child(label);
    if let Some(count) = count {
        if count > 0 {
            element = element.child(
                div()
                    .px(px(4.0))
                    .rounded(radius::sm())
                    .bg(theme.elevated)
                    .text_size(text::micro())
                    .text_color(theme.text_muted)
                    .child(count.to_string()),
            );
        }
    }
    element
}

/// A small pill of text, used for methods and statuses.
pub fn chip(theme: &Theme, label: impl Into<SharedString>, color: gpui::Rgba, filled: bool) -> Div {
    let background = if filled {
        with_alpha(color, 0.16)
    } else {
        theme.elevated
    };
    div()
        .flex()
        .items_center()
        .justify_center()
        .h(px(18.0))
        .px(px(6.0))
        .rounded(radius::sm())
        .bg(background)
        .text_color(color)
        .text_size(text::micro())
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .child(label.into())
}

pub fn method_chip(theme: &Theme, method: HttpMethod) -> Div {
    let color = theme.method_color(method);
    div()
        .flex()
        .items_center()
        .justify_center()
        .min_w(px(48.0))
        .h(px(20.0))
        .px(px(6.0))
        .rounded(radius::sm())
        .bg(with_alpha(color, 0.16))
        .text_color(color)
        .text_size(text::micro())
        .font_weight(gpui::FontWeight::BOLD)
        .child(method.as_str())
}

pub fn status_chip(theme: &Theme, status: Option<u16>, label: impl Into<SharedString>) -> Div {
    let color = theme.status_color(status);
    div()
        .flex()
        .items_center()
        .gap(px(5.0))
        .h(px(20.0))
        .px(px(7.0))
        .rounded(radius::sm())
        .bg(with_alpha(color, 0.14))
        .text_color(color)
        .text_size(text::micro())
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .child(label.into())
}

pub fn section_heading(theme: &Theme, label: impl Into<SharedString>) -> Div {
    div()
        .px(space::md())
        .py(space::sm())
        .text_size(text::micro())
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .text_color(theme.text_muted)
        .child(label.into())
}

pub fn field_label(theme: &Theme, label: impl Into<SharedString>) -> Div {
    div()
        .text_size(text::small())
        .text_color(theme.text_secondary)
        .child(label.into())
}

pub fn empty_state(
    theme: &Theme,
    title: impl Into<SharedString>,
    detail: impl Into<SharedString>,
) -> Div {
    div()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(space::sm())
        .p(space::xl())
        .w_full()
        .child(
            div()
                .text_size(text::title())
                .text_color(theme.text_secondary)
                .child(title.into()),
        )
        .child(
            div()
                .text_size(text::small())
                .text_color(theme.text_muted)
                .text_center()
                .child(detail.into()),
        )
}

pub fn loading(theme: &Theme, label: impl Into<SharedString>) -> Div {
    div()
        .flex()
        .items_center()
        .gap(space::sm())
        .text_size(text::small())
        .text_color(theme.text_secondary)
        .child(div().size(px(8.0)).rounded(px(4.0)).bg(theme.accent))
        .child(label.into())
}

pub fn divider(theme: &Theme) -> Div {
    div().h(px(1.0)).w_full().bg(theme.border)
}

pub fn vertical_divider(theme: &Theme) -> Div {
    div().w(px(1.0)).h_full().flex_shrink_0().bg(theme.border)
}

pub fn kbd(theme: &Theme, label: impl Into<SharedString>) -> Div {
    div()
        .px(px(4.0))
        .py(px(1.0))
        .rounded(radius::sm())
        .bg(theme.elevated)
        .border_1()
        .border_color(theme.border)
        .text_size(text::micro())
        .text_color(theme.text_muted)
        .child(label.into())
}

/// A checkbox-style toggle used by the key/value editors.
pub fn toggle(theme: &Theme, enabled: bool) -> Div {
    let (background, border, mark) = if enabled {
        (theme.accent, theme.accent, theme.background)
    } else {
        (theme.surface, theme.border_strong, theme.text_muted)
    };
    div()
        .flex()
        .items_center()
        .justify_center()
        .size(px(14.0))
        .flex_shrink_0()
        .rounded(px(3.0))
        .border_1()
        .border_color(border)
        .bg(background)
        .text_size(px(9.0))
        .text_color(mark)
        .cursor(CursorStyle::PointingHand)
        .child(if enabled { "✓" } else { "" })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Success,
    Error,
}

pub fn toast(theme: &Theme, message: impl Into<SharedString>, kind: ToastKind) -> Div {
    let color = match kind {
        ToastKind::Info => theme.info,
        ToastKind::Success => theme.success,
        ToastKind::Error => theme.error,
    };
    div()
        .flex()
        .items_center()
        .gap(space::sm())
        .px(space::md())
        .py(space::sm())
        .rounded(radius::md())
        .bg(theme.elevated)
        .border_1()
        .border_color(color)
        .text_size(text::small())
        .text_color(theme.text_primary)
        .child(div().size(px(6.0)).rounded(px(3.0)).bg(color))
        .child(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn button_variants_are_distinct() {
        let theme = Theme::dark();
        let primary = ButtonVariant::Primary.colors(&theme);
        let secondary = ButtonVariant::Secondary.colors(&theme);
        assert_ne!(primary.0, secondary.0);
        assert_eq!(ButtonVariant::Danger.colors(&theme).0, theme.error);
    }

    #[test]
    fn chips_expose_labels() {
        let theme = Theme::dark();
        assert_eq!(HttpMethod::Patch.as_str(), "PATCH");
        let _ = method_chip(&theme, HttpMethod::Patch);
        let _ = status_chip(&theme, Some(200), "200 OK");
    }
}
