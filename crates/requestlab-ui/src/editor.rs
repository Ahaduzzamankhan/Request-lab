//! The request editor: URL bar, configuration tabs and key/value editors.

use gpui::prelude::*;
use gpui::{div, px, Context, IntoElement, SharedString};
use requestlab_models::{ApiKeyLocation, HttpMethod, RequestBodyKind};
use uuid::Uuid;

use crate::app_view::{AppView, Dialog};
use crate::components::InteractiveExt;
use crate::components::{self, ButtonVariant};
use crate::editor_state::{AuthKind, EditorTab, KeyValueRow};
use crate::theme::{radius, space, text};

impl AppView {
    pub(crate) fn render_request_editor(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let tab_index = self.active_tab;
        let tab = self.active_tab_ref();
        let editor = &tab.editor;
        let request = editor.to_request(cx);
        let issues = request.validate();
        let sending = tab.sending;
        let saved = editor.saved.is_some();
        let method = editor.method;
        let title = editor.title(cx);

        let request_bar = div()
            .flex()
            .items_center()
            .gap(space::sm())
            .px(space::md())
            .py(space::sm())
            .bg(theme.background)
            .child(
                components::method_chip(&theme, method)
                    .h(px(28.0))
                    .px(space::sm())
                    .child(div().text_size(text::micro()).child(" ▾"))
                    .on_click(cx.listener(move |view, _: &gpui::ClickEvent, _window, cx| {
                        view.open_dialog(Dialog::ChooseMethod { tab: tab_index }, cx);
                    })),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .flex_1()
                    .min_w(px(0.0))
                    .h(px(28.0))
                    .px(space::sm())
                    .rounded(radius::md())
                    .bg(theme.surface)
                    .border_1()
                    .border_color(if issues.is_empty() {
                        theme.border
                    } else {
                        theme.error
                    })
                    .font_family(theme.mono_family())
                    .text_size(text::body())
                    .child(editor.url.clone()),
            )
            .child(
                components::button(
                    &theme,
                    if sending { "Cancel" } else { "Send" },
                    if sending {
                        ButtonVariant::Danger
                    } else {
                        ButtonVariant::Primary
                    },
                )
                .h(px(28.0))
                .min_w(px(72.0))
                .on_click(cx.listener(
                    |view, _: &gpui::ClickEvent, _window, cx| {
                        if view.active_tab_ref().sending {
                            view.cancel_request(cx);
                        } else {
                            view.send_request(cx);
                        }
                    },
                )),
            )
            .child(
                components::button(
                    &theme,
                    if saved { "Update" } else { "Save" },
                    ButtonVariant::Secondary,
                )
                .h(px(28.0))
                .on_click(cx.listener(
                    |view, _: &gpui::ClickEvent, _window, cx| {
                        match view.active_tab_ref().editor.saved {
                            Some(location) => {
                                view.save_request_to(location.collection_id, location.folder_id, cx)
                            }
                            None => {
                                let index = view.active_tab;
                                view.open_dialog(Dialog::SaveRequest { tab: index }, cx);
                            }
                        }
                    },
                )),
            );

        let mut tab_row = div()
            .flex()
            .items_center()
            .gap(px(2.0))
            .px(space::md())
            .h(px(34.0))
            .bg(theme.background)
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(space::xs())
                    .mr(space::md())
                    .text_size(text::small())
                    .text_color(theme.text_secondary)
                    .child(div().size(px(5.0)).rounded(px(3.0)).bg(if saved {
                        theme.success
                    } else {
                        theme.warning
                    }))
                    .child(title),
            );

        for editor_tab in EditorTab::ALL {
            let selected = editor.tab == editor_tab;
            let count = editor.count(editor_tab, cx);
            tab_row = tab_row.child(
                components::tab_button(&theme, editor_tab.label(), selected, Some(count)).on_click(
                    cx.listener(move |view, _: &gpui::ClickEvent, _window, cx| {
                        if let Some(tab) = view.tabs.get_mut(view.active_tab) {
                            tab.editor.tab = editor_tab;
                        }
                        cx.notify();
                    }),
                ),
            );
        }

        let content: gpui::AnyElement = match editor.tab {
            EditorTab::Params => self
                .render_kv_editor(EditorTab::Params, cx)
                .into_any_element(),
            EditorTab::Headers => self
                .render_kv_editor(EditorTab::Headers, cx)
                .into_any_element(),
            EditorTab::Body => self.render_body_editor(cx).into_any_element(),
            EditorTab::Auth => self.render_auth_editor(cx).into_any_element(),
        };

        let mut footer = div().flex().flex_col().gap(px(2.0));
        for issue in issues.iter().take(3) {
            footer = footer.child(
                div()
                    .text_size(text::small())
                    .text_color(theme.error)
                    .child(SharedString::from(issue.to_string())),
            );
        }

