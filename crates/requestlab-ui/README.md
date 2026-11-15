# requestlab-ui

The native GPUI desktop interface.

## Responsibility

Everything the user sees and clicks: toolbar, tab strip, request editor,
response inspector, collections/history/environments sidebar, dialogs and the
status bar. It renders state and forwards intents; it never runs SQL and never
awaits a network call.

## Structure

| File | Contents |
|---|---|
| `theme.rs` | Design tokens (charcoal dark and newspaper-white light palettes, spacing, radii, type scale) |
| `components.rs` | Buttons, chips, toggles, tab buttons, empty states, toasts, plus `InteractiveExt` |
| `input.rs` | A real text editor element implementing `ElementInputHandler` (caret, selection, word motion) |
| `json_tree.rs` | Flattened, expandable JSON rows with search |
| `app_view.rs` | `AppView` root entity, tabs, dialogs, send/cancel, loading |
| `toolbar.rs`, `editor.rs`, `response_panel.rs`, `sidebar.rs`, `dialogs.rs` | Panel renderers and mutations |

## Threading model

GPUI has no Tokio reactor. `AppView` holds an `Arc<tokio::runtime::Runtime>`
and every async action goes through `spawn_core`, which runs the future on that
runtime and delivers the result back on the UI thread with
`cx.spawn` + `oneshot`. The pass stops any stale response from overwriting a
newer one by checking the execution id.

## GPUI version

Pinned to `gpui 0.2.2`. APIs were verified against the crate source; in
particular, `on_click`/`overflow_scroll` exist on `StatefulInteractiveElement`,
so `InteractiveExt` attaches handlers directly to an element's interactivity
state instead of inventing synthetic ids.

## Licence

MIT - see [LICENSE](LICENSE).
