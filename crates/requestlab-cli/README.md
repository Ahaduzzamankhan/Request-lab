# requestlab-cli

A complete command line interface for RequestLab (`requestlab-cli.exe`).

The CLI is not a stub: it uses `requestlab-core`'s `AppService`, so it sends the
same requests, writes the same SQLite history and resolves the same
environments as the desktop app.

## Commands

```text
requestlab-cli [--json] [--data-dir PATH] <command> [options]

send <url>                    Send an HTTP request
collections list|create|delete
history list|clear
environments list|create|use|variables|set|unset
config get|set
```

## Examples

```bash
# Simple GET
requestlab-cli send https://api.example.com/users

# POST JSON with headers and an environment
requestlab-cli send https://api.example.com/users -X POST \
  -H 'Content-Type: application/json' \
  -d '{"name":"Ada"}' --env Development

# Machine-readable response for scripts
requestlab-cli --json send https://api.example.com/health | jq .status

# Fail the script on a 4xx/5xx response
requestlab-cli send https://api.example.com/missing --fail

# Collections, history and settings
requestlab-cli collections create "Payments"
requestlab-cli collections list
requestlab-cli history list --status 5xx --limit 10
requestlab-cli environments create Staging
requestlab-cli environments use Staging
requestlab-cli environments set Staging API_TOKEN=secret --secret
requestlab-cli environments unset Staging API_TOKEN
requestlab-cli config set timeout 45
```

## Behaviour

- Exit codes: `0` success, `1` runtime/network failure, `2` usage error,
  `22` HTTP error response when `--fail` is used.
- `--json` emits a single JSON document; secrets are redacted even there.
- The URL, headers, body and method are validated before anything is sent.
- `--data-dir` points the CLI at another database, which is how the integration
  tests isolate themselves.

## Tests

```bash
cargo test -p requestlab-cli
```

Unit tests cover the parser and renderers; `tests/cli.rs` runs real commands
against a local TCP server and asserts on exit codes, output, recorded history
and environment resolution.

## Licence

MIT - see [LICENSE](LICENSE).

## cURL import

Paste a cURL command and RequestLab turns it into a request. Unsupported options
fail with a readable error instead of being dropped.

```text
requestlab-cli send --curl "curl -X POST https://api.example.com/users \
  -H 'Content-Type: application/json' -d '{\"name\":\"Ada\"}'"

# Inspect what would be sent, without sending it
requestlab-cli curl "curl -u user:secret 'https://api.example.com/search?q=rust'"
requestlab-cli --json curl "curl -I https://api.example.com"
```

Supported options: `-X/--request`, `--url`, `-H/--header`, `-d/--data*`,
`--data-urlencode`, `-F/--form`, `-u/--user`, `--bearer`, `-b/--cookie`,
`-A/--user-agent`, `-e/--referer`, `-I/--head`, `-G/--get`, plus the connection
flags that do not change the saved request (`-L`, `-k`, `-s`, `-o`, `-m`,
`--retry`, ...). A body without `-X` implies POST, `Authorization: Bearer ...`
becomes the auth field, and query strings move into the parameter table.
