//! Headless smoke test: the real workspace, driven by synthetic input.
//!
//! The GPUI test platform runs the same element tree and event dispatch as the
//! desktop window, so a panic here is a panic in the application.

use std::sync::Arc;

use gpui::{point, px, AnyWindowHandle, Entity, Modifiers, TestAppContext, VisualTestContext};
use requestlab_core::AppService;
use requestlab_ui::{AppView, ThemeMode};

/// The application bridges to a real Tokio runtime for every database and
/// network call, so results arrive from worker threads. GPUI's test scheduler
/// calls that non-deterministic; allowing parking is the documented way to test
/// a mix of deterministic UI work and real I/O.
fn allow_background_work(cx: &TestAppContext) {
    cx.dispatcher.allow_parking();
}

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
    allow_background_work(cx);
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
    allow_background_work(cx);
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

/// Every control that is generated in a loop must have its own element id.
///
/// GPUI dispatches a click to the element that owns the id, so reusing one id for
/// a whole group leaves all but one member inert. This test clicks more than one
/// member of each grouped control and checks that each click takes effect.
#[gpui::test]
fn every_control_in_a_group_is_clickable(cx: &mut TestAppContext) {
    cx.update(ely_gpui_component::init_for_tests);
    allow_background_work(cx);
    let (service, runtime) = workspace();
    let handle = cx.add_window(|_window, cx| AppView::new(service, runtime, ThemeMode::Dark, cx));
    let mut cx = VisualTestContext::from_window(handle.into(), cx);
    let view: Entity<AppView> = handle.root(&mut cx.cx).expect("root view");
    cx.run_until_parked();

    // Settings: two different timeout buttons must both work.
    click(&mut cx, "settings");
    click(&mut cx, "timeout-15");
    assert_eq!(
        cx.cx.update(|app| view.read(app).timeout_seconds),
        15,
        "the 15 s timeout button was inert"
    );
    click(&mut cx, "timeout-120");
    assert_eq!(
        cx.cx.update(|app| view.read(app).timeout_seconds),
        120,
        "the 2 m timeout button was inert"
    );

    // Theme buttons are also generated in a loop.
    click(&mut cx, "theme-light");
    assert_eq!(
        cx.cx.update(|app| view.read(app).theme.mode),
        ThemeMode::Light,
        "the light theme button was inert"
    );
    click(&mut cx, "theme-dark");
    assert_eq!(
        cx.cx.update(|app| view.read(app).theme.mode),
        ThemeMode::Dark,
        "the dark theme button was inert"
    );

    // Dismiss settings, then use the method picker: POST and DELETE are the two
    // ends of the same loop.
    cx.simulate_click(point(px(4.0), px(4.0)), Modifiers::default());
    cx.run_until_parked();

    click(&mut cx, "method-picker");
    click(&mut cx, "method-POST");
    let method = cx
        .cx
        .update(|app| view.read(app).tabs[view.read(app).active_tab].editor.method);
    assert_eq!(method.as_str(), "POST", "the POST method chip was inert");

    click(&mut cx, "method-picker");
    click(&mut cx, "method-DELETE");
    let method = cx
        .cx
        .update(|app| view.read(app).tabs[view.read(app).active_tab].editor.method);
    assert_eq!(
        method.as_str(),
        "DELETE",
        "the DELETE method chip was inert"
    );
}

/// Click a control at a horizontal fraction of its bounds (0.0 = left edge).
fn click_across(cx: &mut VisualTestContext, selector: &'static str, fraction: f32) {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("no element with selector {selector}"));
    let x = bounds.left() + bounds.size.width * fraction;
    cx.simulate_click(point(x, bounds.center().y), Modifiers::default());
    cx.run_until_parked();
}

/// The editor tabs, body types and auth types are also loop-generated, and the
/// two Ely segmented controls drive the sidebar section and the response tab.
#[gpui::test]
fn every_tab_and_segment_switches(cx: &mut TestAppContext) {
    cx.update(ely_gpui_component::init_for_tests);
    allow_background_work(cx);
    let (service, runtime) = workspace();
    let handle = cx.add_window(|_window, cx| AppView::new(service, runtime, ThemeMode::Dark, cx));
    let mut cx = VisualTestContext::from_window(handle.into(), cx);
    let view: Entity<AppView> = handle.root(&mut cx.cx).expect("root view");
    cx.run_until_parked();

    // Editor tabs: open Headers, then Auth.
    click(&mut cx, "editor:137:Headers");
    assert_eq!(
        cx.cx.update(|app| view.read(app).tabs[0].editor.tab),
        requestlab_ui::EditorTab::Headers,
        "the Headers editor tab was inert"
    );
    click(&mut cx, "editor:137:Auth");
    assert_eq!(
        cx.cx.update(|app| view.read(app).tabs[0].editor.tab),
        requestlab_ui::EditorTab::Auth,
        "the Auth editor tab was inert"
    );

    // Auth kinds inside the Auth tab.
    click(&mut cx, "editor:523:Bearer");
    assert_eq!(
        cx.cx.update(|app| view.read(app).tabs[0].editor.auth_kind),
        requestlab_ui::AuthKind::Bearer,
        "the Bearer auth type was inert"
    );
    click(&mut cx, "editor:523:Basic");
    assert_eq!(
        cx.cx.update(|app| view.read(app).tabs[0].editor.auth_kind),
        requestlab_ui::AuthKind::Basic,
        "the Basic auth type was inert"
    );

    // Body kinds inside the Body tab.
    click(&mut cx, "editor:137:Body");
    click(&mut cx, "editor:332:JSON");
    let body_kind = cx.cx.update(|app| view.read(app).tabs[0].editor.body_kind);
    assert_eq!(body_kind.label(), "JSON", "the JSON body type was inert");

    // Sidebar sections, driven by Ely's segmented control. Clicking the far right
    // must leave the starting section, and the far left must return to it.
    let start = cx.cx.update(|app| view.read(app).sidebar);
    click_across(&mut cx, "sidebar-sections", 0.9);
    let moved = cx.cx.update(|app| view.read(app).sidebar);
    assert_ne!(moved, start, "the last sidebar section was not reachable");
    click_across(&mut cx, "sidebar-sections", 0.05);
    assert_eq!(
        cx.cx.update(|app| view.read(app).sidebar),
        start,
        "the first sidebar section was not reachable"
    );

    // Response tabs, also Ely's segmented control.
    click_across(&mut cx, "response-tabs", 0.9);
    assert_eq!(
        cx.cx.update(|app| view.read(app).tabs[0].response.tab),
        requestlab_ui::ResponseTab::Cookies,
        "the Cookies response tab was not reachable"
    );
    click_across(&mut cx, "response-tabs", 0.05);
    assert_eq!(
        cx.cx.update(|app| view.read(app).tabs[0].response.tab),
        requestlab_ui::ResponseTab::Pretty,
        "the Pretty response tab was not reachable"
    );
}
