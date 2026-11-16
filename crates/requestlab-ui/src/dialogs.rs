//! Modal dialogs: confirmation, prompts, pickers and settings.

use gpui::prelude::*;
use gpui::{div, px, AnyElement, Context, IntoElement};
use requestlab_models::HttpMethod;

use ely_gpui_component::buttons::{Button as ElyButton, ButtonVariant as ElyButtonVariant};
use ely_gpui_component::theme::ControlSize;

use crate::app_view::{AppView, Dialog};
use crate::components::{self, ButtonVariant};
use crate::components::{entity_handler, InteractiveExt};
use crate::theme::{radius, space, text, ThemeMode};

impl AppView {
    /// A dialog footer action, rendered with Ely's button.
    fn dialog_button(
        &self,
        cx: &mut Context<Self>,
        id: &'static str,
        label: &'static str,
        variant: ElyButtonVariant,
        run: impl Fn(&mut AppView, &mut Context<AppView>) + 'static,
    ) -> ElyButton {
        ElyButton::new(id, label)
            .variant(variant)
            .size(ControlSize::Sm)
            .on_click(entity_handler(cx, run))
    }

    pub(crate) fn render_dialog(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let theme = self.theme;
        let dialog = self.dialog.as_ref()?;

        let mut card = div()
            .flex()
            .flex_col()
            .gap(space::md())
            .w(px(460.0))
            .p(space::lg())
            .rounded(radius::lg())
            .bg(theme.elevated)
            .border_1()
            .border_color(theme.border_strong);

        match dialog {
            Dialog::Confirm {
                title,
                message,
                action,
            } => {
                let action = *action;
                card = card
                    .child(self.dialog_title(title))
                    .child(
                        div()
                            .text_size(text::small())
                            .text_color(theme.text_secondary)
                            .child(message.clone()),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_end()
                            .gap(space::sm())
                            .child(self.dialog_button(
                                cx,
                                "confirm-cancel",
                                "Cancel",
                                ElyButtonVariant::Ghost,
                                |view, cx| view.close_dialog(cx),
                            ))
                            .child(self.dialog_button(
                                cx,
                                "confirm-delete",
                                "Delete",
                                ElyButtonVariant::Danger,
                                move |view, cx| view.execute_destructive(action, cx),
                            )),
                    );
            }
            Dialog::Prompt {
                title,
                hint,
                action,
                ..
            } => {
                let action = *action;
                card = card
                    .child(self.dialog_title(title))
                    .child(
                        div()
                            .h(px(28.0))
                            .px(space::xs())
                            .rounded(radius::sm())
                            .bg(theme.surface)
                            .border_1()
                            .border_color(theme.border)
                            .font_family(theme.mono_family())
                            .text_size(text::small())
                            .child(self.dialog_input.clone()),
                    )
                    .child(
                        div()
                            .text_size(text::micro())
                            .text_color(theme.text_muted)
                            .child(hint.clone()),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_end()
                            .gap(space::sm())
                            .child(self.dialog_button(
                                cx,
                                "prompt-cancel",
                                "Cancel",
                                ElyButtonVariant::Ghost,
                                |view, cx| view.close_dialog(cx),
                            ))
                            .child(self.dialog_button(
                                cx,
                                "prompt-save",
                                "Save",
                                ElyButtonVariant::Primary,
                                move |view, cx| view.execute_prompt(action, cx),
                            )),
                    );
            }
            Dialog::ChooseMethod { .. } => {
                card = card.child(self.dialog_title("Request method"));
                let mut grid = div().flex().flex_wrap().gap(space::sm());
                for method in HttpMethod::ALL {
                    let color = theme.method_color(method);
                    grid = grid.child(
                        components::chip(&theme, method.as_str(), color, true)
                            .h(px(30.0))
                            .px(space::md())
                            .cursor(gpui::CursorStyle::PointingHand)
                            .on_press(
                                "dialogs:142",
                                cx.listener(move |view, _: &gpui::ClickEvent, _window, cx| {
                                    view.set_method(method, cx);
                                }),
                            ),
                    );
                }
                card = card.child(grid).child(
                    components::button(&theme, "Cancel", ButtonVariant::Ghost).on_press(
                        "dialogs:150",
                        cx.listener(|view, _: &gpui::ClickEvent, _window, cx| {
                            view.close_dialog(cx);
                        }),
                    ),
                );
            }
            Dialog::ChooseEnvironment => {
                card = card.child(self.dialog_title("Environment"));
                let mut list = div().flex().flex_col().gap(px(2.0));
                list = list.child(
                    components::tab_button(
                        &theme,
                        "No environment",
                        self.active_environment.is_none(),
                        None,
                    )
                    .on_press(
                        "dialogs:167",
                        cx.listener(|view, _: &gpui::ClickEvent, _window, cx| {
                            view.set_active_environment(None, cx);
                            view.close_dialog(cx);
                        }),
                    ),
                );
                for environment in &self.environments {
                    let id = environment.id;
                    let name = environment.name.clone();
                    let selected = self.active_environment == Some(id);
                    let count = environment.variables.len();
                    list = list.child(div().flex().items_center().justify_between().child(
                        components::tab_button(&theme, name, selected, Some(count)).on_press(
                            "dialogs:180",
                            cx.listener(move |view, _: &gpui::ClickEvent, _window, cx| {
                                view.set_active_environment(Some(id), cx);
                                view.close_dialog(cx);
                            }),
                        ),
                    ));
                }
                card = card.child(list).child(
                    components::button(&theme, "Close", ButtonVariant::Ghost).on_press(
                        "dialogs:189",
                        cx.listener(|view, _: &gpui::ClickEvent, _window, cx| {
                            view.close_dialog(cx);
                        }),
                    ),
                );
            }
            Dialog::SaveRequest { .. } => {
                card = card.child(self.dialog_title("Save request to"));
                let mut targets: Vec<(uuid::Uuid, Option<uuid::Uuid>, String)> = Vec::new();
                for tree in &self.collections {
                    targets.push((
                        tree.collection.id,
                        None,
                        format!("{} (root)", tree.collection.name),
                    ));
                    for saved in tree.root.walk_requests() {
                        let _ = saved;
                    }
                    collect_folders(&tree.root, 0, &mut targets, tree.collection.id);
                }

                if targets.is_empty() {
                    card = card.child(components::empty_state(
                        &theme,
                        "No collections yet",
                        "Create a collection first, then save this request into it.",
                    ));
                } else {
                    let mut list = div().flex().flex_col().gap(px(2.0));
                    for (collection_id, folder_id, label) in targets {
                        list = list.child(
                            components::tab_button(&theme, label, false, None).on_press(
                                "dialogs:221",
                                cx.listener(move |view, _: &gpui::ClickEvent, _window, cx| {
                                    view.save_request_to(collection_id, folder_id, cx);
                                    view.close_dialog(cx);
                                }),
                            ),
                        );
                    }
                    card = card.child(list.into_any_element());
                }
                card = card.child(
                    components::button(&theme, "Cancel", ButtonVariant::Ghost).on_press(
                        "dialogs:232",
                        cx.listener(|view, _: &gpui::ClickEvent, _window, cx| {
                            view.close_dialog(cx);
                        }),
                    ),
                );
            }
            Dialog::Settings => {
                let timeout = self.timeout_seconds;
                let history_limit = self.history_limit;
                card = card.child(self.dialog_title("Settings"));

                let mut timeouts = div().flex().items_center().gap(px(2.0)).child(
                    div()
                        .w(px(120.0))
                        .child(components::field_label(&theme, "Request timeout")),
                );
                for seconds in [5u64, 15, 30, 60, 120] {
                    timeouts = timeouts.child(
                        components::tab_button(
                            &theme,
                            crate::app_view::timeout_label(seconds),
                            timeout == seconds,
                            None,
                        )
                        .on_press(
                            "dialogs:257",
                            cx.listener(move |view, _: &gpui::ClickEvent, _window, cx| {
                                view.set_timeout(seconds, cx);
                            }),
                        ),
                    );
                }

                let mut limits = div().flex().items_center().gap(px(2.0)).child(
                    div()
                        .w(px(120.0))
                        .child(components::field_label(&theme, "History kept")),
                );
                for limit in [200i64, 1000, 5000] {
                    limits = limits.child(
                        components::tab_button(
                            &theme,
                            limit.to_string(),
                            history_limit == limit,
                            None,
                        )
                        .on_press(
                            "dialogs:278",
                            cx.listener(move |view, _: &gpui::ClickEvent, _window, cx| {
                                view.set_history_limit(limit, cx);
                            }),
                        ),
                    );
                }

                let mut themes = div().flex().items_center().gap(px(2.0)).child(
                    div()
                        .w(px(120.0))
                        .child(components::field_label(&theme, "Theme")),
                );
                for mode in [ThemeMode::Dark, ThemeMode::Light] {
                    let label = if mode == ThemeMode::Dark {
                        "Dark"
                    } else {
                        "Light"
                    };
                    themes = themes.child(
                        components::tab_button(&theme, label, self.theme.mode == mode, None)
                            .on_press(
                                "dialogs:299",
                                cx.listener(move |view, _: &gpui::ClickEvent, _window, cx| {
                                    view.set_theme(mode, cx);
                                }),
                            ),
                    );
                }

                card = card
                    .child(timeouts)
                    .child(limits)
                    .child(themes)
                    .child(components::divider(&theme))
                    .child(
                        div()
                            .text_size(text::micro())
                            .text_color(theme.text_muted)
                            .child(format!(
                                "Database: {}",
                                self.database_path.clone().unwrap_or_else(|| "in memory".into())
                            )),
                    )
                    .child(
                        div()
                            .text_size(text::micro())
                            .text_color(theme.text_muted)
                            .child(
                                "Secret variables are stored in the local SQLite database and are not encrypted at rest.",
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_end()
                            .gap(space::sm())
                            .child(self.dialog_button(
                                cx,
                                "settings-close",
                                "Close",
                                ElyButtonVariant::Secondary,
                                |view, cx| view.close_dialog(cx),
                            )),
                    );
            }
        }

        let card = card.on_press(
            "dialogs:346",
            cx.listener(|_view, _: &gpui::ClickEvent, _window, cx| {
                // Keep the click on the card from reaching the backdrop.
                cx.stop_propagation();
            }),
        );

        Some(
            div()
                .absolute()
                .top(px(0.0))
                .left(px(0.0))
                .w_full()
                .h_full()
                .flex()
                .items_center()
                .justify_center()
                .bg(theme.overlay)
                .on_press(
                    "dialogs:362",
                    cx.listener(|view, _: &gpui::ClickEvent, _window, cx| {
                        view.close_dialog(cx);
                    }),
                )
                .child(card)
                .into_any_element(),
        )
    }

    fn dialog_title(&self, title: &str) -> impl IntoElement {
        let theme = self.theme;
        div()
            .text_size(text::title())
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .text_color(theme.text_primary)
            .child(title.to_string())
    }
}

fn collect_folders(
    node: &requestlab_models::CollectionNode,
    depth: usize,
    out: &mut Vec<(uuid::Uuid, Option<uuid::Uuid>, String)>,
    collection_id: uuid::Uuid,
) {
    for child in &node.children {
        if let Some(folder) = &child.folder {
            out.push((
                collection_id,
                Some(folder.id),
                format!("{}{}", "  ".repeat(depth + 1), folder.name),
            ));
            collect_folders(child, depth + 1, out, collection_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use requestlab_models::{build_tree, Collection};
    use uuid::Uuid;

    #[test]
    fn folder_targets_are_indented_and_nested() {
        let collection = Collection {
            id: Uuid::new_v4(),
            name: "API".into(),
            description: None,
            created_at: "t".into(),
            updated_at: "t".into(),
        };
        let root_folder = requestlab_models::Folder {
            id: Uuid::new_v4(),
            collection_id: collection.id,
            parent_id: None,
            name: "Users".into(),
            sort_order: 0,
            created_at: "t".into(),
            updated_at: "t".into(),
        };
        let child = requestlab_models::Folder {
            id: Uuid::new_v4(),
            collection_id: collection.id,
            parent_id: Some(root_folder.id),
            name: "Admin".into(),
            sort_order: 0,
            created_at: "t".into(),
            updated_at: "t".into(),
        };
        let tree = build_tree(collection.clone(), vec![root_folder, child], vec![]);
        let mut targets = Vec::new();
        collect_folders(&tree.root, 0, &mut targets, collection.id);
        assert_eq!(targets.len(), 2);
        assert!(targets[1].2.trim().ends_with("Admin"));
        assert!(targets[1].2.starts_with("  "));
    }
}
