//! The root view: layout, tabs, async orchestration and app-level actions.
//!
//! Panels live in sibling modules and are reached through `impl AppView` blocks
//! so that each file stays focused while sharing one state machine.

use std::collections::HashSet;
use std::sync::Arc;

use gpui::prelude::*;
use gpui::{div, px, AppContext, Context, Entity, IntoElement, Render, SharedString, Window};
use requestlab_core::{
    cancellation_channel, AppService, Canceller, EnvironmentError, ExecutionError, ExecutionOutcome,
};
use requestlab_models::{
    human_duration, CollectionTree, Environment, HistoryEntry, HistoryQuery, HistoryStatusFilter,
    HttpMethod, HttpResponse,
};
use tokio::runtime::Runtime;
use uuid::Uuid;

use crate::components::InteractiveExt;
use crate::components::{self, ToastKind};
use crate::editor_state::{RequestEditor, SavedLocation};
use crate::input::{key_bindings, TextInput};
use crate::json_tree;
use crate::theme::{space, text, with_alpha, Theme, ThemeMode};

/// Which sidebar list is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarSection {
    Collections,
    History,
    Environments,
}

impl SidebarSection {
    pub const ALL: [SidebarSection; 3] = [
        SidebarSection::Collections,
        SidebarSection::History,
        SidebarSection::Environments,
    ];

    pub fn label(self) -> &'static str {
        match self {
            SidebarSection::Collections => "Collections",
            SidebarSection::History => "History",
            SidebarSection::Environments => "Environments",
        }
    }
}

/// Which response tab is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseTab {
    Pretty,
    Raw,
    Headers,
    Cookies,
}

impl ResponseTab {
    pub const ALL: [ResponseTab; 4] = [
        ResponseTab::Pretty,
        ResponseTab::Raw,
        ResponseTab::Headers,
        ResponseTab::Cookies,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ResponseTab::Pretty => "Pretty",
            ResponseTab::Raw => "Raw",
            ResponseTab::Headers => "Headers",
            ResponseTab::Cookies => "Cookies",
        }
    }
}

/// Response panel state for one tab.
pub struct ResponseState {
    pub response: Option<HttpResponse>,
    pub error: Option<String>,
    pub error_category: Option<String>,
    pub tab: ResponseTab,
    pub search: Entity<TextInput>,
    pub expanded: HashSet<String>,
    pub resolved_url: Option<String>,
    pub variables: Vec<String>,
    pub history_id: Option<Uuid>,
    pub sent_at: Option<String>,
}

impl ResponseState {
    fn new(cx: &mut Context<AppView>) -> Self {
        let search = cx.new(|cx| TextInput::new(cx, "Search response", false));
        let mut expanded = HashSet::new();
        expanded.insert(String::new());
        Self {
            response: None,
            error: None,
            error_category: None,
            tab: ResponseTab::Pretty,
            search,
            expanded,
            resolved_url: None,
            variables: Vec::new(),
            history_id: None,
            sent_at: None,
        }
    }

    /// Parsed JSON rows for the Pretty tab, if the body is JSON.
    pub(crate) fn json_rows(&self, cx: &gpui::App) -> Option<Vec<json_tree::JsonRow>> {
        let response = self.response.as_ref()?;
        let text = response.body.as_text()?;
        let value: serde_json::Value = serde_json::from_str(text).ok()?;
        let search = self.search.read(cx).content().to_string();
        Some(json_tree::build_rows(&value, &self.expanded, &search))
    }
}

/// One open request tab.
pub struct Tab {
    pub editor: RequestEditor,
    pub response: ResponseState,
    pub sending: bool,
    pub cancel: Option<Canceller>,
    pub execution: Option<Uuid>,
}

impl Tab {
    fn new(cx: &mut Context<AppView>, theme: &Theme) -> Self {
        Self {
            editor: RequestEditor::new(cx, theme),
            response: ResponseState::new(cx),
            sending: false,
            cancel: None,
            execution: None,
        }
    }

