//! Sidebar: collections tree, request history and environment variables.

use ely_gpui_component::primitives::IconName;
use gpui::prelude::*;
use gpui::{div, px, Context, IntoElement, SharedString};
use requestlab_core::EnvironmentError;
use requestlab_models::{
    CollectionNode, CollectionTree, EnvironmentVariable, HistoryStatusFilter, HttpMethod,
};
use uuid::Uuid;

use crate::app_view::{AppView, DestructiveAction, Dialog, PromptAction, SidebarSection};
use crate::components::InteractiveExt;
use crate::components::{self, ButtonVariant, ToastKind};
use crate::theme::{radius, space, text, SIDEBAR_WIDTH};

impl AppView {
    pub(crate) fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let header_action = match self.sidebar {
            SidebarSection::Collections => Some(("New collection", "New collection")),
            SidebarSection::Environments => Some(("New environment", "New environment")),
            SidebarSection::History => None,
        };

        let mut header = div()
            .flex()
            .items_center()
            .justify_between()
            .h(px(34.0))
            .px(space::md())
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .text_size(text::small())
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(theme.text_secondary)
                    .child(self.sidebar.label()),
            );

        if let Some((_, tooltip)) = header_action {
            header = header.child(components::icon_button(&theme, IconName::Plus).on_press(
                "sidebar:43",
                cx.listener(move |view, _: &gpui::ClickEvent, _window, cx| {
                    let action = match view.sidebar {
                        SidebarSection::Collections => PromptAction::NewCollection,
                        _ => PromptAction::NewEnvironment,
                    };
                    view.open_dialog(
                        Dialog::Prompt {
                            title: tooltip.to_string(),
                            hint: if view.sidebar == SidebarSection::Collections {
                                "Collection name"
                            } else {
                                "Environment name"
                            }
                            .to_string(),
                            action,
                            initial: String::new(),
                        },
                        cx,
                    );
                }),
            ));
        }

        let body: gpui::AnyElement = match self.sidebar {
            SidebarSection::Collections => self.render_collections(cx).into_any_element(),
            SidebarSection::History => self.render_history(cx).into_any_element(),
            SidebarSection::Environments => self.render_environments(cx).into_any_element(),
        };

        div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .w(SIDEBAR_WIDTH)
            .h_full()
            .bg(theme.sidebar)
            .border_r_1()
            .border_color(theme.border)
            .on_mouse_down(
                gpui::MouseButton::Right,
                cx.listener(|view, event: &gpui::MouseDownEvent, _window, cx| {
                    let items = view.sidebar_menu_items();
                    view.open_context_menu(event.position, "Sidebar", items, cx);
                    cx.stop_propagation();
                }),
            )
            .child(header)
            .child(body)
    }

    fn render_collections(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let mut list = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.0))
            .id("sidebar-scroll-1")
            .overflow_y_scroll();

        if self.collections.is_empty() {
            list = list.child(components::empty_state(
                &theme,
                "No collections",
                "Create a collection to save requests and folders.",
            ));
        }

        for tree in &self.collections {
            list = list.child(self.render_collection_tree(tree, cx));
        }

        list.child(
            div()
                .p(space::sm())
                .text_size(text::micro())
                .text_color(theme.text_muted)
                .child("Click a request to open it. Use the row actions to rename, duplicate or delete."),
        )
    }

    fn render_collection_tree(
        &self,
        tree: &CollectionTree,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = self.theme;
        let collection_id = tree.collection.id;
        let expanded = self.expanded_collections.contains(&collection_id);
        let name = tree.collection.name.clone();
        let name_new_folder = name.clone();
        let name_rename = name.clone();
        let name_duplicate = name.clone();
        let name_delete = name.clone();
        let count = tree.all_requests().len();

        let mut block = div().flex().flex_col();

        block = block.child(
            div()
                .flex()
                .items_center()
                .gap(space::xs())
                .h(px(26.0))
                .px(space::sm())
                .hover(move |style| style.bg(theme.hover))
                .child(
                    div().w(px(12.0)).flex_shrink_0().child(components::icon(
                        if expanded {
                            IconName::ChevronDown
                        } else {
                            IconName::ChevronRight
                        },
                        theme.text_secondary,
                    )),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .truncate()
                        .text_size(text::small())
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(theme.text_primary)
                        .child(name.clone()),
                )
                .child(
                    div()
                        .text_size(text::micro())
                        .text_color(theme.text_muted)
                        .child(count.to_string()),
                )
                .child(components::icon_button(&theme, IconName::Plus).on_press("sidebar:167", cx.listener(
                    move |view, _: &gpui::ClickEvent, _window, cx| {
                        view.open_dialog(
                            Dialog::Prompt {
                                title: format!("New folder in {name_new_folder}"),
                                hint: "Folder name".to_string(),
                                action: PromptAction::NewFolder {
                                    collection_id,
                                    parent_id: None,
                                },
                                initial: String::new(),
                            },
                            cx,
                        );
                    },
                )))
                .child(components::icon_button(&theme, IconName::Pencil).on_press("sidebar:183", cx.listener(
                    move |view, _: &gpui::ClickEvent, _window, cx| {
                        view.open_dialog(
                            Dialog::Prompt {
                                title: "Rename collection".to_string(),
                                hint: "Collection name".to_string(),
                                action: PromptAction::RenameCollection(collection_id),
                                initial: name_rename.clone(),
                            },
                            cx,
                        );
                    },
                )))
                .child(components::icon_button(&theme, IconName::Copy).on_press("sidebar:196", cx.listener(
                    move |view, _: &gpui::ClickEvent, _window, cx| {
                        view.open_dialog(
                            Dialog::Prompt {
                                title: "Duplicate collection".to_string(),
                                hint: "New name".to_string(),
                                action: PromptAction::DuplicateCollection(collection_id),
                                initial: format!("{name_duplicate} copy"),
                            },
                            cx,
                        );
                    },
                )))
                .child(components::icon_button(&theme, IconName::X).on_press("sidebar:209", cx.listener(
                    move |view, _: &gpui::ClickEvent, _window, cx| {
                        view.open_dialog(
                            Dialog::Confirm {
                                title: "Delete collection".to_string(),
                                message: format!(
                                    "Delete \"{name_delete}\" with all of its folders and saved requests? This cannot be undone."
                                ),
                                action: DestructiveAction::DeleteCollection(collection_id),
                            },
                            cx,
                        );
                    },
                )))
                .on_press("sidebar:223", cx.listener(move |view, _: &gpui::ClickEvent, _window, cx| {
                    if !view.expanded_collections.remove(&collection_id) {
                        view.expanded_collections.insert(collection_id);
                    }
                    cx.notify();
                })),
        );

        if expanded {
            block = block.child(self.render_folder_children(tree, &tree.root, 1, cx));
        }

        block
    }

    fn render_folder_children(
        &self,
        tree: &CollectionTree,
        node: &CollectionNode,
        depth: usize,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = self.theme;
        let collection_id = tree.collection.id;
        let mut block = div().flex().flex_col();

        for request in &node.requests {
            let saved_id = request.id;
            let name: SharedString = request.display_name().into();
            let method = request.request.method;
            let folder_id = request.folder_id;
            block = block.child(
                div()
                    .flex()
                    .items_center()
                    .gap(space::xs())
                    .h(px(24.0))
                    .pl(px(8.0 + depth as f32 * 12.0))
                    .pr(space::sm())
                    .hover(move |style| style.bg(theme.hover))
                    .cursor(gpui::CursorStyle::PointingHand)
                    .child(
                        components::method_chip(&theme, method)
                            .min_w(px(38.0))
                            .h(px(16.0)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .truncate()
                            .text_size(text::small())
                            .text_color(theme.text_secondary)
                            .child(name.clone()),
                    )
                    .child(components::icon_button(&theme, IconName::Copy).on_press(
                        format!("sidebar:278:{}", saved_id),
                        cx.listener(move |view, _: &gpui::ClickEvent, _window, cx| {
                            view.duplicate_saved_request(saved_id, cx);
                        }),
                    ))
                    .child(components::icon_button(&theme, IconName::X).on_press(
                        format!("sidebar:283:{}", saved_id),
                        cx.listener(move |view, _: &gpui::ClickEvent, _window, cx| {
                            view.open_dialog(
                                Dialog::Confirm {
                                    title: "Delete request".to_string(),
                                    message: format!("Delete \"{name}\" from this collection?"),
                                    action: DestructiveAction::DeleteSavedRequest(saved_id),
                                },
                                cx,
                            );
                        }),
                    ))
                    .on_press(
                        format!("sidebar:295:{}", saved_id),
                        cx.listener(move |view, _: &gpui::ClickEvent, _window, cx| {
                            view.open_saved_request(saved_id, collection_id, folder_id, cx);
                        }),
                    ),
            );
        }

        for child in &node.children {
            let folder_id = child.folder.as_ref().map(|folder| folder.id);
            let folder_name = child.name().to_string();
            let folder_new = folder_name.clone();
            let folder_rename = folder_name.clone();
            let folder_delete = folder_name.clone();
            let expanded = folder_id.is_some_and(|id| self.expanded_folders.contains(&id));
            let request_count = child.request_count();

            let mut row = div()
                .flex()
                .items_center()
                .gap(space::xs())
                .h(px(25.0))
                .pl(px(8.0 + depth as f32 * 12.0))
                .pr(space::sm())
                .hover(move |style| style.bg(theme.hover))
                .child(div().w(px(12.0)).child(components::icon(
                    if expanded {
                        IconName::ChevronDown
                    } else {
                        IconName::ChevronRight
                    },
                    theme.text_secondary,
                )))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .truncate()
                        .text_size(text::small())
                        .text_color(theme.text_primary)
                        .child(folder_name.clone()),
                )
                .child(
                    div()
                        .text_size(text::micro())
                        .text_color(theme.text_muted)
                        .child(request_count.to_string()),
                )
                .id(gpui::ElementId::from(format!(
                    "folder-row:{collection_id}:{folder_name}"
                )));

            if let Some(folder_id) = folder_id {
                row = row
                    .child(components::icon_button(&theme, IconName::Plus).on_press(
                        format!("sidebar:346:{depth}:{folder_id:?}"),
                        cx.listener(move |view, _: &gpui::ClickEvent, _window, cx| {
                            view.open_dialog(
                                Dialog::Prompt {
                                    title: format!("New folder in {folder_new}"),
                                    hint: "Folder name".to_string(),
                                    action: PromptAction::NewFolder {
                                        collection_id,
                                        parent_id: Some(folder_id),
                                    },
                                    initial: String::new(),
                                },
                                cx,
                            );
                        }),
                    ))
                    .child(components::icon_button(&theme, IconName::Pencil).on_press(
                        format!("sidebar:362:{depth}:{folder_id:?}"),
                        cx.listener(move |view, _: &gpui::ClickEvent, _window, cx| {
                            view.open_dialog(
                                Dialog::Prompt {
                                    title: "Rename folder".to_string(),
                                    hint: "Folder name".to_string(),
                                    action: PromptAction::RenameFolder(folder_id),
                                    initial: folder_rename.clone(),
                                },
                                cx,
                            );
                        }),
                    ))
                    .child(components::icon_button(&theme, IconName::X).on_press(
                        format!("sidebar:375:{depth}:{folder_id:?}"),
                        cx.listener(move |view, _: &gpui::ClickEvent, _window, cx| {
                            view.open_dialog(
                                Dialog::Confirm {
                                    title: "Delete folder".to_string(),
                                    message: format!(
                                        "Delete \"{folder_delete}\" and everything inside it?"
                                    ),
                                    action: DestructiveAction::DeleteFolder(folder_id),
                                },
                                cx,
                            );
                        }),
                    ))
                    .on_click(cx.listener(move |view, _: &gpui::ClickEvent, _window, cx| {
                        if !view.expanded_folders.remove(&folder_id) {
                            view.expanded_folders.insert(folder_id);
                        }
                        cx.notify();
                    }));
            }

            block = block.child(row);
            if expanded {
                block = block.child(self.render_folder_children(tree, child, depth + 1, cx));
            }
        }

        block
    }

    fn render_history(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let mut filters = div().flex().flex_col().gap(px(2.0)).p(space::sm());

        filters = filters.child(
            div()
                .h(px(24.0))
                .px(space::xs())
                .rounded(radius::sm())
                .bg(theme.surface)
                .border_1()
                .border_color(theme.border)
                .font_family(theme.mono_family())
                .text_size(text::small())
                .child(self.history_search.clone()),
        );

        let mut methods = div().flex().flex_wrap().gap(px(2.0));
        methods = methods.child(
            components::tab_button(&theme, "All", self.history_method.is_none(), None).on_press(
                "sidebar:425",
                cx.listener(|view, _: &gpui::ClickEvent, _window, cx| {
                    view.history_method = None;
                    view.reload_history(cx);
                }),
            ),
        );
        for method in HttpMethod::ALL {
            let selected = self.history_method == Some(method);
            methods = methods.child(
                components::tab_button(&theme, method.as_str(), selected, None).on_press(
                    format!("sidebar:435:{}", method.as_str()),
                    cx.listener(move |view, _: &gpui::ClickEvent, _window, cx| {
                        view.history_method = if view.history_method == Some(method) {
                            None
                        } else {
                            Some(method)
                        };
                        view.reload_history(cx);
                    }),
                ),
            );
        }
        filters = filters.child(methods);

        let mut statuses = div().flex().flex_wrap().gap(px(2.0));
        for status in HistoryStatusFilter::ALL {
            let selected = self.history_status == status;
            let label = match status {
                HistoryStatusFilter::All => "All",
                HistoryStatusFilter::Success => "2xx",
                HistoryStatusFilter::Redirect => "3xx",
                HistoryStatusFilter::ClientError => "4xx",
                HistoryStatusFilter::ServerError => "5xx",
                HistoryStatusFilter::Failed => "Failed",
            };
            statuses = statuses.child(
                components::tab_button(&theme, label, selected, None).on_press(
                    format!("sidebar:461:{}", label),
                    cx.listener(move |view, _: &gpui::ClickEvent, _window, cx| {
                        view.history_status = status;
                        view.reload_history(cx);
                    }),
                ),
            );
        }
        filters = filters.child(statuses);
        filters = filters.child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_size(text::micro())
                        .text_color(theme.text_muted)
                        .child(format!("{} entries", self.history.len())),
                )
                .child(
                    components::button(&theme, "Search", ButtonVariant::Ghost)
                        .h(px(20.0))
                        .on_press(
                            "sidebar:484",
                            cx.listener(|view, _: &gpui::ClickEvent, _window, cx| {
                                view.reload_history(cx);
                            }),
                        ),
                ),
        );

        let mut list = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.0))
            .id("sidebar-scroll-2")
            .overflow_y_scroll();
        if self.history.is_empty() {
            list = list.child(components::empty_state(
                &theme,
                "No history",
                "Completed requests appear here automatically.",
            ));
        }
        for entry in &self.history {
            let entry_id = entry.id;
            let request = entry.request.clone();
            let method = entry.method;
            let label: SharedString = entry.label().into();
            let status = entry.status_code;
            let duration = entry
                .duration_ms
                .map(requestlab_models::human_duration)
                .unwrap_or_else(|| "—".to_string());
            let created = entry.created_at.clone();
            list = list.child(
                div()
                    .flex()
                    .items_center()
                    .gap(space::xs())
                    .px(space::sm())
                    .h(px(30.0))
                    .border_b_1()
                    .border_color(theme.border)
                    .hover(move |style| style.bg(theme.hover))
                    .cursor(gpui::CursorStyle::PointingHand)
                    .child(
                        components::method_chip(&theme, method)
                            .min_w(px(38.0))
                            .h(px(16.0)),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w(px(0.0))
                            .child(
                                div()
                                    .truncate()
                                    .text_size(text::small())
                                    .text_color(theme.text_secondary)
                                    .child(label),
                            )
                            .child(
                                div()
                                    .text_size(text::micro())
                                    .text_color(theme.text_muted)
                                    .child(format!(
                                        "{duration} · {}",
                                        created.get(11..19).unwrap_or("")
                                    )),
                            ),
                    )
                    .child(components::status_chip(
                        &theme,
                        status,
                        status
                            .map(|code| code.to_string())
                            .unwrap_or_else(|| "ERR".to_string()),
                    ))
                    .child(components::icon_button(&theme, IconName::X).on_press(
                        format!("sidebar:560:{}", entry_id),
                        cx.listener(move |view, _: &gpui::ClickEvent, _window, cx| {
                            view.open_dialog(
                                Dialog::Confirm {
                                    title: "Delete history entry".to_string(),
                                    message: "Remove this entry from the request history?"
                                        .to_string(),
                                    action: DestructiveAction::DeleteHistoryEntry(entry_id),
                                },
                                cx,
                            );
                        }),
                    ))
                    .on_press(
                        format!("sidebar:573:{}", entry_id),
                        cx.listener(move |view, _: &gpui::ClickEvent, _window, cx| {
                            if let Some(tab) = view.tabs.get_mut(view.active_tab) {
                                tab.editor.load(&request, cx);
                                tab.editor.saved = None;
                            }
                            view.sidebar = SidebarSection::Collections;
                            view.show_toast("Loaded request from history", ToastKind::Info);
                            cx.notify();
                        }),
                    ),
            );
        }

        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h(px(0.0))
            .child(filters)
            .child(list)
            .child(div().p(space::sm()).child(
                components::button(&theme, "Clear history", ButtonVariant::Ghost).on_press(
                    "sidebar:593",
                    cx.listener(|view, _: &gpui::ClickEvent, _window, cx| {
                        view.open_dialog(
                            Dialog::Confirm {
                                title: "Clear history".to_string(),
                                message: "Delete every recorded request attempt?".to_string(),
                                action: DestructiveAction::ClearHistory,
                            },
                            cx,
                        );
                    }),
                ),
            ))
    }

    fn render_environments(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let mut list = div().flex().flex_col().flex_1().min_h(px(0.0));

        let mut env_list = div().flex().flex_col();
        if self.environments.is_empty() {
            env_list = env_list.child(components::empty_state(
                &theme,
                "No environments",
                "Create an environment to define reusable variables.",
            ));
        }
        for environment in &self.environments {
            let id = environment.id;
            let name = environment.name.clone();
            let env_rename = name.clone();
            let env_duplicate = name.clone();
            let env_delete = name.clone();
            let active = self.active_environment == Some(id);
            let selected = self.env_selected == Some(id);
            let variable_count = environment.variables.len();
            env_list = env_list.child(
                div()
                    .flex()
                    .items_center()
                    .gap(space::xs())
                    .h(px(28.0))
                    .px(space::sm())
                    .when(selected, |style| style.bg(theme.active))
                    .hover(move |style| style.bg(theme.hover))
                    .cursor(gpui::CursorStyle::PointingHand)
                    .child(div().size(px(6.0)).rounded(px(3.0)).bg(if active {
                        theme.success
                    } else {
                        theme.border_strong
                    }))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .truncate()
                            .text_size(text::small())
                            .text_color(theme.text_primary)
                            .child(name.clone()),
                    )
                    .child(
                        div()
                            .text_size(text::micro())
                            .text_color(theme.text_muted)
                            .child(variable_count.to_string()),
                    )
                    .child(components::icon_button(&theme, IconName::Check).on_press(
                        format!("sidebar:659:{}", id),
                        cx.listener(move |view, _: &gpui::ClickEvent, _window, cx| {
                            view.set_active_environment(Some(id), cx);
                        }),
                    ))
                    .child(components::icon_button(&theme, IconName::Pencil).on_press(
                        format!("sidebar:664:{}", id),
                        cx.listener(move |view, _: &gpui::ClickEvent, _window, cx| {
                            view.open_dialog(
                                Dialog::Prompt {
                                    title: "Rename environment".to_string(),
                                    hint: "Environment name".to_string(),
                                    action: PromptAction::RenameEnvironment(id),
                                    initial: env_rename.clone(),
                                },
                                cx,
                            );
                        }),
                    ))
                    .child(components::icon_button(&theme, IconName::Copy).on_press(
                        format!("sidebar:677:{}", id),
                        cx.listener(move |view, _: &gpui::ClickEvent, _window, cx| {
                            view.open_dialog(
                                Dialog::Prompt {
                                    title: "Duplicate environment".to_string(),
                                    hint: "New name".to_string(),
                                    action: PromptAction::DuplicateEnvironment(id),
                                    initial: format!("{env_duplicate} copy"),
                                },
                                cx,
                            );
                        }),
                    ))
                    .child(components::icon_button(&theme, IconName::X).on_press(
                        format!("sidebar:690:{}", id),
                        cx.listener(move |view, _: &gpui::ClickEvent, _window, cx| {
                            view.open_dialog(
                                Dialog::Confirm {
                                    title: "Delete environment".to_string(),
                                    message: format!("Delete \"{env_delete}\" and its variables?"),
                                    action: DestructiveAction::DeleteEnvironment(id),
                                },
                                cx,
                            );
                        }),
                    ))
                    .on_press(
                        format!("sidebar:702:{}", id),
                        cx.listener(move |view, _: &gpui::ClickEvent, _window, cx| {
                            view.load_environment(id, cx);
                            cx.notify();
                        }),
                    ),
            );
        }

        let mut variables = div().flex().flex_col().gap(px(2.0)).p(space::sm());
        if let Some(selected) = self.env_selected {
            variables = variables.child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(text::micro())
                            .text_color(theme.text_muted)
                            .child("Variables"),
                    )
                    .child(
                        components::button(&theme, "Save", ButtonVariant::Secondary)
                            .h(px(22.0))
                            .on_press(
                                "sidebar:725",
                                cx.listener(move |view, _: &gpui::ClickEvent, _window, cx| {
                                    view.save_environment_variables(selected, cx);
                                }),
                            ),
                    ),
            );

            for row in &self.env_rows {
                let row_id = row.id;
                let enabled = row.enabled;
                let secret = row.secret;
                variables = variables.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(3.0))
                        .child(components::toggle(&theme, enabled).on_press(
                            format!("sidebar:742:{}", row_id),
                            cx.listener(move |view, _: &gpui::ClickEvent, _window, cx| {
                                if let Some(row) = view.env_rows.iter_mut().find(|r| r.id == row_id)
                                {
                                    row.enabled = !row.enabled;
                                }
                                cx.notify();
                            }),
                        ))
                        .child(
                            div()
                                .w(px(104.0))
                                .h(px(22.0))
                                .px(space::xs())
                                .rounded(px(3.0))
                                .bg(theme.surface)
                                .border_1()
                                .border_color(theme.border)
                                .font_family(theme.mono_family())
                                .text_size(text::micro())
                                .child(row.key.clone()),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.0))
                                .h(px(22.0))
                                .px(space::xs())
                                .rounded(px(3.0))
                                .bg(theme.surface)
                                .border_1()
                                .border_color(theme.border)
                                .font_family(theme.mono_family())
                                .text_size(text::micro())
                                .child(row.value.clone()),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_center()
                                .size(px(20.0))
                                .rounded(px(3.0))
                                .text_size(px(9.0))
                                .cursor(gpui::CursorStyle::PointingHand)
                                .text_color(if secret {
                                    theme.warning
                                } else {
                                    theme.text_muted
                                })
                                .bg(if secret {
                                    crate::theme::with_alpha(theme.warning, 0.16)
                                } else {
                                    theme.surface
                                })
                                .child("★")
                                .on_press(
                                    format!("sidebar:798:{}", row_id),
                                    cx.listener(move |view, _: &gpui::ClickEvent, _window, cx| {
                                        if let Some(row) =
                                            view.env_rows.iter_mut().find(|r| r.id == row_id)
                                        {
                                            row.secret = !row.secret;
                                        }
                                        cx.notify();
                                    }),
                                ),
                        )
                        .child(components::icon_button(&theme, IconName::X).on_press(
                            format!("sidebar:809:{}", row_id),
                            cx.listener(move |view, _: &gpui::ClickEvent, _window, cx| {
                                view.env_rows.retain(|row| row.id != row_id);
                                cx.notify();
                            }),
                        )),
                );
            }

            variables = variables.child(
                div()
                    .flex()
                    .items_center()
                    .gap(space::sm())
                    .pt(space::xs())
                    .child(
                        components::button(&theme, "＋ Add", ButtonVariant::Ghost)
                            .h(px(22.0))
                            .on_press(
                                "sidebar:827",
                                cx.listener(|view, _: &gpui::ClickEvent, _window, cx| {
                                    view.push_env_row(cx);
                                    cx.notify();
                                }),
                            ),
                    )
                    .child(
                        div()
                            .text_size(text::micro())
                            .text_color(theme.text_muted)
                            .child("★ marks a secret"),
                    ),
            );
        } else {
            variables = variables.child(components::empty_state(
                &theme,
                "No environment selected",
                "Pick an environment above to edit its variables.",
            ));
        }

        list = list.child(env_list).child(variables);
        list
    }

    // -------------------------------------------------------------- mutations

    pub(crate) fn set_active_environment(&mut self, id: Option<Uuid>, cx: &mut Context<Self>) {
        let service = self.service.clone();
        self.spawn_core(
            cx,
            async move {
                service.environments().set_active(id).await?;
                Ok::<_, EnvironmentError>(())
            },
            move |this, result, cx| {
                match result {
                    Ok(()) => {
                        this.active_environment = id;
                        if let Some(id) = id {
                            this.load_environment(id, cx);
                        }
                        this.show_toast("Active environment updated", ToastKind::Success);
                    }
                    Err(error) => this.show_toast(format!("{error}"), ToastKind::Error),
                }
                cx.notify();
            },
        );
    }

    pub(crate) fn save_environment_variables(&mut self, id: Uuid, cx: &mut Context<Self>) {
        let variables: Vec<EnvironmentVariable> = self
            .env_rows
            .iter()
            .filter(|row| !row.key.read(cx).content().trim().is_empty())
            .map(|row| EnvironmentVariable {
                id: row.id,
                key: row.key.read(cx).content().trim().to_string(),
                value: row.value.read(cx).content().to_string(),
                enabled: row.enabled,
                is_secret: row.secret,
            })
            .collect();

        let service = self.service.clone();
        self.spawn_core(
            cx,
            async move {
                service
                    .environments()
                    .save_variables(id, &variables)
                    .await?;
                Ok::<_, EnvironmentError>(())
            },
            move |this, result, cx| {
                match result {
                    Ok(()) => {
                        this.show_toast("Environment saved", ToastKind::Success);
                        this.reload_environments(cx);
                    }
                    Err(error) => this.show_toast(format!("{error}"), ToastKind::Error),
                }
                cx.notify();
            },
        );
    }

    pub(crate) fn duplicate_saved_request(&mut self, id: Uuid, cx: &mut Context<Self>) {
        let service = self.service.clone();
        self.spawn_core(
            cx,
            async move { service.collections().duplicate_saved_request(id).await },
            |this, result, cx| {
                match result {
                    Ok(saved) => {
                        this.show_toast(format!("Duplicated {}", saved.name), ToastKind::Success);
                        this.reload_collections(cx);
                    }
                    Err(error) => this.show_toast(format!("{error}"), ToastKind::Error),
                }
                cx.notify();
            },
        );
    }

    /// Apply the dialog that is currently open.
    pub(crate) fn execute_destructive(
        &mut self,
        action: DestructiveAction,
        cx: &mut Context<Self>,
    ) {
        self.dialog = None;
        let service = self.service.clone();
        match action {
            DestructiveAction::DeleteCollection(id) => {
                self.spawn_core(
                    cx,
                    async move {
                        service.collections().delete_collection(id).await?;
                        Ok::<_, requestlab_core::CollectionError>(())
                    },
                    |this, result, cx| {
                        this.finish_simple(
                            result.map_err(|e| e.to_string()),
                            "Collection deleted",
                            cx,
                        );
                        this.reload_collections(cx);
                    },
                );
            }
            DestructiveAction::DeleteFolder(id) => {
                self.spawn_core(
                    cx,
                    async move {
                        service.collections().delete_folder(id).await?;
                        Ok::<_, requestlab_core::CollectionError>(())
                    },
                    |this, result, cx| {
                        this.finish_simple(result.map_err(|e| e.to_string()), "Folder deleted", cx);
                        this.reload_collections(cx);
                    },
                );
            }
            DestructiveAction::DeleteSavedRequest(id) => {
                self.spawn_core(
                    cx,
                    async move {
                        service.collections().delete_saved_request(id).await?;
                        Ok::<_, requestlab_core::CollectionError>(())
                    },
                    |this, result, cx| {
                        this.finish_simple(
                            result.map_err(|e| e.to_string()),
                            "Request deleted",
                            cx,
                        );
                        this.reload_collections(cx);
                    },
                );
            }
            DestructiveAction::DeleteHistoryEntry(id) => {
                self.spawn_core(
                    cx,
                    async move { service.delete_history_entry(id).await },
                    |this, result, cx| {
                        this.finish_simple(
                            result.map_err(|e| e.to_string()),
                            "History entry removed",
                            cx,
                        );
                        this.reload_history(cx);
                    },
                );
            }
            DestructiveAction::ClearHistory => {
                self.spawn_core(
                    cx,
                    async move { service.clear_history().await },
                    |this, result, cx| {
                        this.finish_simple(
                            result.map(|_| ()).map_err(|e| e.to_string()),
                            "History cleared",
                            cx,
                        );
                        this.reload_history(cx);
                    },
                );
            }
            DestructiveAction::DeleteEnvironment(id) => {
                self.spawn_core(
                    cx,
                    async move {
                        service.environments().delete(id).await?;
                        Ok::<_, EnvironmentError>(())
                    },
                    |this, result, cx| {
                        this.finish_simple(
                            result.map_err(|e| e.to_string()),
                            "Environment deleted",
                            cx,
                        );
                        this.env_selected = None;
                        this.reload_environments(cx);
                    },
                );
            }
        }
        cx.notify();
    }

    fn finish_simple(&mut self, result: Result<(), String>, success: &str, cx: &mut Context<Self>) {
        match result {
            Ok(()) => self.show_toast(success, ToastKind::Success),
            Err(error) => self.show_toast(error, ToastKind::Error),
        }
        cx.notify();
    }

    /// Apply a prompt dialog with the text the user typed.
    pub(crate) fn execute_prompt(&mut self, action: PromptAction, cx: &mut Context<Self>) {
        let value = self.dialog_input.read(cx).content().trim().to_string();
        self.dialog = None;
        if value.is_empty() {
            self.show_toast("A name is required", ToastKind::Error);
            cx.notify();
            return;
        }
        let service = self.service.clone();
        match action {
            PromptAction::NewCollection => {
                self.spawn_core(
                    cx,
                    async move { service.collections().create_collection(&value).await },
                    |this, result, cx| {
                        match result {
                            Ok(collection) => {
                                this.expanded_collections.insert(collection.id);
                                this.toast = Some((
                                    format!("Created {}", collection.name),
                                    ToastKind::Success,
                                ));
                                this.reload_collections(cx);
                            }
                            Err(error) => this.toast = Some((format!("{error}"), ToastKind::Error)),
                        }
                        cx.notify();
                    },
                );
            }
            PromptAction::NewEnvironment => {
                self.spawn_core(
                    cx,
                    async move { service.environments().create(&value).await },
                    |this, result, cx| {
                        match result {
                            Ok(environment) => {
                                this.toast = Some((
                                    format!("Created {}", environment.name),
                                    ToastKind::Success,
                                ));
                                this.reload_environments(cx);
                            }
                            Err(error) => this.toast = Some((format!("{error}"), ToastKind::Error)),
                        }
                        cx.notify();
                    },
                );
            }
            PromptAction::RenameCollection(id) => {
                self.spawn_core(
                    cx,
                    async move {
                        service.collections().rename_collection(id, &value).await?;
                        Ok::<_, requestlab_core::CollectionError>(())
                    },
                    |this, result, cx| {
                        this.finish_simple(
                            result.map_err(|e| e.to_string()),
                            "Collection renamed",
                            cx,
                        );
                        this.reload_collections(cx);
                    },
                );
            }
            PromptAction::NewFolder {
                collection_id,
                parent_id,
            } => {
                self.spawn_core(
                    cx,
                    async move {
                        let folder = service
                            .collections()
                            .create_folder(collection_id, parent_id, &value)
                            .await?;
                        Ok::<_, requestlab_core::CollectionError>(folder)
                    },
                    |this, result, cx| {
                        match result {
                            Ok(folder) => {
                                this.expanded_folders.insert(folder.id);
                                if let Some(parent) = folder.parent_id {
                                    this.expanded_folders.insert(parent);
                                }
                                this.toast =
                                    Some(("Folder created".to_string(), ToastKind::Success));
                                this.reload_collections(cx);
                            }
                            Err(error) => this.toast = Some((format!("{error}"), ToastKind::Error)),
                        }
                        cx.notify();
                    },
                );
            }
            PromptAction::RenameFolder(id) => {
                self.spawn_core(
                    cx,
                    async move {
                        service.collections().rename_folder(id, &value).await?;
                        Ok::<_, requestlab_core::CollectionError>(())
                    },
                    |this, result, cx| {
                        this.finish_simple(result.map_err(|e| e.to_string()), "Folder renamed", cx);
                        this.reload_collections(cx);
                    },
                );
            }
            PromptAction::RenameSavedRequest(id) => {
                self.spawn_core(
                    cx,
                    async move {
                        let existing = service.collections().saved_request(id).await?;
                        service
                            .collections()
                            .update_saved_request(id, &value, &existing.request)
                            .await?;
                        Ok::<_, requestlab_core::CollectionError>(())
                    },
                    |this, result, cx| {
                        this.finish_simple(
                            result.map_err(|e| e.to_string()),
                            "Request renamed",
                            cx,
                        );
                        this.reload_collections(cx);
                    },
                );
            }
            PromptAction::RenameEnvironment(id) => {
                self.spawn_core(
                    cx,
                    async move {
                        service.environments().rename(id, &value).await?;
                        Ok::<_, EnvironmentError>(())
                    },
                    |this, result, cx| {
                        this.finish_simple(
                            result.map_err(|e| e.to_string()),
                            "Environment renamed",
                            cx,
                        );
                        this.reload_environments(cx);
                    },
                );
            }
            PromptAction::DuplicateEnvironment(id) => {
                self.spawn_core(
                    cx,
                    async move {
                        service.environments().duplicate(id, &value).await?;
                        Ok::<_, EnvironmentError>(())
                    },
                    |this, result, cx| {
                        this.finish_simple(
                            result.map_err(|e| e.to_string()),
                            "Environment duplicated",
                            cx,
                        );
                        this.reload_environments(cx);
                    },
                );
            }
            PromptAction::DuplicateCollection(id) => {
                self.spawn_core(
                    cx,
                    async move {
                        service
                            .collections()
                            .duplicate_collection(id, &value)
                            .await?;
                        Ok::<_, requestlab_core::CollectionError>(())
                    },
                    |this, result, cx| {
                        this.finish_simple(
                            result.map_err(|e| e.to_string()),
                            "Collection duplicated",
                            cx,
                        );
                        this.reload_collections(cx);
                    },
                );
            }
        }
        cx.notify();
    }
}