        div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .child(request_bar)
            .child(tab_row)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .h(px(196.0))
                    .overflow_scroll()
                    .bg(theme.background)
                    .child(content),
            )
            .child(
                div()
                    .px(space::md())
                    .py(px(4.0))
                    .min_h(px(22.0))
                    .border_t_1()
                    .border_color(theme.border)
                    .child(if issues.is_empty() {
                        div()
                            .text_size(text::micro())
                            .text_color(theme.text_muted)
                            .child("Request is valid")
                            .into_any_element()
                    } else {
                        footer.into_any_element()
                    }),
            )
    }

    fn render_kv_editor(&self, editor_tab: EditorTab, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let editor = &self.active_tab_ref().editor;
        let rows: &[KeyValueRow] = match editor_tab {
            EditorTab::Params => &editor.params,
            EditorTab::Headers => &editor.headers,
            _ => &editor.params,
        };

        let mut list = div().flex().flex_col().gap(px(1.0)).p(space::sm());
        list = list.child(
            div()
                .flex()
                .items_center()
                .gap(space::sm())
                .px(space::sm())
                .py(px(2.0))
                .text_size(text::micro())
                .text_color(theme.text_muted)
                .child(div().w(px(18.0)).child(""))
                .child(div().flex_1().child("Key"))
                .child(div().flex_1().child("Value"))
                .child(div().w(px(52.0)).child("")),
        );

        for row in rows {
            let row_id = row.id;
            let enabled = row.enabled;
            list = list.child(
                div()
                    .flex()
                    .items_center()
                    .gap(space::sm())
                    .px(space::sm())
                    .py(px(2.0))
                    .rounded(radius::sm())
                    .child(components::toggle(&theme, enabled).on_click(cx.listener(
                        move |view, _: &gpui::ClickEvent, _window, cx| {
                            if let Some(tab) = view.tabs.get_mut(view.active_tab) {
                                let rows = match editor_tab {
                                    EditorTab::Headers => &mut tab.editor.headers,
                                    _ => &mut tab.editor.params,
                                };
                                if let Some(row) = rows.iter_mut().find(|row| row.id == row_id) {
                                    row.enabled = !row.enabled;
                                }
                            }
                            cx.notify();
                        },
                    )))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .h(px(24.0))
                            .px(space::xs())
                            .rounded(px(3.0))
                            .bg(theme.surface)
                            .border_1()
                            .border_color(theme.border)
                            .font_family(theme.mono_family())
                            .text_size(text::small())
                            .child(row.key.clone()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .h(px(24.0))
                            .px(space::xs())
                            .rounded(px(3.0))
                            .bg(theme.surface)
                            .border_1()
                            .border_color(theme.border)
                            .font_family(theme.mono_family())
                            .text_size(text::small())
                            .child(row.value.clone()),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .w(px(52.0))
                            .child(components::icon_button(&theme, "⧉").on_click(cx.listener(
                                move |view, _: &gpui::ClickEvent, _window, cx| {
                                    view.duplicate_row(editor_tab, row_id, cx);
                                },
                            )))
                            .child(components::icon_button(&theme, "✕").on_click(cx.listener(
                                move |view, _: &gpui::ClickEvent, _window, cx| {
                                    if let Some(tab) = view.tabs.get_mut(view.active_tab) {
                                        tab.editor.remove_row(editor_tab, row_id);
                                    }
                                    cx.notify();
                                },
                            ))),
                    ),
            );
        }

        list.child(
            div()
                .flex()
                .items_center()
                .gap(space::sm())
                .px(space::sm())
                .py(space::sm())
                .child(
                    components::button(&theme, "＋ Add row", ButtonVariant::Ghost).on_click(
                        cx.listener(move |view, _: &gpui::ClickEvent, _window, cx| {
                            if let Some(tab) = view.tabs.get_mut(view.active_tab) {
                                tab.editor.add_row(editor_tab, cx);
                            }
                            cx.notify();
                        }),
                    ),
                ),
        )
    }

    fn render_body_editor(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let editor = &self.active_tab_ref().editor;
        let kind = editor.body_kind;

        let mut kinds = div().flex().items_center().gap(px(2.0)).p(space::sm());
        for candidate in RequestBodyKind::ALL {
            kinds = kinds.child(
                components::tab_button(&theme, candidate.label(), kind == candidate, None)
                    .on_click(cx.listener(move |view, _: &gpui::ClickEvent, _window, cx| {
                        if let Some(tab) = view.tabs.get_mut(view.active_tab) {
                            tab.editor.body_kind = candidate;
                        }
                        cx.notify();
                    })),
            );
        }

        let body_area: gpui::AnyElement = match kind {
            RequestBodyKind::None => components::empty_state(
                &theme,
                "No request body",
                "Choose JSON, Raw or Form to send content with this request.",
            )
            .into_any_element(),
            RequestBodyKind::Json => {
                let error = editor.to_request(cx).body.json_validation_error();
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h(px(0.0))
                    .child(
                        div()
                            .flex_1()
                            .min_h(px(0.0))
                            .overflow_scroll()
                            .m(space::sm())
                            .p(space::sm())
                            .rounded(radius::md())
                            .bg(theme.surface)
                            .border_1()
                            .border_color(if error.is_some() {
                                theme.error
                            } else {
                                theme.border
                            })
                            .font_family(theme.mono_family())
                            .text_size(text::small())
                            .child(editor.body.clone()),
                    )
                    .child(match error {
                        Some(message) => div()
                            .px(space::md())
                            .pb(space::sm())
                            .text_size(text::micro())
                            .text_color(theme.error)
                            .child(message),
                        None => div()
                            .px(space::md())
                            .pb(space::sm())
                            .text_size(text::micro())
                            .text_color(theme.success)
                            .child("Valid JSON"),
                    })
                    .into_any_element()
            }
            RequestBodyKind::Raw => div()
                .flex()
                .flex_col()
                .flex_1()
                .min_h(px(0.0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(space::sm())
                        .px(space::md())
                        .py(px(2.0))
                        .child(
                            div()
                                .text_size(text::micro())
                                .text_color(theme.text_muted)
                                .child("Content-Type"),
                        )
                        .child(
                            div()
                                .w(px(220.0))
                                .h(px(24.0))
                                .px(space::xs())
                                .rounded(px(3.0))
                                .bg(theme.surface)
                                .border_1()
                                .border_color(theme.border)
                                .font_family(theme.mono_family())
                                .text_size(text::small())
                                .child(editor.body_content_type.clone()),
                        ),
                )
                .child(
                    div()
                        .flex_1()
                        .min_h(px(0.0))
                        .overflow_scroll()
                        .m(space::sm())
                        .p(space::sm())
                        .rounded(radius::md())
                        .bg(theme.surface)
                        .border_1()
                        .border_color(theme.border)
                        .font_family(theme.mono_family())
                        .text_size(text::small())
                        .child(editor.body.clone()),
                )
                .into_any_element(),
            RequestBodyKind::FormUrlEncoded => {
                let mut rows = div().flex().flex_col().gap(px(2.0)).p(space::sm());
                for row in &editor.form {
                    let row_id = row.id;
                    rows = rows.child(
                        div()
                            .flex()
                            .items_center()
                            .gap(space::sm())
                            .px(space::sm())
                            .child(
                                div()
                                    .flex_1()
                                    .h(px(24.0))
                                    .px(space::xs())
                                    .rounded(px(3.0))
                                    .bg(theme.surface)
                                    .border_1()
                                    .border_color(theme.border)
                                    .font_family(theme.mono_family())
                                    .text_size(text::small())
                                    .child(row.key.clone()),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .h(px(24.0))
                                    .px(space::xs())
                                    .rounded(px(3.0))
                                    .bg(theme.surface)
                                    .border_1()
                                    .border_color(theme.border)
                                    .font_family(theme.mono_family())
                                    .text_size(text::small())
                                    .child(row.value.clone()),
                            )
                            .child(components::icon_button(&theme, "✕").on_click(cx.listener(
                                move |view, _: &gpui::ClickEvent, _window, cx| {
                                    if let Some(tab) = view.tabs.get_mut(view.active_tab) {
                                        tab.editor.remove_form_row(row_id);
                                    }
                                    cx.notify();
                                },
                            ))),
                    );
                }
                rows.child(div().px(space::sm()).py(space::sm()).child(
                    components::button(&theme, "＋ Add field", ButtonVariant::Ghost).on_click(
                        cx.listener(|view, _: &gpui::ClickEvent, _window, cx| {
                            if let Some(tab) = view.tabs.get_mut(view.active_tab) {
                                tab.editor.add_form_row(cx);
                            }
                            cx.notify();
                        }),
                    ),
                ))
                .into_any_element()
            }
        };

        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.0))
            .child(kinds)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h(px(0.0))
                    .child(body_area),
            )
    }

    fn render_auth_editor(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let editor = &self.active_tab_ref().editor;
        let kind = editor.auth_kind;

        let mut kinds = div().flex().items_center().gap(px(2.0)).p(space::sm());
        for candidate in AuthKind::ALL {
            kinds = kinds.child(
                components::tab_button(&theme, candidate.label(), kind == candidate, None)
                    .on_click(cx.listener(move |view, _: &gpui::ClickEvent, _window, cx| {
                        if let Some(tab) = view.tabs.get_mut(view.active_tab) {
                            let editor = &mut tab.editor;
                            if editor.auth_kind != candidate {
                                editor.auth_kind = candidate;
                                editor.auth_primary.update(cx, |input, _| input.clear());
                                editor.auth_secondary.update(cx, |input, _| input.clear());
                            }
                        }
                        cx.notify();
                    })),
            );
        }

        let fields: gpui::AnyElement = match kind {
            AuthKind::None => components::empty_state(
                &theme,
                "No authentication",
                "Requests are sent without credentials. Values are never logged.",
            )
            .into_any_element(),
            _ => {
                let primary_label = kind.primary_label();
                let secondary_label = kind.secondary_label();
                let mut fields = div()
                    .flex()
                    .flex_col()
                    .gap(space::sm())
                    .p(space::md())
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(space::sm())
                            .child(
                                div()
                                    .w(px(90.0))
                                    .child(components::field_label(&theme, primary_label)),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .h(px(26.0))
                                    .px(space::xs())
                                    .rounded(radius::sm())
                                    .bg(theme.surface)
                                    .border_1()
                                    .border_color(theme.border)
                                    .font_family(theme.mono_family())
                                    .text_size(text::small())
                                    .child(editor.auth_primary.clone()),
                            ),
                    );

                if !secondary_label.is_empty() {
                    fields = fields.child(
                        div()
                            .flex()
                            .items_center()
                            .gap(space::sm())
                            .child(
                                div()
                                    .w(px(90.0))
                                    .child(components::field_label(&theme, secondary_label)),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .h(px(26.0))
                                    .px(space::xs())
                                    .rounded(radius::sm())
                                    .bg(theme.surface)
                                    .border_1()
                                    .border_color(theme.border)
                                    .font_family(theme.mono_family())
                                    .text_size(text::small())
                                    .child(editor.auth_secondary.clone()),
                            ),
                    );
                }

                if kind == AuthKind::ApiKey {
                    let mut placement = div().flex().items_center().gap(space::xs()).child(
                        div()
                            .w(px(90.0))
                            .child(components::field_label(&theme, "Add to")),
                    );
                    for location in [ApiKeyLocation::Header, ApiKeyLocation::Query] {
                        let label = if location == ApiKeyLocation::Header {
                            "Header"
                        } else {
                            "Query"
                        };
                        placement = placement.child(
                            components::tab_button(
                                &theme,
                                label,
                                editor.api_key_location == location,
                                None,
                            )
                            .on_click(cx.listener(
                                move |view, _: &gpui::ClickEvent, _window, cx| {
                                    if let Some(tab) = view.tabs.get_mut(view.active_tab) {
                                        tab.editor.api_key_location = location;
                                    }
                                    cx.notify();
                                },
                            )),
                        );
                    }
                    fields = fields.child(placement);
                }

                fields
                    .child(
                        div()
                            .text_size(text::micro())
                            .text_color(theme.text_muted)
                            .child(
                            "Credentials are stored with the request and never written to logs.",
                        ),
                    )
                    .into_any_element()
            }
        };

        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.0))
            .child(kinds)
            .child(fields)
    }

    /// Copy a key/value row, preserving its text.
    pub(crate) fn duplicate_row(&mut self, tab: EditorTab, row_id: Uuid, cx: &mut Context<Self>) {
        let Some(current) = self.tabs.get_mut(self.active_tab) else {
            return;
        };
        let rows = match tab {
            EditorTab::Headers => &current.editor.headers,
            _ => &current.editor.params,
        };
        let Some(row) = rows.iter().find(|row| row.id == row_id) else {
            return;
        };
        let key = row.key_text(cx);
        let value = row.value_text(cx);
        let enabled = row.enabled;
        let placeholder = if tab == EditorTab::Headers {
            "Header"
        } else {
            "Parameter"
        };
        let key_entity = cx.new({
            let key = key.clone();
            move |cx| {
                let mut input = crate::input::TextInput::new(cx, placeholder, false);
                input.set_content(key);
                input
            }
        });
        let value_entity = cx.new(move |cx| {
            let mut input = crate::input::TextInput::new(cx, "Value", false);
            input.set_content(value);
            input
        });
        let new_row = KeyValueRow {
            id: Uuid::new_v4(),
            key: key_entity,
            value: value_entity,
            enabled,
        };
        let Some(current) = self.tabs.get_mut(self.active_tab) else {
            return;
        };
        match tab {
            EditorTab::Headers => current.editor.headers.push(new_row),
            _ => current.editor.params.push(new_row),
        }
        cx.notify();
    }

    /// Select the request method and close the picker.
    pub(crate) fn set_method(&mut self, method: HttpMethod, cx: &mut Context<Self>) {
        if let Some(tab) = self.tabs.get_mut(self.active_tab) {
            tab.editor.method = method;
        }
        self.dialog = None;
        cx.notify();
    }
}
