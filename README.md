# RequestLab
[![OctoCounts](https://api.octocounts.com/badge/Ahaduzzamankhan/Request-lab/branch/main)](https://octocounts.com/github/Ahaduzzamankhan/Request-lab/tree/main?analysis=%7B%22ignoredDirs%22%3A%5B%5D%2C%22ignoredLanguages%22%3A%5B%5D%2C%22profile%22%3A%22default%22%2C%22includeDocs%22%3Afalse%2C%22includeTests%22%3Afalse%2C%22includeGenerated%22%3Afalse%7D)

RequestLab is a native API development studio for Windows written in Rust with
[GPUI](https://gpui.rs), Zed's GPU-accelerated UI framework, taken from the
`gpui-pre` 0.3.8 snapshot and styled with the
[Ely](https://github.com/ZacharyZhang-NY/Ely-GPUI-Components) component library. It sends HTTP requests, inspects responses
in detail, stores reusable request collections and environments in SQLite, and
ships with a first-class command line interface that uses the same engine.

No browser engine, no Electron, no Tauri, no local server process: one native
window, one SQLite file, one optional CLI binary.

```
┌──────────────────────────────────────────────────────────────────────┐
│ RequestLab        Collections  History  Environments          ⚙  ☀   │
├───────────────┬──────────────────────────────────────────────────────┤
│ Collections   │ POST  {{BASE_URL}}/users                    Send     │
│  ▸ Payments   │ Params  Headers  Body  Auth                          │
│  ▸ Users      │                                                      │
│               ├──────────────────────────────────────────────────────┤
│ History       │ 201 Created · 128 B · 42 ms · application/json        │
│ Environments  │ Pretty  Raw  Preview  Headers  Cookies               │
└───────────────┴──────────────────────────────────────────────────────┘
```

## Features

### Request builder
- All seven methods: `GET`, `POST`, `PUT`, `PATCH`, `DELETE`, `HEAD`, `OPTIONS`.
- URL validation, query parameter editor, per-row enable/disable.
- Headers: add, edit, delete, duplicate, enable/disable, with name/value validation.
- Bodies: none, JSON (validated before sending), raw text, URL-encoded form.
- Authentication: Basic, Bearer, API key in a header or as a query parameter.
- Asynchronous execution with timing, configurable timeouts and cancellation.

### Response inspector
- Status, status text, duration, size and content type at a glance.
- Pretty (expandable, searchable JSON tree), Raw, Preview, Headers and Cookies tabs.
- JSON, plain text, XML, HTML, empty and binary bodies are handled differently.
- Large bodies are streamed with a cap and clearly marked as truncated.

### Collections and history
- Collections with nested folders (depth-limited), duplicate/move/reorder.
- Save requests into a collection or a folder; rename, duplicate, delete.
- Every attempt is recorded in history with status, duration and error.
- History search, method/status filters, replay, delete one, clear all, and a
  bounded retention limit.

### Environments
- Create, rename, duplicate, delete environments and choose the active one.
- `{{VARIABLE}}` interpolation in URL, query parameters, headers and body.
- Missing, disabled and malformed variables are reported - never replaced by an
  empty string. `\{{NAME}}` escapes a literal.
- Secret variables are redacted in logs and CLI output.

### Command line interface
The CLI is a real interface over the same `AppService`, not a mock:

```bash
requestlab-cli send https://api.example.com/users -X POST \
  -H 'Content-Type: application/json' -d '{"name":"Ada"}' --env Development

requestlab-cli --json send https://api.example.com/health | jq .status
requestlab-cli history list --status 5xx --limit 10
requestlab-cli collections create "Payments"
requestlab-cli environments set Development API_TOKEN=abc --secret
requestlab-cli config set timeout 45
```

See [crates/requestlab-cli/README.md](crates/requestlab-cli/README.md) for the
full command reference.

## Platform support

| Platform | GUI | CLI |
|---|---|---|
| Windows 10 / 11 x64 | Primary target, built by `release-windows.yml` | Supported |
| Linux | Compiles (needs the GPUI system libraries) | Supported |
| macOS | Not built in CI | Supported |

The GUI has been compiled and its tests are part of the workspace, but the
interface has **not** been visually verified on Windows from this environment -
there is no Windows host or display available to the build agent. That
limitation is stated rather than hidden; see [docs/BUILD.md](docs/BUILD.md).

## Installation

Download `RequestLab-Setup-<version>-x64.exe` from the GitHub Releases page and
run it. The Inno Setup installer places the application and the CLI, creates the
Start Menu entries, offers a desktop shortcut and registers an uninstall entry.
It defaults to a per-user install and can install machine-wide from its
privileges dialog. Silent install and uninstall are supported:

```text
RequestLab-Setup-0.1.2-x64.exe /VERYSILENT /SUPPRESSMSGBOXES /NORESTART
"%LOCALAPPDATA%\Programs\RequestLab\unins000.exe" /VERYSILENT
```

The release also publishes the individual files:

- `requestlab.exe` - the desktop application (no console window). With arguments
  it runs the CLI, and `requestlab.exe update` runs the updater, so one file
  covers the GUI, the CLI and the update flow.
- `requestlab-cli.exe` - the standalone command line interface.
- `requestlab-update-helper.exe` - applies a staged update after the app closes.
- `manifest.json` / `manifest.json.sig` - the signed description of the release.
- `SHA256SUMS.txt` - checksums for the published files.

Every binary is self-contained; no runtime needs to be installed. The application
only installs releases whose manifest verifies against its trusted Ed25519 key.

## Installer and updates

Windows installs come from the Inno Setup script in [installer/](installer).
There is no second installer program: `requestlab-installer.exe` no longer
exists, and releases do not publish an updater binary either.

The updater lives inside `RequestLab.exe`:

```text
requestlab.exe update                # updater window
requestlab.exe update --check        # exit 0: update available, 3: current
requestlab.exe update --silent --pid 1234 --launch
```

An update downloads the signed package, verifies its size and SHA-256, and either
replaces the files in place (application closed) or stages them for
`requestlab-update-helper.exe`, which waits for the running application to exit
and swaps files with automatic rollback. Installing, upgrading and uninstalling
never touch `%APPDATA%\RequestLab`, so collections, environments, history and
settings survive all three.

## Build from source

Requirements: a stable Rust toolchain and a C linker. On Windows the MSVC
toolchain is required (Visual Studio Build Tools with the C++ workload).

```bash
cargo build --release -p requestlab-app -p requestlab-cli
```

On Linux, GPUI needs `libxcb`, `libxkbcommon` and `libxkbcommon-x11` development
packages for *linking* binaries. `cargo check` works without them.

Full instructions, including cross-checks and troubleshooting:
[docs/BUILD.md](docs/BUILD.md).

## Development

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace          # UI tests additionally need the GPUI system libs
cargo run -p requestlab-app     # desktop
cargo run -p requestlab-cli -- --help
```

Data lives in a single SQLite database:

- Windows: `%APPDATA%\RequestLab\requestlab.sqlite`
- Linux: `$XDG_DATA_HOME/RequestLab/requestlab.sqlite` (or `~/.local/share/...`)
- Override with `REQUESTLAB_DATA_DIR`, or `--data-dir` for the CLI.

Logs go to `requestlab.log` in the same directory (`REQUESTLAB_LOG=debug` to
raise verbosity, `REQUESTLAB_LOG_STDERR=1` to log to the terminal).

## Project structure

```
crates/
  requestlab-models         Shared data types, validation, redaction (no I/O)
  requestlab-http           Reqwest/Rustls engine, cancellation, timing
  requestlab-storage        SQLite pool, migrations, repositories
  requestlab-collections    Collections, folders, saved requests
  requestlab-environments   Environments and {{VARIABLE}} resolution
  requestlab-core           AppService shared by the GUI and the CLI
  requestlab-ui             GPUI interface (theme, components, panels, editor)
  requestlab-app            Desktop binary
  requestlab-cli            CLI binary + argument parser + integration tests
docs/                       Architecture, database schema, data models, build
version.json                Authoritative version for releases
```

Each crate has its own `README.md` and `LICENSE`.

## Architecture in one paragraph

`requestlab-models` defines the data. `requestlab-http`,
`requestlab-storage`, `requestlab-collections` and `requestlab-environments`
each own one concern and depend only on storage/models. `requestlab-core`
orchestrates them behind `AppService`, which exposes plain async functions.
`requestlab-ui` renders that state with GPUI and runs every async call on a
dedicated Tokio runtime, delivering results back to the UI thread; it never
executes SQL and never blocks on the network. `requestlab-app` and
`requestlab-cli` are two thin front ends over the same service.
Details: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Testing

| Suite | Command | Notes |
|---|---|---|
| Unit tests | `cargo test --workspace --lib` | Parsing, validation, redaction, migrations, resolution |
| Core integration | `cargo test -p requestlab-core --test integration` | Local HTTP server, restart persistence |
| CLI integration | `cargo test -p requestlab-cli` | Real commands, exit codes, history, environments |

Tests never depend on public APIs or the user's real data directory. Anything
that cannot run in a given environment is reported honestly in the final report
rather than silently skipped.

## Versioning and releases

`version.json` is the authoritative version for releases. `Cargo.toml`'s
`[workspace.package] version` must match it, and CI fails if they diverge.

```bash
# bump the version in both places, then
scripts/verify-version.sh
```

A release is produced by `.github/workflows/release-windows.yml`, which validates
the version, runs the test suite, builds both Windows binaries, writes
`SHA256SUMS.txt`, tags the commit, and attaches everything to a GitHub Release.
It refuses to overwrite an existing tag or release. See
[docs/BUILD.md](docs/BUILD.md#release-process).

## Contributing

1. Create a branch and keep changes focused.
2. `cargo fmt --all` and `cargo clippy --workspace --all-targets -- -D warnings`
   must be clean.
3. Add tests for behaviour changes - especially for anything touching HTTP
   execution, storage or variable resolution.
4. Describe the change in a conventional commit message (`feat:`, `fix:`,
   `docs:`, `chore:`).

Do not weaken TLS, log credentials, or replace persistence with in-memory
storage to make a test pass.

## Design system

One palette, defined once in `crates/requestlab-ui/src/theme.rs`, is pushed into
Ely's global theme at start-up, so Ely components and RequestLab widgets paint
from the same tokens.

| Token | Dark | Light |
|---|---|---|
| Background | `#080B12` | `#F5F8FD` |
| Sidebar / Surface 1 | `#0D111B` | `#FFFFFF` |
| Surface 2 | `#121827` | `#EDF3FA` |
| Surface 3 | `#192235` | `#E3ECF8` |
| Primary blue | `#2878FF` | `#216BEB` |
| Border | `#253249` | `#DCE5F0` |
| Text | `#F1F5FC` | `#142033` |

Inter is the UI face, JetBrains Mono the code face. Icons are Lucide glyphs from
Ely's bundled set, never emoji. The application icon lives in
`assets/branding/` (`requestlab-icon.png`, `requestlab.ico`) and is embedded into
`requestlab.exe` by `crates/requestlab-app/build.rs`.

## License

MIT - see [LICENSE](LICENSE). Third-party dependencies keep their own licenses.
Dependencies are pinned in `Cargo.toml`; `Cargo.lock` is committed.
