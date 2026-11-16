//! Headless smoke test: the real workspace, driven by synthetic input.
//!
//! The GPUI test platform runs the same element tree and event dispatch as the
//! desktop window, so a panic here is a panic in the application.

use std::sync::Arc;

use gpui::{point, px, AnyWindowHandle, Entity, Modifiers, TestAppContext, VisualTestContext};
use requestlab_core::AppService;
use requestlab_ui::{AppView, ThemeMode};

fn service() -> AppService {
    let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
    runtime
        .block_on(AppService::open_in_memory())
        .expect("in-memory service")
}

fn workspace() -> (AppService, Arc<tokio::runtime::Runtime>) {
    (
        service(),
        Arc::new(tokio::runtime::Runtime::new().expect("tokio runtime")),
    )
}

/// Click the centre of a control found by its debug selector.
fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("no element with selector {selector}"));
    cx.simulate_click(bounds.center(), Modifiers::default());
    cx.run_until_parked();
}

#[gpui::test]
fn the_workspace_renders_and_survives_input(cx: &mut TestAppContext) {
    cx.update(ely_gpui_component::init_for_tests);
    let (service, runtime) = workspace();
    let handle: AnyWindowHandle = cx
        .add_window(|_window, cx| AppView::new(service, runtime, ThemeMode::Dark, cx))
        .into();
    let mut cx = VisualTestContext::from_window(handle, cx);
    cx.run_until_parked();

    let size = cx.update(|window, _| window.viewport_size());
    let width = f32::from(size.width);
    let height = f32::from(size.height);

    // Walk the window, clicking at every region and typing after each click so
    // a focused input receives real keystrokes.
    let xs = [
        40.0_f32,
        width * 0.18,
        width * 0.35,
        width * 0.6,
        width * 0.85,
    ];
    let ys = [20.0_f32, 60.0, 96.0, 150.0, height * 0.35, height * 0.6];

    for y in ys {
        for x in xs {
            cx.simulate_click(point(px(x), px(y)), Modifiers::default());
            cx.run_until_parked();
            cx.simulate_keystrokes("a");
            cx.run_until_parked();
        }
    }
}

#[gpui::test]
fn the_toolbar_controls_work(cx: &mut TestAppContext) {
    cx.update(ely_gpui_component::init_for_tests);
    let (service, runtime) = workspace();
    let handle = cx.add_window(|_window, cx| AppView::new(service, runtime, ThemeMode::Dark, cx));
    let mut cx = VisualTestContext::from_window(handle.into(), cx);
    let view: Entity<AppView> = handle.root(&mut cx.cx).expect("root view");
    cx.run_until_parked();

    // Settings opens a dialog and the dialog closes when the scrim is clicked.
    click(&mut cx, "settings");
    assert!(
        cx.cx.update(|app| view.read(app).dialog.is_some()),
        "the settings button must open the settings dialog"
    );
    cx.simulate_click(point(px(4.0), px(4.0)), Modifiers::default());
    cx.run_until_parked();
    assert!(
        cx.cx.update(|app| view.read(app).dialog.is_none()),
        "clicking the scrim must dismiss the dialog"
    );

    // The theme toggle really switches the palette.
    click(&mut cx, "theme-toggle");
    assert_eq!(
        cx.cx.update(|app| view.read(app).theme.mode),
        ThemeMode::Light
    );

    // A new tab is created and becomes active.
    click(&mut cx, "new-tab");
    let (tabs, active) = cx
        .cx
        .update(|app| (view.read(app).tabs.len(), view.read(app).active_tab));
    assert_eq!(tabs, 2);
    assert_eq!(active, 1);
}
