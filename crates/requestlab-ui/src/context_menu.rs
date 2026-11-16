//! Right-click context menus.
//!
//! Every item maps to an action the application already implements; a menu never
//! lists something it cannot do. The menu itself is a positioned overlay rendered
//! above the workspace, and a click anywhere else dismisses it.

use gpui::prelude::*;
use gpui::{div, px, AnyElement, ClipboardItem, Context, IntoElement, Pixels, Point};

use crate::app_view::{AppView, Dialog, PromptAction};
use crate::components::InteractiveExt;
use crate::theme::{radius, space, text};

/// What a menu item does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    NewTab,
    CloseTab(usize),
    Settings,
    ToggleTheme,
    ChooseMethod,
    SaveRequest,
    NewCollection,
    NewEnvironment,
    ReloadCollections,
    CopyResponse,
    ClearResponse,
    CloseMenu,
}

/// One row in a context menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MenuItem {
    pub label: &'static str,
    pub action: MenuAction,
    pub destructive: bool,
}

impl MenuItem {
    pub fn new(label: &'static str, action: MenuAction) -> Self {
        Self {
            label,
            action,
            destructive: false,
        }
    }

    pub fn destructive(label: &'static str, action: MenuAction) -> Self {
        Self {
            label,
            action,
            destructive: true,
        }
    }
}

/// An open context menu.
#[derive(Debug, Clone, PartialEq)]
pub struct ContextMenu {
    pub title: String,
    pub position: Point<Pixels>,
    pub items: Vec<MenuItem>,
}

impl AppView {
    /// Open a context menu at the pointer position.
    pub fn open_context_menu(
        &mut self,
        position: Point<Pixels>,
        title: impl Into<String>,
        items: Vec<MenuItem>,
        cx: &mut Context<Self>,
    ) {
        if items.is_empty() {
            return;
        }
        self.context_menu = Some(ContextMenu {
            title: title.into(),
            position,
            items,
        });
        cx.notify();
    }

    pub fn close_context_menu(&mut self, cx: &mut Context<Self>) {
        if self.context_menu.take().is_some() {
            cx.notify();
        }
    }

    /// The workspace actions available from a right-click on the sidebar.
    pub fn sidebar_menu_items(&self) -> Vec<MenuItem> {
        let mut items = vec![
            MenuItem::new("New collection", MenuAction::NewCollection),
            MenuItem::new("New environment", MenuAction::NewEnvironment),
            MenuItem::new("Refresh", MenuAction::ReloadCollections),
        ];
        if let Some(text) = self.active_response_text() {
            let _ = text;
            items.push(MenuItem::new(
                "Copy response body",
                MenuAction::CopyResponse,
            ));
        }
        items
    }

    pub fn tab_menu_items(&self, index: usize) -> Vec<MenuItem> {
        let mut items = vec![MenuItem::new("New tab", MenuAction::NewTab)];
        if self.tabs.len() > 1 {
            items.push(MenuItem::new("Close tab", MenuAction::CloseTab(index)));
        }
        items.push(MenuItem::new("Choose method", MenuAction::ChooseMethod));
        items
    }

    pub fn request_menu_items(&self) -> Vec<MenuItem> {
        let mut items = vec![
            MenuItem::new("Save request", MenuAction::SaveRequest),
            MenuItem::new("Choose method", MenuAction::ChooseMethod),
        ];
        if self.active_response_text().is_some() {
            items.push(MenuItem::new(
                "Copy response body",
                MenuAction::CopyResponse,
            ));
            items.push(MenuItem::new("Clear response", MenuAction::ClearResponse));
        }
        items.push(MenuItem::new("Settings", MenuAction::Settings));
        items.push(MenuItem::new("Toggle theme", MenuAction::ToggleTheme));
        items
    }

