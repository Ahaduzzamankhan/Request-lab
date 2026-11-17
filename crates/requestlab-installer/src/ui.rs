//! The installer window.
//!
//! It renders the shared [`FlowState`] and nothing else: every line on screen
//! comes from a real engine event, so the progress and the log are always the
//! truth of the running installation.

use std::path::PathBuf;
use std::sync::Arc;

use gpui::prelude::*;
use gpui::{div, px, relative, Context, IntoElement, ParentElement, Render, Styled, Window};
use requestlab_manager::events::ManagerEvent;
use requestlab_manager::installer::InstallOptions;
use requestlab_manager::manifest::Channel;
use requestlab_manager::version::Version;
use requestlab_manager_ui::{FlowState, Operation};
use requestlab_ui::components::{self, ButtonVariant, InteractiveExt};
use requestlab_ui::theme::{radius, space, text, Theme};
use tokio::runtime::Runtime;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};

use crate::flow;

/// What the background task reports to the window.
enum UiEvent {
    Manager(ManagerEvent),
    Done {
        version: Version,
        directory: PathBuf,
    },
    Failed(String),
}

pub struct InstallerView {
    flow: FlowState,
    theme: Theme,
    runtime: Arc<Runtime>,
    install_dir: PathBuf,
    events: UnboundedSender<UiEvent>,
}

impl InstallerView {
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
        let mut flow = FlowState::new(Operation::Install);
        flow.install_dir = install_dir.display().to_string();
        Self {
            flow,
            theme: Theme::dark(),
            runtime,
            install_dir,
            events,
        }
    }

    fn apply(&mut self, event: UiEvent) {
        match event {
            UiEvent::Manager(event) => self.flow.apply(event),
            UiEvent::Done { version, directory } => self.flow.succeed(version, &directory),
            UiEvent::Failed(message) => self.flow.fail(message),
        }
    }

    fn start(&mut self, cx: &mut Context<Self>) {
        if self.flow.is_working() {
            return;
        }
        // A finished or failed run starts over from a clean state.
        self.flow.start();
        self.flow.install_dir = self.install_dir.display().to_string();
        cx.notify();

        let options = InstallOptions {
            install_dir: self.install_dir.clone(),
            channel: Channel::Stable,
            shortcuts: true,
            register_uninstall: true,
        };
        let install_dir = self.install_dir.clone();
        let events = self.events.clone();
        // The engine's event sink is a borrowed `FnMut`, so its future is not
        // `Send`. Give the installation its own thread and runtime instead of
        // holding any part of it on the UI thread.
        let runtime = self.runtime.clone();
        std::thread::spawn(move || {
            let result = runtime.block_on(flow::install(options, true, |event| {
                let _ = events.send(UiEvent::Manager(event));
            }));
            let event = match result {
                Ok(metadata) => UiEvent::Done {
                    version: metadata.version,
                    directory: install_dir,
                },
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
            .id("installer-log")
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
}

impl Render for InstallerView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let working = self.flow.is_working();
        let finished = self.flow.is_finished();

        let label = self.flow.primary_label();
        let variant = if finished {
            ButtonVariant::Secondary
        } else {
            ButtonVariant::Primary
        };
        let primary = if working {
            components::button(&theme, label, variant)
                .opacity(0.6)
                .into_any_element()
        } else {
            components::button(&theme, label, variant)
                .debug_selector(|| "installer-primary".to_string())
                .on_press(
                    "installer-primary",
                    cx.listener(|view, _: &gpui::ClickEvent, _window, cx| {
                        if view.flow.is_finished() {
                            cx.quit();
                        } else {
                            view.start(cx);
                        }
                    }),
                )
                .into_any_element()
        };

        let close = components::button(&theme, "Close", ButtonVariant::Ghost)
            .debug_selector(|| "installer-close".to_string())
            .on_press(
                "installer-close",
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
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(text::title())
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(self.flow.title()),
                    )
                    .child(
                        div()
                            .text_size(text::small())
                            .text_color(theme.text_secondary)
                            .child(format!(
                                "Version {} · signed releases only",
                                requestlab_manager::VERSION
                            )),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .p(space::sm())
                    .rounded(radius::md())
                    .bg(theme.surface)
                    .border_1()
                    .border_color(theme.border)
                    .child(components::field_label(&theme, "Install location"))
                    .child(
                        div()
                            .font_family(theme.mono_family())
                            .text_size(text::small())
                            .child(self.install_dir.display().to_string()),
                    )
                    .child(
                        div()
                            .text_size(text::micro())
                            .text_color(theme.text_muted)
                            .child(
                            "Desktop and start menu shortcuts and an uninstall entry are created.",
                        ),
                    ),
            )
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
            .when(!self.flow.warnings.is_empty(), |row| {
                row.child(
                    div()
                        .text_size(text::micro())
                        .text_color(theme.warning)
                        .child(self.flow.warnings.join(" · ")),
                )
            })
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

/// Open the installer window and run until it closes.
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
                    gpui::size(px(560.0), px(470.0)),
                    cx,
                ))),
                titlebar: Some(gpui::TitlebarOptions {
                    title: Some("RequestLab installer".into()),
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
                window.set_window_title("RequestLab installer");
                cx.new(|cx| InstallerView::new(runtime.clone(), cx))
            }) {
                eprintln!("The installer window could not open: {error}");
                cx.quit();
            }
            cx.activate(true);
        });
    Ok(())
}
