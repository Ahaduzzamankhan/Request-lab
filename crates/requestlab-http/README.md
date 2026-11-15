# requestlab-http

The HTTP execution engine.

## Responsibility

Turns a validated `Request` into a real HTTP call and captures everything the
UI and CLI need: status, headers, body, timing and cancellation.

## What lives here

- `client` - `HttpClient` built on Reqwest with Rustls. Two inner clients (one
  that follows redirects, one that does not) plus `cancellation_channel` so the
  UI can abort an in-flight request.
- `builder` - `prepare_request` resolves authentication into headers, applies
  enabled headers and query parameters, and produces a redacted view for logs.
- `authentication` - Basic, Bearer and API-key (header or query) application,
  including Base64 encoding.
- `response` - Bounded body streaming with truncation flags, content-type
  detection and cookie collection.
- `timing` / `errors` - duration measurement and a typed, cloneable
  `HttpError` with categories the UI can display.

## Example

```rust
use requestlab_http::{cancellation_channel, HttpClient, HttpConfig, SendOptions};
use requestlab_models::{HttpMethod, Request};

# async fn example() -> Result<(), Box<dyn std::error::Error>> {
let client = HttpClient::new(HttpConfig::default())?;
let request = Request::new(HttpMethod::Get, "https://example.com");
let (_canceller, receiver) = cancellation_channel();
let response = client
    .execute(&request, &SendOptions::default(), receiver)
    .await?;
println!("{} in {} ms", response.status, response.duration_ms);
# Ok(())
# }
```

## Rules

- TLS verification is always on; there is no "ignore certificate errors" switch.
- Response bodies are streamed with a size cap so a huge download cannot
  exhaust memory.
- Credentials never appear in log targets; `redacted_headers` is the only view
  that reaches tracing.

## Licence

MIT - see [LICENSE](LICENSE).
