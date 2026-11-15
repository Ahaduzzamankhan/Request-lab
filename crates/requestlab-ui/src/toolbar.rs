//! Application toolbar, request tab strip and status bar.

use gpui::prelude::*;
use gpui::{div, px, Context, IntoElement, SharedString, Window};
use requestlab_models::human_duration;

use crate::app_view::{AppView, Dialog, SidebarSection};
use crate::components::InteractiveExt;
use crate::components::{self, ButtonVariant};
use crate::theme::{radius, space, text, with_alpha, ThemeMode, TITLE_BAR_HEIGHT};

impl AppView {
    pub(crate) fn render_toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let active_environment = self
            .active_environment
            .and_then(|id| self.environments.iter().find(|env| env.id == id))
            .map(|env| env.name.clone())
            .unwrap_or_else(|| "No environment".to_string());
        let environment_color = if self.active_environment.is_some() {
            theme.success
        } else {
            theme.text_muted
        };

        let mut sections = div().flex().items_center().gap(px(2.0));
        for section in SidebarSection::ALL {
            sections = sections.child(
                components::tab_button(&theme, section.label(), self.sidebar == section, None)
                    .on_click(cx.listener(move |view, _: &gpui::ClickEvent, _window, cx| {
                        view.sidebar = section;
                        cx.notify();
                    })),
            );
        }

