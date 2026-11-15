# Data Models

All models live in `crates/requestlab-models`. They are plain Rust structs and
enums, `serde`-serialisable, and free of runtime dependencies.

## Request

```rust
pub struct Request {
    pub id: Uuid,
    pub name: String,
    pub method: HttpMethod,
    pub url: String,
    pub query_params: Vec<QueryParameter>,
    pub headers: Vec<HeaderEntry>,
    pub body: RequestBody,
    pub authentication: Authentication,
}
```

- `HttpMethod`: `Get`, `Post`, `Put`, `Patch`, `Delete`, `Head`, `Options`.
  `Head::forbids_body()` is used during validation.
- `QueryParameter` / `HeaderEntry`: `id`, `key`, `value`, `enabled`.
  Disabled rows are kept (so a user can toggle them back) but excluded when the
  request is built.
- `RequestBody`:
  - `None`
  - `Json { content }` - validated as JSON before sending
  - `Raw { content, content_type }`
  - `FormUrlEncoded { fields: Vec<QueryParameter> }`
- `Authentication`:
  - `None`
  - `Basic { username, password }`
  - `Bearer { token }`
  - `ApiKey { key, value, location: ApiKeyLocation }` where `ApiKeyLocation` is
    `Header` or `Query`.

`Request::validate()` returns `Vec<ValidationIssue>`; an empty vector means the
request can be sent.

## Response

```rust
pub struct HttpResponse {
    pub status: u16,
    pub status_text: String,
    pub headers: Vec<ResponseHeader>,
    pub body: ResponseBody,
    pub content_type: Option<String>,
    pub size_bytes: usize,
    pub duration_ms: u64,
    pub completed_at: String,
    pub url: String,
    pub cookies: Vec<Cookie>,
}

pub enum ResponseBody {
    Empty,
    Text { text: String, truncated: bool },
    Binary { bytes: Vec<u8>, truncated: bool },
}
```

Helpers: `is_success`, `is_redirect`, `is_client_error`, `is_server_error`,
`pretty_json`, `as_text`, `len`, plus `classify_body` which decides between
text, JSON, XML, HTML and binary from the content type and the payload.

## Collections

```rust
pub struct Collection { id, name, description, created_at, updated_at }
pub struct Folder { id, collection_id, parent_id, name, sort_order, created_at, updated_at }
pub struct SavedRequest { id, collection_id, folder_id, name, request, sort_order, created_at, updated_at }
pub struct CollectionTree { collection: Collection, root: CollectionNode }
pub struct CollectionNode { folder: Option<Folder>, requests: Vec<SavedRequest>, children: Vec<CollectionNode> }
```

`build_tree(collection, folders, requests)` assembles the tree from flat rows;
`walk_requests()` flattens it. Depth and cycle checks live in
`requestlab-collections`.

## History

```rust
pub struct HistoryEntry {
    pub id: Uuid,
    pub method: HttpMethod,
    pub url: String,
    pub request: Request,          // redacted copy
    pub status_code: Option<u16>,
    pub duration_ms: Option<u64>,
    pub error_message: Option<String>,
    pub created_at: String,
}

pub struct HistoryQuery {
    pub search: String,
    pub method: Option<HttpMethod>,
    pub status: HistoryStatusFilter,   // All, Success, Redirect, ClientError, ServerError, Failed
    pub limit: i64,
    pub offset: i64,
}
```

`HistoryEntry::from_attempt` is the only constructor used by the application: it
redacts credential headers and truncates oversized bodies before the row is
written.

## Environments

```rust
pub struct Environment { id, name, variables: Vec<EnvironmentVariable>, created_at, updated_at }
pub struct EnvironmentVariable { id, key, value, enabled, is_secret }
```

## Execution outcome

`requestlab-core` wraps a send in:

```rust
pub struct ExecutionOutcome {
    pub execution_id: Uuid,
    pub request_id: Uuid,
    pub request: Request,                 // as the user had it
    pub resolved_request: Option<Request>,
    pub variables: Vec<String>,           // names substituted
    pub result: Result<HttpResponse, ExecutionError>,
    pub history_id: Option<Uuid>,
}
```

`execution_id` lets the UI discard stale results; `history_id` points at the row
that was written for the attempt.
