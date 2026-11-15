# requestlab-environments

Environment storage and `{{VARIABLE}}` interpolation.

## Responsibility

Keeps the list of environments, tracks which one is active, and resolves
variables in the URL, query parameters, headers and body of a request.

## Resolution rules

1. `{{NAME}}` is replaced only when the environment is active and the variable
   is enabled.
2. `\{{NAME}}` is an escape and produces a literal `{{NAME}}`.
3. Resolution is single pass - substituted text is never re-scanned, so values
   cannot trigger recursive interpolation.
4. A missing variable, a disabled variable and a malformed expression are
   reported as errors. Nothing is replaced with an empty string.
5. The original request is never mutated; `resolve` returns a new request.

## Example

```rust
# async fn example(service: &requestlab_environments::EnvironmentService) -> Result<(), Box<dyn std::error::Error>> {
use requestlab_models::{HttpMethod, Request};

let environment = service.create("Development").await?;
service.set_active(Some(environment.id)).await?;

let request = Request::new(HttpMethod::Get, "{{BASE_URL}}/users");
let resolved = service.resolve(&request).await?;
println!("calling {}", resolved.request.url);
# Ok(())
# }
```

## Secrets

Variables marked `is_secret` are redacted in logs and in CLI output. They are
stored as plain text in the local SQLite file - the UI and README state this
explicitly rather than implying encryption at rest.

## Licence

MIT - see [LICENSE](LICENSE).
