# requestlab-core

Application service: the single entry point both interfaces use.

## Responsibility

`AppService` owns storage, the HTTP client, the collection service, the
environment service and the persisted settings. It exposes plain `async`
functions - no GPUI types - so the desktop UI and the CLI share exactly the same
logic.

## API surface

| Method | Purpose |
|---|---|
| `AppService::open(path)` | Open (or create) the database and load settings |
| `snapshot()` | Collections, environments, active environment, history, settings |
| `send(request, execution_id, cancel)` | Validate, resolve, execute, record history |
| `send_simple(request)` | The same without cancellation, for the CLI |
| `history(query)` / `clear_history()` / `delete_history_entry(id)` | History |
| `set_timeout_seconds` / `set_history_limit` / `set_theme` | Persisted settings |

`ExecutionOutcome` carries the request, the resolved request, the variables that
were substituted, the result and the history row id, plus the execution id so
stale results can be discarded by the UI.

## Example

```rust
# async fn example() -> Result<(), Box<dyn std::error::Error>> {
use requestlab_core::AppService;
use requestlab_models::{HttpMethod, Request};

let service = AppService::open(None).await?;
let outcome = service
    .send_simple(&Request::new(HttpMethod::Get, "https://example.com"))
    .await;
println!("{:?}", outcome.status());
# Ok(())
# }
```

## Tests

`cargo test -p requestlab-core` runs the unit tests plus
`tests/integration.rs`, which exercises restart persistence, history pruning,
environment resolution and network failures against a local server.

## Licence

MIT - see [LICENSE](LICENSE).