    fn apply_outcome(&mut self, outcome: ExecutionOutcome, cx: &mut Context<AppView>) {
        self.response.variables = outcome.variables.clone();
        self.response.history_id = outcome.history_id;
        self.response.resolved_url = outcome
            .resolved_request
            .as_ref()
            .map(|request| request.url.clone());
        match outcome.result {
            Ok(response) => {
                self.response.error = None;
                self.response.error_category = None;
                self.response.sent_at = Some(response.completed_at.clone());
                self.response.response = Some(response);
                self.response.expanded.clear();
                self.response.expanded.insert(String::new());
            }
            Err(error) => {
                self.response.response = None;
                self.response.error_category = Some(error.category().to_string());
                self.response.error = Some(error.message());
            }
        }
        self.response.search.update(cx, |input, _| input.clear());
        cx.notify();
    }
}

/// An editable environment variable row.
pub struct EnvRow {
    pub id: Uuid,
    pub key: Entity<TextInput>,
    pub value: Entity<TextInput>,
    pub enabled: bool,
    pub secret: bool,
}

/// A modal dialog.
pub enum Dialog {
    Confirm {
        title: String,
        message: String,
        action: DestructiveAction,
    },
    Prompt {
        title: String,
        hint: String,
        action: PromptAction,
        initial: String,
    },
    ChooseMethod {
        tab: usize,
    },
    ChooseEnvironment,
    SaveRequest {
        tab: usize,
    },
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DestructiveAction {
    DeleteCollection(Uuid),
    DeleteFolder(Uuid),
    DeleteSavedRequest(Uuid),
    DeleteHistoryEntry(Uuid),
    ClearHistory,
    DeleteEnvironment(Uuid),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptAction {
    NewCollection,
    RenameCollection(Uuid),
    NewFolder {
        collection_id: Uuid,
        parent_id: Option<Uuid>,
    },
    RenameFolder(Uuid),
    RenameSavedRequest(Uuid),
    NewEnvironment,
    RenameEnvironment(Uuid),
    DuplicateEnvironment(Uuid),
    DuplicateCollection(Uuid),
}

/// The root application view.
pub struct AppView {
    pub service: AppService,
    pub runtime: Arc<Runtime>,
    pub theme: Theme,

    pub tabs: Vec<Tab>,
    pub active_tab: usize,

    pub sidebar: SidebarSection,
    pub collections: Vec<CollectionTree>,
    pub expanded_collections: HashSet<Uuid>,
    pub expanded_folders: HashSet<Uuid>,

    pub environments: Vec<Environment>,
    pub active_environment: Option<Uuid>,
    pub env_rows: Vec<EnvRow>,
    pub env_selected: Option<Uuid>,

    pub history: Vec<HistoryEntry>,
    pub history_search: Entity<TextInput>,
    pub history_method: Option<HttpMethod>,
    pub history_status: HistoryStatusFilter,

    pub dialog: Option<Dialog>,
    pub dialog_input: Entity<TextInput>,
    pub toast: Option<(String, ToastKind)>,

    pub loading: bool,
    pub error_banner: Option<String>,
    pub status_note: Option<String>,
    pub database_path: Option<String>,
    pub timeout_seconds: u64,
    pub history_limit: i64,
}

impl AppView {
    pub fn new(
        service: AppService,
        runtime: Arc<Runtime>,
        mode: ThemeMode,
        cx: &mut Context<Self>,
    ) -> Self {
        let theme = Theme::for_mode(mode);
        let history_search = cx.new(|cx| TextInput::new(cx, "Search history", false));
        let dialog_input = cx.new(|cx| TextInput::new(cx, "", false));
        let database_path = service
            .storage()
            .path()
            .map(|path| path.display().to_string());
        let timeout_seconds = service.settings().timeout_seconds;
        let history_limit = service.settings().history_limit;

        let mut view = Self {
            tabs: vec![Tab::new(cx, &theme)],
            active_tab: 0,
            sidebar: SidebarSection::Collections,
            collections: Vec::new(),
            expanded_collections: HashSet::new(),
            expanded_folders: HashSet::new(),
            environments: Vec::new(),
            active_environment: None,
            env_rows: Vec::new(),
            env_selected: None,
            history: Vec::new(),
            history_search,
            history_method: None,
            history_status: HistoryStatusFilter::All,
            dialog: None,
            dialog_input,
            toast: None,
            loading: true,
            error_banner: None,
            status_note: None,
            database_path,
            timeout_seconds,
            history_limit,
            service,
            runtime,
            theme,
        };
        view.load_initial(cx);
        view
    }

    // ------------------------------------------------------------- async glue

    /// Run `work` on the Tokio runtime, then apply its result on the UI thread.
    ///
    /// Reqwest needs a Tokio reactor, which GPUI's executor does not provide, so
    /// every network and database call goes through here.
    pub fn spawn_core<T, F, D>(&self, cx: &mut Context<Self>, work: F, done: D)
    where
        T: Send + 'static,
        F: std::future::Future<Output = T> + Send + 'static,
        D: FnOnce(&mut Self, T, &mut Context<Self>) + 'static,
    {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.runtime.spawn(async move {
            let _ = tx.send(work.await);
        });
        cx.spawn(async move |weak, cx| {
            if let Ok(value) = rx.await {
                let _ = weak.update(cx, |this, cx| done(this, value, cx));
            }
        })
        .detach();
    }

    fn load_initial(&mut self, cx: &mut Context<Self>) {
        let service = self.service.clone();
        self.spawn_core(
            cx,
            async move { service.snapshot().await },
            |this, result, cx| {
                this.loading = false;
                match result {
                    Ok(snapshot) => {
                        this.collections = snapshot.collections;
                        this.expanded_collections = this
                            .collections
                            .iter()
                            .map(|tree| tree.collection.id)
                            .collect();
                        this.environments = snapshot.environments;
                        this.active_environment = snapshot.active_environment;
                        this.history = snapshot.history;
                        this.theme =
                            Theme::for_mode(ThemeMode::from_setting(&snapshot.settings.theme));
                        if let Some(environment) = this.active_environment {
                            this.load_environment(environment, cx);
                        }
                    }
                    Err(error) => {
                        this.error_banner =
                            Some(format!("Could not open RequestLab data: {error}"));
                    }
                }
                cx.notify();
            },
        );
    }

    pub fn reload_collections(&mut self, cx: &mut Context<Self>) {
        let service = self.service.clone();
        self.spawn_core(
            cx,
            async move { service.collections().list_trees().await },
            |this, result, cx| {
                match result {
                    Ok(trees) => {
                        this.collections = trees;
                        this.error_banner = None;
                    }
                    Err(error) => {
                        this.error_banner = Some(format!("Collections: {error}"));
                    }
                }
                cx.notify();
            },
        );
    }

    pub fn reload_environments(&mut self, cx: &mut Context<Self>) {
        let service = self.service.clone();
        self.spawn_core(
            cx,
            async move {
                let environments = service.environments().list().await?;
                let active = service.environments().active_id().await?;
                Ok::<_, EnvironmentError>((environments, active))
            },
            |this, result, cx| {
                match result {
                    Ok((environments, active)) => {
                        this.environments = environments;
                        this.active_environment = active;
                        if let Some(selected) = this.env_selected {
                            if !this.environments.iter().any(|env| env.id == selected) {
                                this.env_selected =
                                    active.or_else(|| this.environments.first().map(|env| env.id));
                                if let Some(id) = this.env_selected {
                                    this.load_environment(id, cx);
                                }
                            }
                        } else {
                            this.env_selected =
                                active.or_else(|| this.environments.first().map(|env| env.id));
                            if let Some(id) = this.env_selected {
                                this.load_environment(id, cx);
                            }
                        }
                    }
                    Err(error) => this.error_banner = Some(format!("Environments: {error}")),
                }
                cx.notify();
            },
        );
    }

    pub fn reload_history(&mut self, cx: &mut Context<Self>) {
        let service = self.service.clone();
        let query = self.current_history_query(cx);
        self.spawn_core(
            cx,
            async move { service.history(&query).await },
            |this, result, cx| {
                match result {
                    Ok(entries) => this.history = entries,
                    Err(error) => this.error_banner = Some(format!("History: {error}")),
                }
                cx.notify();
            },
        );
    }

    pub fn current_history_query(&self, cx: &gpui::App) -> HistoryQuery {
        HistoryQuery {
            search: self.history_search.read(cx).content().to_string(),
            method: self.history_method,
            status: self.history_status,
            limit: 200,
            offset: 0,
        }
    }

    /// Load an environment's variables into the editor rows.
    pub fn load_environment(&mut self, id: Uuid, cx: &mut Context<Self>) {
        self.env_selected = Some(id);
        let Some(environment) = self.environments.iter().find(|env| env.id == id) else {
            self.env_rows.clear();
            return;
        };
        let variables = environment.variables.clone();
        self.env_rows = variables
            .into_iter()
            .map(|variable| EnvRow {
                id: variable.id,
                key: cx.new(move |cx| {
                    let mut input = TextInput::new(cx, "VARIABLE", false);
                    input.set_content(variable.key.clone());
                    input
                }),
                value: cx.new(move |cx| {
                    let mut input = TextInput::new(cx, "value", false);
                    input.set_content(variable.value.clone());
                    input
                }),
                enabled: variable.enabled,
                secret: variable.is_secret,
            })
            .collect();
        if self.env_rows.is_empty() {
            self.push_env_row(cx);
        }
    }

    pub fn push_env_row(&mut self, cx: &mut Context<Self>) {
        self.env_rows.push(EnvRow {
            id: Uuid::new_v4(),
            key: cx.new(|cx| TextInput::new(cx, "VARIABLE", false)),
            value: cx.new(|cx| TextInput::new(cx, "value", false)),
            enabled: true,
            secret: false,
        });
    }

    // ---------------------------------------------------------------- actions

    pub fn active_tab_mut(&mut self) -> &mut Tab {
        let index = self.active_tab.min(self.tabs.len().saturating_sub(1));
        &mut self.tabs[index]
    }

    pub fn active_tab_ref(&self) -> &Tab {
        let index = self.active_tab.min(self.tabs.len().saturating_sub(1));
        &self.tabs[index]
    }

    pub fn new_tab(&mut self, cx: &mut Context<Self>) {
        let theme = self.theme;
        let tab = Tab::new(cx, &theme);
        self.tabs.push(tab);
        self.active_tab = self.tabs.len() - 1;
        cx.notify();
    }

    pub fn close_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.tabs.len() == 1 {
            return;
        }
        self.tabs.remove(index);
        if self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len() - 1;
        }
        cx.notify();
    }

