# RequestLab

RequestLab is a native API development studio for Windows written in Rust with
[GPUI](https://crates.io/crates/gpui). It sends HTTP requests, inspects responses
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

Download the release archive from the GitHub Releases page:

- `requestlab.exe` - the desktop application (no console window).
- `requestlab-cli.exe` - the command line interface.
- `SHA256SUMS.txt` - checksums for both binaries.

Both binaries are self-contained; no runtime needs to be installed.

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

## License

MIT - see [LICENSE](LICENSE). Third-party dependencies keep their own licenses;
`cargo deny`-style attribution is out of scope for this repository, but the
`vendor/xattr` patch keeps the upstream MIT/Apache-2.0 license files.
