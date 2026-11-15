//! Main window creation.

use std::sync::Arc;

use gpui::{
    px, size, App, AppContext as _, Bounds, TitlebarOptions, WindowBounds, WindowKind,
    WindowOptions,
};
use requestlab_core::AppService;
use requestlab_ui::{AppView, ThemeMode};
use tokio::runtime::Runtime;

/// Default and minimum main-window geometry in logical pixels.
pub const DEFAULT_WIDTH: f32 = 1180.0;
pub const DEFAULT_HEIGHT: f32 = 760.0;
pub const MIN_WIDTH: f32 = 900.0;
pub const MIN_HEIGHT: f32 = 560.0;

/// Window options for the single RequestLab workspace window.
pub fn main_window_options(cx: &mut App) -> WindowOptions {
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            None,
            size(
                px(DEFAULT_WIDTH.max(MIN_WIDTH)),
                px(DEFAULT_HEIGHT.max(MIN_HEIGHT)),
            ),
            cx,
        ))),
        titlebar: Some(TitlebarOptions {
            title: Some("RequestLab".into()),
            ..Default::default()
        }),
        focus: true,
        show: true,
        kind: WindowKind::Normal,
        is_movable: true,
        is_resizable: true,
        is_minimizable: true,
        ..Default::default()
    }
}

/// Open the workspace window, logging rather than panicking on failure so the
/// process can report a readable startup error.
pub fn open_main_window(
    cx: &mut App,
    service: AppService,
    runtime: Arc<Runtime>,
    theme: ThemeMode,
) {
    let options = main_window_options(cx);
    match cx.open_window(options, move |window, cx| {
        window.set_window_title("RequestLab");
        cx.new(|cx| AppView::new(service.clone(), runtime.clone(), theme, cx))
    }) {
        Ok(handle) => {
            if let Err(error) = handle.update(cx, |_, window, _| window.activate_window()) {
                tracing::warn!(%error, "could not activate the RequestLab window");
            }
        }
        Err(error) => {
            tracing::error!(%error, "could not open the RequestLab window");
            eprintln!("RequestLab could not open its window: {error}");
            cx.quit();
        }
    }
}
