//! The updater window: check on open, then show and install what is available.

use std::path::PathBuf;
use std::sync::Arc;

use gpui::prelude::*;
use gpui::{div, px, relative, Context, IntoElement, ParentElement, Render, Styled, Window};
use requestlab_manager::events::ManagerEvent;
use requestlab_manager::version::Version;
use requestlab_manager_ui::{FlowState, Operation, ReleaseInfo};
use requestlab_ui::components::{self, ButtonVariant, InteractiveExt};
use requestlab_ui::theme::{radius, space, text, Theme};
use tokio::runtime::Runtime;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};

use crate::flow;

enum UiEvent {
    Available(ReleaseInfo),
    UpToDate(Version),
    Manager(ManagerEvent),
    Done(Version),
    Failed(String),
}

pub struct UpdaterView {
    flow: FlowState,
    theme: Theme,
    runtime: Arc<Runtime>,
    install_dir: PathBuf,
    current: Version,
    available: Option<ReleaseInfo>,
    up_to_date: bool,
    events: UnboundedSender<UiEvent>,
}

impl UpdaterView {
    pub fn new(runtime: Arc<Runtime>, cx: &mut Context<Self>) -> Self {
        let (events, mut receiver) = unbounded_channel::<UiEvent>();
        cx.spawn(async move |weak, cx| {
            while let Some(event) = receiver.recv().await {
                let updated = weak.update(cx, |this, cx| {
                    this.apply(event);
                    cx.notify();
                });
                if updated.is_err() {
                    break;
                }
            }
        })
        .detach();

        let install_dir = crate::flow::suggested_install_dir();
        let current = flow::installed_version(&install_dir);
        let mut view = Self {
            flow: FlowState::new(Operation::Update),
            theme: Theme::dark(),
            runtime,
            install_dir,
            current,
            available: None,
            up_to_date: false,
            events,
        };
        view.check_now(cx);
        view
    }

    fn apply(&mut self, event: UiEvent) {
        match event {
            UiEvent::Available(info) => {
                self.flow.detail = format!("RequestLab {} is available", info.version);
                self.available = Some(info);
            }
            UiEvent::UpToDate(version) => {
                self.up_to_date = true;
                self.flow.detail = format!("RequestLab {version} is the newest version");
            }
            UiEvent::Manager(event) => self.flow.apply(event),
            UiEvent::Done(version) => {
                self.available = None;
                self.flow.succeed(version, &self.install_dir);
            }
            UiEvent::Failed(message) => self.flow.fail(message),
        }
    }

    fn check_now(&mut self, cx: &mut Context<Self>) {
        self.flow.detail = "Checking for updates".to_string();
        self.flow.stage = Some(requestlab_manager::events::Stage::CheckingRelease);
        cx.notify();

        let install_dir = self.install_dir.clone();
        let runtime = self.runtime.clone();
        let events = self.events.clone();
        std::thread::spawn(move || {
            let event = match runtime.block_on(flow::check(&install_dir)) {
                Ok(Some(update)) => UiEvent::Available(ReleaseInfo::from_update(&update)),
                Ok(None) => UiEvent::UpToDate(flow::installed_version(&install_dir)),
                Err(message) => UiEvent::Failed(message),
            };
            let _ = events.send(event);
        });
    }

    fn install(&mut self, cx: &mut Context<Self>) {
        if self.flow.is_working() || self.available.is_none() {
            return;
        }
        self.flow.start();
        cx.notify();

        let install_dir = self.install_dir.clone();
        let events = self.events.clone();
        let runtime = self.runtime.clone();
        std::thread::spawn(move || {
            let result = runtime.block_on(flow::update(&install_dir, None, true, |event| {
                let _ = events.send(UiEvent::Manager(event));
            }));
            let event = match result {
                Ok(version) => UiEvent::Done(version),
                Err(message) => UiEvent::Failed(message),
            };
            let _ = events.send(event);
        });
    }

    fn progress_bar(&self) -> impl IntoElement {
        let theme = self.theme;
        let fraction = self.flow.percent().unwrap_or(0.0).clamp(0.0, 1.0);
        div()
            .w_full()
            .h(px(6.0))
            .rounded(radius::sm())
            .bg(theme.elevated)
            .overflow_hidden()
            .child(
                div()
                    .h_full()
                    .w(relative(fraction))
                    .rounded(radius::sm())
                    .bg(
                        if self.flow.screen == requestlab_manager_ui::Screen::Failed {
                            theme.error
                        } else {
                            theme.accent
                        },
                    ),
            )
    }

    fn log_tail(&self) -> impl IntoElement {
        let theme = self.theme;
        let mut list = div()
            .flex()
            .flex_col()
            .gap(px(1.0))
            .flex_1()
            .min_h(px(0.0))
            .id("updater-log")
            .overflow_y_scroll();
        let start = self.flow.log.len().saturating_sub(40);
        for line in &self.flow.log[start..] {
            let color = match line.level {
                requestlab_manager_ui::LogLevel::Info => theme.text_muted,
                requestlab_manager_ui::LogLevel::Warning => theme.warning,
                requestlab_manager_ui::LogLevel::Error => theme.error,
            };
            list = list.child(
                div()
                    .font_family(theme.mono_family())
                    .text_size(text::micro())
                    .text_color(color)
                    .child(line.text.clone()),
            );
        }
        list
    }