    pub fn send_request(&mut self, cx: &mut Context<Self>) {
        let index = self.active_tab;
        let Some(tab) = self.tabs.get_mut(index) else {
            return;
        };
        if tab.sending {
            return;
        }
        let request = tab.editor.to_request(cx);
        let execution_id = Uuid::new_v4();
        let (canceller, cancel_rx) = cancellation_channel();
        tab.sending = true;
        tab.execution = Some(execution_id);
        tab.cancel = Some(canceller);
        tab.response.error = None;
        tab.response.error_category = None;
        self.status_note = Some(format!("Sending {} {}", request.method, request.url));
        cx.notify();

        let service = self.service.clone();
        self.spawn_core(
            cx,
            async move { service.send(&request, execution_id, cancel_rx).await },
            move |this, outcome, cx| {
                if let Some(tab) = this.tabs.get_mut(index) {
                    if tab.execution == Some(outcome.execution_id) {
                        tab.sending = false;
                        tab.cancel = None;
                        tab.apply_outcome(outcome, cx);
                    }
                }
                this.status_note = None;
                this.reload_history(cx);
                cx.notify();
            },
        );
    }

    pub fn cancel_request(&mut self, cx: &mut Context<Self>) {
        let index = self.active_tab;
        if let Some(tab) = self.tabs.get_mut(index) {
            if let Some(canceller) = tab.cancel.as_mut() {
                canceller.cancel();
            }
            tab.sending = false;
            tab.cancel = None;
            tab.response.error_category = Some("Cancelled".to_string());
            tab.response.error = Some("The request was cancelled.".to_string());
        }
        self.status_note = None;
        cx.notify();
    }

