# requestlab-models

Shared, dependency-free data types used by every other RequestLab crate.

## Responsibility

`requestlab-models` is the bottom of the dependency graph: it defines the
serialisable structures that are stored in SQLite, sent over HTTP and rendered
by the UI. It has no knowledge of GPUI, SQLx, Reqwest or Tokio, so every other
crate can depend on it without pulling in a runtime.

## What lives here

| Module | Contents |
|---|---|
| `common` | Validation helpers, header/name rules, URL checks, byte and duration formatting, credential redaction |
| `request` | `HttpMethod`, `Request`, `HeaderEntry`, `QueryParameter`, `RequestBody`, `Authentication` |
| `response` | `HttpResponse`, `ResponseBody`, content-type classification, cookie parsing, pretty JSON |
| `collection` | `Collection`, `Folder`, `SavedRequest`, `CollectionTree`, tree building and depth checks |
| `history` | `HistoryEntry`, `HistoryQuery`, `HistoryStatusFilter`, redacting history seeds |
| `environment` | `Environment`, `EnvironmentVariable` |

## Example

```rust
use requestlab_models::{HeaderEntry, HttpMethod, Request};

let mut request = Request::new(HttpMethod::Get, "https://api.example.com/users");
request.headers.push(HeaderEntry::new("Accept", "application/json"));

let issues = request.validate();
assert!(issues.is_empty());
```

## Rules

- No I/O, no async, no global state.
- Everything persisted is `serde`-serialisable and stable across releases.
- Values that may hold credentials are redacted by the helpers here before they
  can reach a log line or a history row.

## Licence

MIT - see [LICENSE](LICENSE).