    fn details(&self) -> impl IntoElement {
        let theme = self.theme;
        let text_color = theme.text_secondary;
        let headline = if let Some(info) = &self.available {
            format!(
                "RequestLab {} is available ({} download)",
                info.version,
                info.size_text()
            )
        } else if self.up_to_date {
            format!("RequestLab {} is up to date", self.current)
        } else {
            "Checking for updates…".to_string()
        };

        div()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .p(space::sm())
            .rounded(radius::md())
            .bg(theme.surface)
            .border_1()
            .border_color(theme.border)
            .child(
                div()
                    .text_size(text::small())
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .child(headline),
            )
            .child(
                div()
                    .text_size(text::micro())
                    .text_color(text_color)
                    .child(format!(
                        "Installed: {} · {} channel · {}",
                        self.current,
                        "stable",
                        self.install_dir.display()
                    )),
            )
            .when_some(
                self.available
                    .as_ref()
                    .and_then(|info| info.notes_url.clone()),
                |card, url| {
                    card.child(
                        div()
                            .text_size(text::micro())
                            .text_color(theme.accent)
                            .child(url),
                    )
                },
            )
    }
}

impl Render for UpdaterView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let working = self.flow.is_working();
        let finished = self.flow.is_finished();
        let label = if finished {
            "Close"
        } else if self.available.is_some() {
            "Update now"
        } else {
            "Check again"
        };

        let primary = if working {
            components::button(&theme, "Updating…", ButtonVariant::Primary)
                .opacity(0.6)
                .into_any_element()
        } else {
            components::button(
                &theme,
                label,
                if finished {
                    ButtonVariant::Secondary
                } else {
                    ButtonVariant::Primary
                },
            )
            .debug_selector(|| "updater-primary".to_string())
            .on_press(
                "updater-primary",
                cx.listener(|view, _: &gpui::ClickEvent, _window, cx| {
                    if view.flow.is_finished() {
                        cx.quit();
                    } else if view.available.is_some() {
                        view.install(cx);
                    } else {
                        view.up_to_date = false;
                        view.check_now(cx);
                    }
                }),
            )
            .into_any_element()
        };

        let close = components::button(&theme, "Close", ButtonVariant::Ghost)
            .debug_selector(|| "updater-close".to_string())
            .on_press(
                "updater-close",
                cx.listener(|_view, _: &gpui::ClickEvent, _window, cx| cx.quit()),
            );

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(theme.background)
            .text_color(theme.text_primary)
            .font_family(theme.sans_family())
            .text_size(text::body())
            .p(space::lg())
            .gap(space::md())
            .child(
                div()
                    .text_size(text::title())
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(self.flow.title()),
            )
            .child(self.details())
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(space::xs())
                    .child(
                        div()
                            .text_size(text::small())
                            .text_color(
                                if self.flow.screen == requestlab_manager_ui::Screen::Failed {
                                    theme.error
                                } else {
                                    theme.text_secondary
                                },
                            )
                            .child(self.flow.status_line()),
                    )
                    .child(self.progress_bar())
                    .child(
                        div()
                            .text_size(text::micro())
                            .text_color(theme.text_muted)
                            .child(self.flow.progress_text()),
                    ),
            )
            .child(self.log_tail())
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(space::sm())
                    .child(close)
                    .child(primary),
            )
    }
}

pub fn run(runtime: Arc<Runtime>) -> anyhow::Result<()> {
    gpui_platform::application()
        .with_assets(ely_gpui_component::Assets)
        .run(move |cx| {
            if let Err(error) = ely_gpui_component::init(cx) {
                eprintln!("RequestLab's interface assets could not load: {error:#}");
                cx.quit();
                return;
            }
            let options = gpui::WindowOptions {
                window_bounds: Some(gpui::WindowBounds::Windowed(gpui::Bounds::centered(
                    None,
                    gpui::size(px(560.0), px(440.0)),
                    cx,
                ))),
                titlebar: Some(gpui::TitlebarOptions {
                    title: Some("RequestLab updater".into()),
                    ..Default::default()
                }),
                focus: true,
                show: true,
                kind: gpui::WindowKind::Normal,
                is_movable: true,
                is_resizable: true,
                is_minimizable: true,
                ..Default::default()
            };
            let runtime = runtime.clone();
            if let Err(error) = cx.open_window(options, move |window, cx| {
                window.set_window_title("RequestLab updater");
                cx.new(|cx| UpdaterView::new(runtime.clone(), cx))
            }) {
                eprintln!("The updater window could not open: {error}");
                cx.quit();
            }
            cx.activate(true);
        });
    Ok(())
}