    /// Load a saved request into the active tab.
    pub fn open_saved_request(
        &mut self,
        saved_id: Uuid,
        collection_id: Uuid,
        folder_id: Option<Uuid>,
        cx: &mut Context<Self>,
    ) {
        let service = self.service.clone();
        self.spawn_core(
            cx,
            async move { service.collections().saved_request(saved_id).await },
            move |this, result, cx| {
                match result {
                    Ok(saved) => {
                        let theme = this.theme;
                        let mut editor = RequestEditor::new(cx, &theme);
                        editor.load(&saved.request, cx);
                        editor.saved = Some(SavedLocation {
                            saved_request_id: saved.id,
                            collection_id,
                            folder_id,
                        });
                        if let Some(tab) = this.tabs.get_mut(this.active_tab) {
                            tab.editor = editor;
                            tab.response = ResponseState::new(cx);
                        }
                        this.toast =
                            Some((format!("Opened {}", saved.display_name()), ToastKind::Info));
                    }
                    Err(error) => this.toast = Some((format!("{error}"), ToastKind::Error)),
                }
                cx.notify();
            },
        );
    }

    /// Save the active editor's request into a collection or folder.
    pub fn save_request_to(
        &mut self,
        collection_id: Uuid,
        folder_id: Option<Uuid>,
        cx: &mut Context<Self>,
    ) {
        let request = self.active_tab_ref().editor.to_request(cx);
        let name = if request.display_name().trim().is_empty() {
            "Untitled request".to_string()
        } else {
            request.display_name()
        };
        let existing = self.active_tab_ref().editor.saved;
        let service = self.service.clone();
        let saved_name = name.clone();
        self.spawn_core(
            cx,
            async move {
                match existing {
                    Some(location) => {
                        service
                            .collections()
                            .update_saved_request(location.saved_request_id, &name, &request)
                            .await?;
                        Ok::<_, requestlab_core::CollectionError>(location.saved_request_id)
                    }
                    None => {
                        let saved = service
                            .collections()
                            .save_request(collection_id, folder_id, &name, &request)
                            .await?;
                        Ok::<_, requestlab_core::CollectionError>(saved.id)
                    }
                }
            },
            move |this, result, cx| {
                match result {
                    Ok(saved_id) => {
                        if let Some(tab) = this.tabs.get_mut(this.active_tab) {
                            tab.editor.saved = Some(SavedLocation {
                                saved_request_id: saved_id,
                                collection_id,
                                folder_id,
                            });
                        }
                        this.toast = Some((format!("Saved {saved_name}"), ToastKind::Success));
                        this.reload_collections(cx);
                    }
                    Err(error) => {
                        this.toast = Some((format!("Save failed: {error}"), ToastKind::Error));
                    }
                }
                cx.notify();
            },
        );
    }