    fn run_menu_action(&mut self, action: MenuAction, cx: &mut Context<Self>) {
        self.context_menu = None;
        match action {
            MenuAction::NewTab => self.new_tab(cx),
            MenuAction::CloseTab(index) => self.close_tab(index, cx),
            MenuAction::Settings => self.open_dialog(Dialog::Settings, cx),
            MenuAction::ToggleTheme => {
                let next = if self.theme.mode == crate::theme::ThemeMode::Dark {
                    crate::theme::ThemeMode::Light
                } else {
                    crate::theme::ThemeMode::Dark
                };
                self.set_theme(next, cx);
            }
            MenuAction::ChooseMethod => {
                let tab = self.active_tab;
                self.open_dialog(Dialog::ChooseMethod { tab }, cx);
            }
            MenuAction::SaveRequest => self.request_save(cx),
            MenuAction::NewCollection => self.open_dialog(
                Dialog::Prompt {
                    title: "New collection".to_string(),
                    hint: "Give the collection a name.".to_string(),
                    action: PromptAction::NewCollection,
                    initial: String::new(),
                },
                cx,
            ),
            MenuAction::NewEnvironment => self.open_dialog(
                Dialog::Prompt {
                    title: "New environment".to_string(),
                    hint: "Give the environment a name.".to_string(),
                    action: PromptAction::NewEnvironment,
                    initial: String::new(),
                },
                cx,
            ),
            MenuAction::ReloadCollections => self.reload_collections(cx),
            MenuAction::CopyResponse => {
                if let Some(text) = self.active_response_text() {
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                    self.show_toast(
                        "Response body copied",
                        crate::components::ToastKind::Success,
                    );
                    cx.notify();
                } else {
                    self.show_toast(
                        "There is no response body to copy",
                        crate::components::ToastKind::Info,
                    );
                    cx.notify();
                }
            }
            MenuAction::ClearResponse => {
                if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                    tab.response.response = None;
                    tab.response.error = None;
                    tab.response.error_category = None;
                    tab.response.sent_at = None;
                    tab.response.expanded.clear();
                }
                cx.notify();
            }
            MenuAction::CloseMenu => cx.notify(),
        }
    }

    /// Render the open context menu, if any.
    pub(crate) fn render_context_menu(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let theme = self.theme;
        let menu = self.context_menu.as_ref()?;

        let mut card = div()
            .absolute()
            .left(menu.position.x)
            .top(menu.position.y)
            .w(px(196.0))
            .flex()
            .flex_col()
            .gap(px(1.0))
            .p(px(4.0))
            .rounded(radius::md())
            .bg(theme.elevated)
            .border_1()
            .border_color(theme.border_strong)
            .shadow_lg();

        card = card.child(
            div()
                .px(space::sm())
                .py(px(3.0))
                .text_size(text::micro())
                .text_color(theme.text_muted)
                .child(menu.title.clone()),
        );

        for (index, item) in menu.items.iter().enumerate() {
            let action = item.action;
            let color = if item.destructive {
                theme.error
            } else {
                theme.text_primary
            };
            card = card.child(
                div()
                    .flex()
                    .items_center()
                    .h(px(26.0))
                    .px(space::sm())
                    .rounded(radius::sm())
                    .text_size(text::small())
                    .text_color(color)
                    .cursor(gpui::CursorStyle::PointingHand)
                    .hover(move |style| style.bg(theme.hover))
                    .child(item.label)
                    .debug_selector(move || format!("menu-item-{index}"))
                    .on_press(
                        // Menu ids are stable per row; the listener closes the menu
                        // and runs the action immediately.
                        "context-menu-item",
                        cx.listener(move |view, _: &gpui::ClickEvent, _window, cx| {
                            view.run_menu_action(action, cx);
                        }),
                    ),
            );
        }

        Some(
            div()
                .absolute()
                .top(px(0.0))
                .left(px(0.0))
                .w_full()
                .h_full()
                .on_press(
                    "context-menu-scrim",
                    cx.listener(|view, _: &gpui::ClickEvent, _window, cx| {
                        view.close_context_menu(cx);
                    }),
                )
                .child(card)
                .into_any_element(),
        )
    }
}

/// The text body of the active response, when there is one worth copying.
impl AppView {
    pub fn active_response_text(&self) -> Option<String> {
        let response = self.active_tab_ref().response.response.as_ref()?;
        match &response.body {
            requestlab_models::ResponseBody::Text { text, .. } if !text.is_empty() => {
                Some(text.clone())
            }
            _ => None,
        }
    }
}

/// A short hint used by tests and empty states.
pub fn menu_hint() -> &'static str {
    "Right-click for actions"
}

/// Shared menu card styling used by both front ends.
pub fn menu_width() -> Pixels {
    px(196.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_items_carry_their_action() {
        let item = MenuItem::new("New tab", MenuAction::NewTab);
        assert_eq!(item.action, MenuAction::NewTab);
        let destructive = MenuItem::destructive("Delete", MenuAction::ClearResponse);
        assert!(destructive.destructive);
    }

    #[test]
    fn menu_width_matches_the_card() {
        assert_eq!(menu_width(), px(196.0));
        assert!(!menu_hint().is_empty());
    }
}
