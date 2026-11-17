# RequestLab Architecture

## Goals

- One native Windows application, no browser runtime and no helper server.
- Local-first: every feature works offline against a single SQLite file.
- A GUI and a CLI that cannot drift apart because both call the same service.
- Strong separation: the UI never runs SQL, never awaits the network and never
  knows about database rows.

## Crate graph

```
             requestlab-app        requestlab-cli
                   │                     │
                   └────────┬────────────┘
                            ▼
                     requestlab-ui        (GPUI only)
                            │
                            ▼
                     requestlab-core
              ┌─────────────┼───────────────┐
              ▼             ▼               ▼
    requestlab-http  requestlab-storage  requestlab-collections
              │             │               │
              │             ▼               │
              │      requestlab-environments│
              └─────────────┴───────────────┘
                            ▼
                    requestlab-models
```

Management front ends have their own chain and never touch the application's
data path:

```
        requestlab-app  (GUI + CLI dispatch + `update` subcommand)
                   │
                   ▼
          requestlab-manager-ui
                   │
                   ▼
           requestlab-manager  ──►  GitHub API, signed manifests, filesystem
```

`requestlab-manager-ui` holds only presentation state, so every progress and
failure rule is testable without a display. The `requestlab-update-helper`
process runs the same engine after the application has exited.

Rules enforced by the layout:

- `requestlab-models` has no I/O, no async and no third-party runtime.
- Only `requestlab-storage` contains SQL.
- `requestlab-http` never depends on GPUI or storage.
- The interface runs on GPUI 0.3.8 (`gpui-pre` on crates.io) and the Ely
  component library, which is built against that exact snapshot. `requestlab-ui`
  derives every colour token from Ely's palette and uses Ely's segmented control
  and buttons, so both share one theme and one app context. `requestlab-app`
  embeds the Windows icon and version resource through `build.rs`.
- `requestlab-ui` depends on `requestlab-core` only; core re-exports the small
  set of types the UI needs (`EnvironmentError`, `Canceller`,
  `cancellation_channel`) so the UI does not reach into the service crates.
- `requestlab-app` and `requestlab-cli` are thin front ends.
- No cycles; `cargo check --workspace` builds in dependency order.

## Responsibilities

| Crate | Owns |
|---|---|
| `requestlab-models` | `Request`, `HttpResponse`, `Collection`/`Folder`/`SavedRequest`, `HistoryEntry`, `Environment`, validation, redaction, formatting |
| `requestlab-http` | Request preparation, authentication, execution, cancellation, bounded body capture, timing, typed errors |
| `requestlab-storage` | Pool creation, PRAGMAs, migrations, repositories, corrupt-database recovery |
| `requestlab-collections` | Collection/folder/request tree operations and their invariants |
| `requestlab-environments` | Environment CRUD, active environment, `{{VAR}}` resolution, validation |
| `requestlab-core` | `AppService`: the only orchestration point |
| `requestlab-ui` | Theme, components, text editor element, panels, dialogs, view state |
| `requestlab-app` | Process bootstrap, window creation, logging, shutdown; dispatches the CLI and runs the embedded updater via `requestlab.exe update` |
| `requestlab-cli` | Argument parsing, command execution, text/JSON rendering, exit codes |
| `requestlab-manager` | Install/update engine: manifest parsing, Ed25519 verification, GitHub releases, verified downloads, transactional install, rollback, update staging, platform integration (shortcuts, registry, process waits) |
| `requestlab-manager-ui` | Installer/updater state model: stages, bounded log, progress, warnings, terminal result (no GPUI dependency, unit tested) |
| `requestlab-updater` | Library (no binary): update window, `--check` and `--silent`; staged and verified updates, handed to the helper |
| `requestlab-update-helper` | Applies a staged update after the app exits, writes a recovery log, and carries the release tooling (`build-manifest`, `sign-manifest`, `public-key`, `generate-key`) |

## Application state

`AppView` (one GPUI entity) owns:

- `tabs: Vec<Tab>` with an active index. Each `Tab` holds a `RequestEditor`,
  a `ResponseState`, the in-flight `Canceller` and the current execution id.
- Sidebar state: collections trees, expanded collections/folders, history rows
  and filters, environments and the active environment.
- UI-only state: dialog, prompt input, toast, loading flag, theme.

Persistent models stay separate from editor state: text fields live in
`TextInput` entities and a `Request` is rebuilt on demand with `to_request`,
so `{{VARIABLE}}` resolution never mutates what the user typed or what is
saved in SQLite.

## Request execution pipeline

```
editor text ──► Request::to_request()      (no env resolution)
                    │
                    ▼
              Request::validate()          (URL, headers, JSON body)
                    │
                    ▼
   EnvironmentService::resolve(&request)   (active env, single pass)
                    │
                    ▼
        HttpClient::execute(&resolved)     (auth, redirects, timeout)
                    │
                    ▼
        bounded body capture + timing      (ResponseBody)
                    │
        ┌───────────┴────────────┐
        ▼                        ▼
  Response view            history insert + prune
```

Guarantees:

- The original `Request` is never mutated by resolution.
- Each send carries a fresh execution id; a response whose id does not match
  the tab's current id is discarded, so a slow reply cannot overwrite a newer
  one.
- Cancellation uses a `tokio::sync::oneshot` receiver inside a `tokio::select!`
  in the HTTP client.
- History is written by the core, independently of whether the UI renders it.

## Threading model

GPUI runs on the main thread and has no Tokio reactor. The application creates
one multi-threaded Tokio runtime at startup:

- `requestlab-app` opens `AppService` on that runtime before the window exists.
- `requestlab-ui` receives `Arc<Runtime>` and uses `AppView::spawn_core`:
  the future runs on the runtime, and `cx.spawn` awaits the `oneshot` result
  before mutating the entity. Rendering never blocks.
- The CLI simply `block_on`s the same service from its own runtime.

SQLite (SQLx) and Reqwest both run inside that runtime; no blocking call touches
the UI thread.

## Security boundaries

- TLS verification is always enabled (Rustls); there is no opt-out.
- Header values that can hold credentials (`authorization`, `x-api-key`,
  `apikey`, cookies) are redacted before logging or storing history.
- History rows store a redacted copy of the request with a truncated body.
- Response HTML is displayed as text, never executed.
- Secret environment variables are redacted in logs and CLI output; the UI and
  README state that SQLite storage is not encrypted at rest.
- SQL is always parameterised; the only interpolated SQL is the compile-time
  migration version in `PRAGMA user_version`.

## Error handling

Each layer has its own error type (`HttpError`, `StorageError`,
`CollectionError`, `EnvironmentError`, `CoreError`, `CliError`). The UI maps
them to a category chip and a readable message; the CLI maps them to exit codes
(0 success, 1 runtime failure, 2 usage error, 22 HTTP error status with
`--fail`). No error is swallowed to make a build pass.