    pub fn set_theme(&mut self, mode: ThemeMode, cx: &mut Context<Self>) {
        self.theme = Theme::for_mode(mode);
        let mut service = self.service.clone();
        let setting = mode.as_setting().to_string();
        self.spawn_core(
            cx,
            async move { service.set_theme(&setting).await },
            |_this, result, _cx| {
                if let Err(error) = result {
                    tracing::warn!(%error, "could not persist theme");
                }
            },
        );
        cx.notify();
    }

    pub fn set_timeout(&mut self, seconds: u64, cx: &mut Context<Self>) {
        self.timeout_seconds = seconds;
        let mut service = self.service.clone();
        self.spawn_core(
            cx,
            async move { service.set_timeout_seconds(seconds).await },
            move |this, result, cx| {
                match result {
                    Ok(()) => {
                        this.toast = Some((
                            format!("Request timeout set to {seconds}s"),
                            ToastKind::Success,
                        ))
                    }
                    Err(error) => this.toast = Some((format!("{error}"), ToastKind::Error)),
                }
                cx.notify();
            },
        );
    }

    pub fn open_dialog(&mut self, dialog: Dialog, cx: &mut Context<Self>) {
        if let Dialog::Prompt { initial, .. } = &dialog {
            let initial = initial.clone();
            self.dialog_input
                .update(cx, |input, _| input.set_content(initial));
        }
        self.dialog = Some(dialog);
        cx.notify();
    }

