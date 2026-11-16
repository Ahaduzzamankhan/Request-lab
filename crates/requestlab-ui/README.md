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

## GPUI and Ely

Pinned to `gpui` 0.3.8, which crates.io publishes as `gpui-pre`, and to
`ely-gpui-component`, whose components are built against the same snapshot, so
the window has exactly one GPUI and one component library. `theme.rs` maps Ely's
palette onto this crate's tokens, Ely's segmented control is the section switch
and the response tab bar, and Ely's buttons are the dialog actions.

`InteractiveExt` attaches click handlers and scroll styles to plain elements and
consumes the click so a button inside a clickable row does not also trigger the
row.

## Licence

MIT - see [LICENSE](LICENSE).
