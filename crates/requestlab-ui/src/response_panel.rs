//! The response inspector: status line, tabs, JSON tree, raw text and headers.

use gpui::prelude::*;
use gpui::InteractiveElement as _;
use gpui::{div, px, ClipboardItem, Context, IntoElement, SharedString};
use requestlab_models::{human_duration, BodyKind, ResponseBody};

use crate::app_view::{AppView, ResponseTab};
use crate::components::{self, ButtonVariant};
use crate::json_tree::{self, JsonRow};
use crate::theme::{radius, space, text};

/// Rows beyond this count are rendered truncated, with a clear notice.
const MAX_JSON_ROWS: usize = 3000;
/// Characters beyond this count are truncated in the text view.
const MAX_TEXT_CHARS: usize = 200_000;

impl AppView {
    pub(crate) fn render_response(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let tab = self.active_tab_ref();
        let state = &tab.response;
        let body_kind = state.response.as_ref().map(|response| response.body_kind());

        let mut header = div()
            .flex()
            .items_center()
            .justify_between()
            .flex_shrink_0()
            .px(space::md())
            .h(px(32.0))
            .bg(theme.surface)
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(space::sm())
                    .child(
                        div()
                            .text_size(text::small())
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(theme.text_secondary)
                            .child("Response"),
                    )
                    .child(match (&state.response, &state.error_category) {
                        (Some(response), _) => components::status_chip(
                            &theme,
                            Some(response.status),
                            format!(
                                "{} {}",
                                response.status,
                                if response.status_text.is_empty() {
                                    ""
                                } else {
                                    &response.status_text
                                }
                            ),
                        ),
                        (None, Some(category)) => {
                            components::chip(&theme, category.clone(), theme.error, true)
                        }
                        _ => components::chip(&theme, "Idle", theme.text_muted, false),
                    })
                    .child({
                        let timing = state
                            .response
                            .as_ref()
                            .map(|response| human_duration(response.duration_ms))
                            .unwrap_or_else(|| "—".to_string());
                        let size = state
                            .response
                            .as_ref()
                            .map(|response| response.size_label())
                            .unwrap_or_else(|| "—".to_string());
                        div()
                            .flex()
                            .items_center()
                            .gap(space::sm())
                            .text_size(text::micro())
                            .text_color(theme.text_muted)
                            .child(format!("{timing} · {size}"))
                    })
                    .child(
                        state
                            .response
                            .as_ref()
                            .and_then(|response| response.content_type.clone())
                            .map(|content_type| {
                                components::chip(&theme, content_type, theme.text_secondary, false)
                            })
                            .unwrap_or_else(|| div()),
                    )
                    .child(
                        body_kind
                            .map(|kind| components::chip(&theme, kind.label(), theme.info, false))
                            .unwrap_or_else(|| div()),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(space::xs())
                    .child(
                        components::button(&theme, "Copy body", ButtonVariant::Ghost)
                            .h(px(22.0))
                            .on_click(cx.listener(|view, _: &gpui::ClickEvent, _window, cx| {
                                let text = view
                                    .active_tab_ref()
                                    .response
                                    .response
                                    .as_ref()
                                    .and_then(|response| response.body.as_text())
                                    .unwrap_or_default()
                                    .to_string();
                                if !text.is_empty() {
                                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                                    view.toast = Some((
                                        "Response body copied".to_string(),
                                        crate::components::ToastKind::Success,
                                    ));
                                    cx.notify();
                                }
                            })),
                    )
                    .child(
                        components::button(&theme, "Clear", ButtonVariant::Ghost)
                            .h(px(22.0))
                            .on_click(cx.listener(|view, _: &gpui::ClickEvent, _window, cx| {
                                if let Some(tab) = view.tabs.get_mut(view.active_tab) {
                                    tab.response.response = None;
                                    tab.response.error = None;
                                    tab.response.error_category = None;
                                    tab.response.variables.clear();
                                    tab.response.resolved_url = None;
                                }
                                cx.notify();
                            })),
                    ),
            );

        let mut tabs = div()
            .flex()
            .items_center()
            .gap(px(2.0))
            .px(space::sm())
            .h(px(30.0))
            .bg(theme.background)
            .border_b_1()
            .border_color(theme.border);

        for response_tab in ResponseTab::ALL {
            let selected = state.tab == response_tab;
            let count = match response_tab {
                ResponseTab::Headers => state.response.as_ref().map(|r| r.headers.len()),
                ResponseTab::Cookies => state.response.as_ref().map(|r| r.cookies.len()),
                _ => None,
            };
            tabs = tabs.child(
                components::tab_button(&theme, response_tab.label(), selected, count).on_click(
                    cx.listener(move |view, _: &gpui::ClickEvent, _window, cx| {
                        if let Some(tab) = view.tabs.get_mut(view.active_tab) {
                            tab.response.tab = response_tab;
                        }
                        cx.notify();
                    }),
                ),
            );
        }

        if matches!(state.tab, ResponseTab::Pretty | ResponseTab::Raw) && state.response.is_some() {
            tabs = tabs.child(
                div()
                    .flex()
                    .items_center()
                    .flex_1()
                    .justify_end()
                    .child(
                        div()
                            .w(px(200.0))
                            .h(px(22.0))
                            .px(space::xs())
                            .rounded(radius::sm())
                            .bg(theme.surface)
                            .border_1()
                            .border_color(theme.border)
                            .font_family(theme.mono_family())
                            .text_size(text::small())
                            .child(state.search.clone()),
                    ),
            );
        }

        let variables_line = if state.variables.is_empty() {
            div().h(px(0.0)).into_any_element()
        } else {
            div()
                .flex()
                .items_center()
                .gap(space::xs())
                .px(space::md())
                .py(px(3.0))
                .bg(with_alpha_bg(theme.accent))
                .text_size(text::micro())
                .text_color(theme.accent)
                .child(format!("resolved {}", state.resolved_url.clone().unwrap_or_default()))
                .into_any_element()
        };

        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.0))
            .bg(theme.background)
            .child(header)
            .child(tabs)
            .child(variables_line)
            .child(self.render_response_body(cx))
    }

    fn render_response_body(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let state = &self.active_tab_ref().response;

        if let Some(error) = &state.error {
            let category = state
                .error_category
                .clone()
                .unwrap_or_else(|| "Error".to_string());
            return div()
                .flex()
                .flex_col()
                .flex_1()
                .min_h(px(0.0))
                .p(space::lg())
                .gap(space::sm())
                .child(
                    div()
                        .text_size(text::title())
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(theme.error)
                        .child(format!("{category} error")),
                )
                .child(
                    div()
                        .text_size(text::small())
                        .text_color(theme.text_secondary)
                        .child(SharedString::from(error.clone())),
                );
        }

        let Some(response) = &state.response else {
            return components::empty_state(
                &theme,
                "No response yet",
                "Press Send (or Ctrl+Enter) to run this request.",
            )
            .into_any_element();
        };

        match state.tab {
            ResponseTab::Pretty => {
                let kind = response.body_kind();
                if kind == BodyKind::Empty {
                    return components::empty_state(
                        &theme,
                        "Empty response body",
                        "The server returned no content.",
                    )
                    .into_any_element();
                }
                if kind == BodyKind::Json {
                    match response.pretty_json() {
                        Some(Ok(_)) => {
                            let rows = state.json_rows(cx).unwrap_or_default();
                            return self.render_json_rows(rows, cx);
                        }
                        Some(Err(message)) => {
                            return div()
                                .p(space::md())
                                .text_size(text::small())
                                .text_color(theme.error)
                                .child(message)
                                .into_any_element();
                        }
                        None => {}
                    }
                }
                if kind.is_textual() {
                    let text = response.body.as_text().unwrap_or_default();
                    let truncated = text.chars().count() > MAX_TEXT_CHARS;
                    let shown: String = if truncated {
                        text.chars().take(MAX_TEXT_CHARS).collect()
                    } else {
                        text.to_string()
                    };
                    return div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_h(px(0.0))
                        .overflow_scroll()
                        .p(space::md())
                        .font_family(theme.mono_family())
                        .text_size(text::small())
                        .text_color(theme.text_primary)
                        .child(shown)
                        .when(truncated, |style| {
                            style.child(
                                div()
                                    .mt(space::sm())
                                    .text_size(text::micro())
                                    .text_color(theme.warning)
                                    .child(format!(
                                        "Preview limited to {MAX_TEXT_CHARS} characters. Use Raw or Copy for the full body."
                                    )),
                            )
                        })
                        .into_any_element();
                }

                let size = response
                    .body
                    .len();
                components::empty_state(
                    &theme,
                    "Binary response",
                    format!("{size} bytes of binary data. Use Copy or save the response to inspect it."),
                )
                .into_any_element()
            }
            ResponseTab::Raw => {
                let text = match &response.body {
                    ResponseBody::Text { text, .. } => text.clone(),
                    ResponseBody::Binary { bytes, .. } => {
                        format!("<binary body, {} bytes>", bytes.len())
                    }
                    ResponseBody::Empty => String::new(),
                };
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_scroll()
                    .p(space::md())
                    .font_family(theme.mono_family())
                    .text_size(text::small())
                    .text_color(theme.text_primary)
                    .child(text)
                    .into_any_element()
            }
            ResponseTab::Headers => {
                let mut list = div().flex().flex_col().flex_1().min_h(px(0.0)).overflow_scroll();
                for header in &response.headers {
                    list = list.child(
                        div()
                            .flex()
                            .items_start()
                            .gap(space::md())
                            .px(space::md())
                            .py(px(3.0))
                            .border_b_1()
                            .border_color(theme.border)
                            .child(
                                div()
                                    .w(px(220.0))
                                    .flex_shrink_0()
                                    .font_family(theme.mono_family())
                                    .text_size(text::small())
                                    .text_color(theme.accent)
                                    .child(header.name.clone()),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .font_family(theme.mono_family())
                                    .text_size(text::small())
                                    .text_color(theme.text_secondary)
                                    .child(header.value.clone()),
                            ),
                    );
                }
                list.into_any_element()
            }
            ResponseTab::Cookies => {
                if response.cookies.is_empty() {
                    return components::empty_state(
                        &theme,
                        "No cookies",
                        "The server did not set any cookies on this response.",
                    )
                    .into_any_element();
                }
                let mut list = div().flex().flex_col().flex_1().min_h(px(0.0)).overflow_scroll();
                for cookie in &response.cookies {
                    list = list.child(
                        div()
                            .flex()
                            .items_center()
                            .gap(space::md())
                            .px(space::md())
                            .py(px(4.0))
                            .border_b_1()
                            .border_color(theme.border)
                            .child(
                                div()
                                    .w(px(160.0))
                                    .font_family(theme.mono_family())
                                    .text_size(text::small())
                                    .text_color(theme.accent)
                                    .child(cookie.name.clone()),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .font_family(theme.mono_family())
                                    .text_size(text::small())
                                    .text_color(theme.text_secondary)
                                    .child(cookie.value.clone()),
                            )
                            .child(
                                div()
                                    .text_size(text::micro())
                                    .text_color(theme.text_muted)
                                    .child(
                                        [
                                            cookie.domain.clone().unwrap_or_default(),
                                            cookie.path.clone().unwrap_or_default(),
                                            if cookie.secure { "Secure".into() } else { String::new() },
                                            if cookie.http_only { "HttpOnly".into() } else { String::new() },
                                        ]
                                        .into_iter()
                                        .filter(|part| !part.is_empty())
                                        .collect::<Vec<_>>()
                                        .join(" · "),
                                    ),
                            ),
                    );
                }
                list.into_any_element()
            }
        }
    }

    fn render_json_rows(&self, rows: Vec<JsonRow>, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let total = rows.len();
        let matches = json_tree::match_count(&rows);
        let truncated = total > MAX_JSON_ROWS;

        let mut list = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.0))
            .overflow_scroll()
            .py(space::xs())
            .font_family(theme.mono_family())
            .text_size(text::small());

        for row in rows.into_iter().take(MAX_JSON_ROWS) {
            let row_clone = row.clone();
            let path = row.path.clone();
            let expandable = row.expandable;
            let matched = row.matched;
            let token_color = theme.json_color(row.token);
            let mut line = div()
                .flex()
                .items_center()
                .w_full()
                .h(px(18.0))
                .pl(space::md() + px(json_tree::indent(row.depth)))
                .pr(space::md())
                .when(matched, |style| style.bg(crate::theme::with_alpha(theme.warning, 0.16)))
                .hover(|style| style.bg(theme.hover))
                .cursor(gpui::CursorStyle::PointingHand);

            if expandable {
                line = line.child(
                    div()
                        .w(px(12.0))
                        .flex_shrink_0()
                        .text_color(theme.text_secondary)
                        .child(if row.expanded { "▾" } else { "▸" }),
                );
            } else {
                line = line.child(div().w(px(12.0)).flex_shrink_0());
            }

            if let Some(key) = &row.key {
                line = line.child(
                    div()
                        .text_color(theme.json_color(crate::theme::JsonToken::Key))
                        .child(format!("\"{key}\":")),
                );
            }

            if expandable {
                if let Some(summary) = &row.summary {
                    line = line.child(
                        div()
                            .ml(px(4.0))
                            .text_color(theme.text_muted)
                            .child(summary.clone()),
                    );
                }
            } else {
                line = line.child(
                    div()
                        .ml(px(4.0))
                        .text_color(token_color)
                        .child(row.value.clone()),
                );
            }

            if row.trailing_comma {
                line = line.child(div().text_color(theme.text_muted).child(","));
            }

            line = line.on_click(cx.listener(move |view, _: &gpui::ClickEvent, _window, cx| {
                if expandable {
                    if let Some(tab) = view.tabs.get_mut(view.active_tab) {
                        if !tab.response.expanded.remove(&path) {
                            tab.response.expanded.insert(path.clone());
                        }
                    }
                } else if let Some(response) = &view.active_tab_ref().response.response {
                    if let Some(text) = response.body.as_text() {
                        cx.write_to_clipboard(ClipboardItem::new_string(text.to_string()));
                    }
                }
                let _ = &row_clone;
                cx.notify();
            }));

            list = list.child(line);
        }

        let mut container = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.0))
            .child(list);

        if truncated || matches > 0 {
            container = container.child(
                div()
                    .px(space::md())
                    .py(px(3.0))
                    .border_t_1()
                    .border_color(theme.border)
                    .text_size(text::micro())
                    .text_color(theme.warning)
                    .child(if truncated {
                        format!(
                            "Showing the first {MAX_JSON_ROWS} of {total} rows. Search or use Copy for the whole document."
                        )
                    } else {
                        format!("{matches} matching rows")
                    }),
            );
        }

        container
    }
}

fn with_alpha_bg(color: gpui::Rgba) -> gpui::Rgba {
    crate::theme::with_alpha(color, 0.10)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limits_are_reasonable() {
        assert!(MAX_JSON_ROWS >= 1000);
        assert!(MAX_TEXT_CHARS >= 100_000);
    }
}