    pub fn close_dialog(&mut self, cx: &mut Context<Self>) {
        self.dialog = None;
        cx.notify();
    }

    pub fn show_toast(&mut self, message: impl Into<String>, kind: ToastKind) {
        self.toast = Some((message.into(), kind));
    }

    pub fn set_history_limit(&mut self, limit: i64, cx: &mut Context<Self>) {
        self.history_limit = limit;
        let mut service = self.service.clone();
        self.spawn_core(
            cx,
            async move { service.set_history_limit(limit).await },
            move |this, result, cx| {
                match result {
                    Ok(()) => {
                        this.show_toast(format!("History limit set to {limit}"), ToastKind::Success)
                    }
                    Err(error) => this.show_toast(format!("{error}"), ToastKind::Error),
                }
                this.reload_history(cx);
                cx.notify();
            },
        );
    }
}

impl Render for AppView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let mut content = div()
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .bg(theme.background)
            .text_color(theme.text_primary)
            .font_family(theme.sans_family())
            .text_size(text::body())
            .child(self.render_toolbar(cx))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .min_h(px(0.0))
                    .child(self.render_sidebar(cx))
                    .child(components::vertical_divider(&theme))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w(px(0.0))
                            .child(self.render_tab_strip(cx))
                            .child(self.render_request_editor(cx))
                            .child(div().h(px(1.0)).w_full().bg(theme.border))
                            .child(self.render_response(cx)),
                    ),
            )
            .child(self.render_status_bar(window, cx));

        if let Some(banner) = self.error_banner.clone() {
            let background = with_alpha(theme.error, 0.12);
            content = content.child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px(space::md())
                    .py(space::sm())
                    .bg(background)
                    .border_t_1()
                    .border_color(theme.error)
                    .text_size(text::small())
                    .text_color(theme.error)
                    .child(banner)
                    .child(components::icon_button(&theme, "✕").on_click(cx.listener(
                        |view, _: &gpui::ClickEvent, _window, cx| {
                            view.error_banner = None;
                            cx.notify();
                        },
                    ))),
            );
        }

        let mut root = div().relative().size_full().child(content);

        if let Some(dialog) = self.render_dialog(cx) {
            root = root.child(dialog);
        }
        if let Some((message, kind)) = self.toast.clone() {
            root = root.child(
                div()
                    .absolute()
                    .bottom(px(40.0))
                    .right(px(16.0))
                    .child(components::toast(&theme, message, kind))
                    .on_click(cx.listener(|view, _: &gpui::ClickEvent, _window, cx| {
                        view.toast = None;
                        cx.notify();
                    })),
            );
        }

        root
    }
}

/// Register all keyboard shortcuts. Called once during application start-up.
pub fn bind_keys(cx: &mut gpui::App) {
    let mut bindings = key_bindings();
    bindings.extend(crate::actions::key_bindings());
    cx.bind_keys(bindings);
}

/// Convenience: the label used for a timeout setting.
pub fn timeout_label(seconds: u64) -> String {
    if seconds < 60 {
        format!("{seconds}s")
    } else {
        format!("{}m", seconds / 60)
    }
}

/// Label used for an execution error's category chip.
pub fn error_badge(error: &ExecutionError) -> SharedString {
    format!("{} error", error.category()).into()
}

/// Human readable duration for the status bar.
pub fn duration_label(response: &HttpResponse) -> String {
    human_duration(response.duration_ms)
}

/// A short preview of a history row's outcome.
pub fn history_outcome(entry: &HistoryEntry) -> String {
    match (entry.status_code, &entry.error_message) {
        (Some(code), _) => format!("{code}"),
        (None, Some(_)) => "Failed".to_string(),
        (None, None) => "Pending".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_and_response_labels_are_stable() {
        assert_eq!(SidebarSection::ALL.len(), 3);
        assert_eq!(ResponseTab::ALL[2].label(), "Headers");
    }

    #[test]
    fn timeout_labels_are_compact() {
        assert_eq!(timeout_label(30), "30s");
        assert_eq!(timeout_label(120), "2m");
    }
}
