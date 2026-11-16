# requestlab-app

The Windows desktop binary (`requestlab.exe`).

## Responsibility

Process entry point: initialises tracing, opens storage on a Tokio runtime,
creates the GPUI application through `gpui_platform`, loads the Ely component
library's assets and fonts, and shows the single workspace window. Release
builds use the `windows` subsystem so no console window appears.

## Files

| File | Contents |
|---|---|
| `main.rs` | Entry point, startup failure handling, exit code |
| `application.rs` | `RequestLabApplication::bootstrap()` and `.run()` |
| `window.rs` | Window options (centred 1180x760, resizable, titled) and window creation |
| `lifecycle.rs` | Tracing to `requestlab.log` in the data directory, shutdown logging |

## Running

```bash
cargo run -p requestlab-app
```

Environment variables:

- `REQUESTLAB_DATA_DIR` - override the data directory (used by tests).
- `REQUESTLAB_LOG` - tracing filter, e.g. `requestlab=debug`.
- `REQUESTLAB_LOG_STDERR` - log to the terminal instead of the log file.

## Building for Windows

See [../../docs/BUILD.md](../../docs/BUILD.md) and the
`release-windows.yml` workflow.

## Licence

MIT - see [LICENSE](LICENSE).