        let tab_count = self.tabs.len();
        div()
            .flex()
            .items_center()
            .justify_between()
            .h(TITLE_BAR_HEIGHT)
            .px(space::md())
            .bg(theme.sidebar)
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(space::md())
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(space::sm())
                            .child(
                                div()
                                    .size(px(16.0))
                                    .rounded(radius::sm())
                                    .bg(theme.accent)
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_size(px(10.0))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .text_color(theme.background)
                                    .child("R"),
                            )
                            .child(
                                div()
                                    .text_size(text::title())
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(theme.text_primary)
                                    .child("RequestLab"),
                            ),
                    )
                    .child(components::vertical_divider(&theme))
                    .child(sections),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(space::sm())
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(space::xs())
                            .px(space::sm())
                            .h(px(24.0))
                            .rounded(radius::sm())
                            .bg(theme.surface)
                            .border_1()
                            .border_color(theme.border)
                            .text_size(text::small())
                            .text_color(theme.text_secondary)
                            .cursor(gpui::CursorStyle::PointingHand)
                            .hover(move |style| style.bg(theme.hover))
                            .child(div().size(px(6.0)).rounded(px(3.0)).bg(environment_color))
                            .child(active_environment)
                            .child(div().text_color(theme.text_muted).child("▾"))
                            .on_click(cx.listener(|view, _: &gpui::ClickEvent, _window, cx| {
                                view.open_dialog(Dialog::ChooseEnvironment, cx);
                            })),
                    )
                    .child(
                        components::icon_button(
                            &theme,
                            if theme.mode == ThemeMode::Dark {
                                "☀"
                            } else {
                                "☾"
                            },
                        )
                        .on_click(cx.listener(
                            |view, _: &gpui::ClickEvent, _window, cx| {
                                let next = if view.theme.mode == ThemeMode::Dark {
                                    ThemeMode::Light
                                } else {
                                    ThemeMode::Dark
                                };
                                view.set_theme(next, cx);
                            },
                        )),
                    )
                    .child(components::icon_button(&theme, "⚙").on_click(cx.listener(
                        |view, _: &gpui::ClickEvent, _window, cx| {
                            view.open_dialog(Dialog::Settings, cx);
                        },
                    )))
                    .child(
                        div()
                            .text_size(text::micro())
                            .text_color(theme.text_muted)
                            .child(format!(
                                "{tab_count} tab{}",
                                if tab_count == 1 { "" } else { "s" }
                            )),
                    ),
            )
    }

    pub(crate) fn render_tab_strip(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let mut strip = div()
            .flex()
            .items_center()
            .flex_shrink_0()
            .h(px(32.0))
            .px(space::sm())
            .gap(px(2.0))
            .bg(theme.sidebar)
            .border_b_1()
            .border_color(theme.border);

        for (index, tab) in self.tabs.iter().enumerate() {
            let selected = index == self.active_tab;
            let title: SharedString = tab.editor.title(cx).into();
            let unsaved = tab.editor.saved.is_none();
            let mut item = div()
                .flex()
                .items_center()
                .gap(space::xs())
                .px(space::sm())
                .h(px(24.0))
                .max_w(px(200.0))
                .rounded(radius::sm())
                .text_size(text::small())
                .text_color(if selected {
                    theme.text_primary
                } else {
                    theme.text_secondary
                })
                .when(selected, |style| style.bg(theme.surface))
                .hover(move |style| style.bg(theme.hover))
                .cursor(gpui::CursorStyle::PointingHand)
                .child(
                    div()
                        .flex_shrink_0()
                        .size(px(5.0))
                        .rounded(px(3.0))
                        .bg(if unsaved {
                            theme.warning
                        } else {
                            theme.success
                        }),
                )
                .child(div().truncate().child(title))
                .on_click(cx.listener(move |view, _: &gpui::ClickEvent, _window, cx| {
                    view.active_tab = index;
                    cx.notify();
                }));

            if self.tabs.len() > 1 {
                item = item.child(components::icon_button(&theme, "✕").on_click(cx.listener(
                    move |view, event: &gpui::ClickEvent, window, cx| {
                        if event.click_count() > 0 {
                            view.close_tab(index, cx);
                            let _ = window;
                        }
                    },
                )));
            }
            strip = strip.child(item);
        }

        strip.child(div().flex().items_center().child(
            components::icon_button(&theme, "＋").on_click(cx.listener(
                |view, _: &gpui::ClickEvent, _window, cx| {
                    view.new_tab(cx);
                },
            )),
        ))
    }

    pub(crate) fn render_status_bar(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = self.theme;
        let tab = self.active_tab_ref();
        let (status_label, status_color) = match &tab.response.response {
            Some(response) => (
                format!("{} {}", response.status, response.status_text),
                theme.status_color(Some(response.status)),
            ),
            None => match (&tab.response.error_category, &tab.response.error) {
                (Some(category), _) => (format!("{category} error"), theme.error),
                _ => ("Ready".to_string(), theme.text_muted),
            },
        };
        let duration = tab
            .response
            .response
            .as_ref()
            .map(|response| human_duration(response.duration_ms))
            .unwrap_or_else(|| "—".to_string());
        let size = tab
            .response
            .response
            .as_ref()
            .map(|response| response.size_label())
            .unwrap_or_else(|| "—".to_string());
        let variables = if tab.response.variables.is_empty() {
            "no variables".to_string()
        } else {
            format!("vars: {}", tab.response.variables.join(", "))
        };
        let note = self
            .status_note
            .clone()
            .or_else(|| self.database_path.clone())
            .unwrap_or_default();

        div()
            .flex()
            .items_center()
            .justify_between()
            .h(px(26.0))
            .px(space::md())
            .bg(theme.sidebar)
            .border_t_1()
            .border_color(theme.border)
            .text_size(text::micro())
            .text_color(theme.text_muted)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(space::sm())
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(5.0))
                            .child(div().size(px(6.0)).rounded(px(3.0)).bg(status_color))
                            .child(div().text_color(status_color).child(status_label)),
                    )
                    .child(div().child(format!("· {duration} · {size}")))
                    .child(div().child(format!("· {variables}"))),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(space::md())
                    .child(
                        div()
                            .px(px(5.0))
                            .py(px(1.0))
                            .rounded(px(3.0))
                            .bg(with_alpha(theme.accent, 0.14))
                            .text_color(theme.accent)
                            .child(format!(
                                "timeout {}",
                                crate::app_view::timeout_label(self.timeout_seconds)
                            )),
                    )
                    .child(div().truncate().max_w(px(420.0)).child(note)),
            )
    }
}

/// A small helper so panels can render consistent buttons.
pub fn send_button_label(sending: bool) -> &'static str {
    if sending {
        "Cancel"
    } else {
        "Send"
    }
}

/// The variant used by the primary action.
pub fn send_button_variant(sending: bool) -> ButtonVariant {
    if sending {
        ButtonVariant::Danger
    } else {
        ButtonVariant::Primary
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn send_button_reflects_state() {
        assert_eq!(send_button_label(false), "Send");
        assert_eq!(send_button_label(true), "Cancel");
        assert_eq!(send_button_variant(true), ButtonVariant::Danger);
    }
}
