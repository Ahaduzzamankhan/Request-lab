# RequestLab upgrade programme — 10 phases

Status legend: **done** (implemented, tested, pushed) · **next** (in progress) ·
**planned** (not started).

The work below covers the full request: Postman-style API client features, the
UI redesign, one executable with an embedded updater, an Inno Setup installer,
release engineering, database safety, performance and documentation. Phases are
ordered so that every phase leaves the repository buildable and shippable.

## Phase 1 — Release foundation (done)

One executable `RequestLab.exe` (GUI + CLI dispatch + `update` subcommand),
`requestlab-cli.exe` as the standalone CLI, the Rust installer and the separate
updater binary deleted, Inno Setup (`installer/requestlab.iss`) as the only
Windows installer, `release-windows.yml` compiling it with ISCC, and one version
source (`version.json`) checked against the workspace by
`scripts/verify-version.sh`.

Acceptance: `cargo fmt --check`, `cargo clippy --workspace --all-targets -D
warnings`, Linux + Windows CI green, `requestlab.exe update --version` works,
dry-run release produces `RequestLab-Setup-<version>-x64.exe`.

## Phase 2 — cURL interoperability (done)

`requestlab-models::curl` parses a cURL command into a `Request` and renders a
`Request` back to cURL, with unit tests for methods, headers, bodies, query
parameters, basic/bearer auth, quoting and line continuations. Exposed in the CLI
through `send --curl "<command>"` and `curl "<command>"` (inspect without
sending).

Acceptance: parsing never silently drops an option — unknown or unsupported
flags fail with a readable message; round-trip curly → request → cURL → request
is stable.

## Phase 3 — Request editing completion

Custom HTTP methods, multipart/form-data and binary bodies in the model, engine
and editor; redirect, timeout and TLS policy per request; request duplication
from the editor and the CLI so a variant never overwrites the original.

Acceptance: every new body type is sent by the engine and covered by an
integration test against a local server; the CLI and GUI share the same builder.

## Phase 4 — Authentication suite (done)

Digest, OAuth 2.0 (client credentials + password), AWS Signature v4 and custom
auth headers, with credentials kept out of logs, history and exports. CLI flags
mirror the GUI fields.

- `requestlab-http::digest` parses `WWW-Authenticate` challenges (MD5, MD5-sess,
  SHA-256, SHA-256-sess, `qop=auth`) and computes the RFC 7616 response; the
  client sends the unauthenticated request, then repeats it once with the
  computed header.
- `requestlab-http::aws_sigv4` signs method, canonical URI/query, host, payload
  hash and session token, and was checked against an independently computed
  HMAC vector.
- `requestlab-http::oauth2` fetches tokens for the client-credentials and
  password grants, caches them in memory until shortly before expiry, and
  reports `error`/`error_description` without echoing secrets.
- CLI: `--digest`, `--aws-sigv4` (+ `--aws-session-token`), `--oauth2` with
  `--client-id`, `--client-secret`, `--scope`, `--oauth2-grant`,
  `--oauth2-user`, `--oauth2-password`.
- The editor exposes Digest as a first-class auth choice and preserves AWS /
  OAuth 2.0 configuration when editing other tabs.

Acceptance met: Digest retry and refusal, OAuth 2.0 fetch/reuse/rejection and
SigV4 headers are covered by tests against a local HTTP server
(`crates/requestlab-http/tests/auth.rs`), and the Digest/AWS algorithms have unit
vectors. **Not implemented:** the interactive authorization-code flow (needs a
browser and a redirect listener) and OAuth 1.0; both are reported rather than
stubbed.

## Phase 5 — Response inspector (partial)

Pretty/raw/HTML/XML/text/binary views, collapsible JSON tree, in-response search,
syntax highlighting, download and copy, cookies, headers, timing, and the
test-results tab. Large responses stay bounded and off the UI thread.

Shipped in `requestlab-models::inspector`: per-content-type view lists, bounded
body previews (JSON pretty, XML/HTML indentation, binary descriptions), literal
in-response search with line/column/previews and a cap, and download filenames
that honour `Content-Disposition` without allowing path escapes. Tests cover
invalid JSON, binary bodies and a 2 MB body being cut instead of formatted.

**Still missing:** the GUI surface for the new views and search box (the existing
panel keeps its Pretty/Raw tabs), syntax highlighting, and the test-results tab.

## Phase 6 — Collections, workspaces and variables (partial)

Workspaces, collection descriptions, drag-and-drop ordering, collection-level
auth and variables, local/global/environment variables with autocomplete,
import/export of RequestLab and Postman formats, and a collection runner.

Shipped: `requestlab-environments::scopes` resolves global, collection,
environment and local values with Postman's precedence, falls back when a
higher-scope definition is disabled, and feeds both resolution and `{{`
completion with deduplicated names. `requestlab-models::postman` imports and
exports Postman v2.1 collections (folders, queries, headers, raw/JSON/form
bodies, basic/bearer/apikey/digest auth, collection variables) and reports the
schemes Postman cannot represent instead of dropping them; export → import
round-trips in tests.

**Still missing:** database-backed workspaces and collection-level
variables/auth (needs a migration), drag-and-drop reordering in the sidebar, and
the collection runner.

## Phase 7 — Scripts, tests and automation

Isolated pre-request/post-response scripting with a sandbox that exposes only
request, response, environment and test APIs; assertions; test reports; repeated
runs; console output. No filesystem, process or network access from scripts.

Acceptance: the sandbox refuses OS access with a test that proves it; failing
assertions produce a report with request-level detail.

## Phase 8 — Documentation and OpenAPI

Request and collection descriptions, generated API documentation, OpenAPI import
and export, Swagger-compatible specs, and schema visualisation where practical.

Acceptance: an OpenAPI document imports into a collection and exports back to an
equivalent document; invalid documents are rejected with actionable errors.

## Phase 9 — UI redesign and interaction quality

Postman-inspired information architecture (sidebar sections, request tabs, top
navigation), a documented design-token system, resizable and remembered panels,
command palette, global search, keyboard shortcuts, drag-and-drop, context menus,
empty/loading/error states, and both themes.

Acceptance: every visible control performs its action (headless click tests);
layout sizes persist across restarts; no placeholder screens remain.

## Phase 10 — Remaining client protocols, performance and release

WebSocket, server-sent events, GraphQL support, cookie manager, proxy and
certificate settings, request and response comparison, code generation for
common languages, plus startup/memory/large-response budgets, database migration
tests against previous versions, documentation for every shipped feature, and a
verified release.

Acceptance: performance budgets are measured and met, migrations are tested from
a 0.1.x database, docs match the implementation, and the release publishes the
Inno Setup installer with checksums and a signed manifest.

## How phases are reported

Each phase ends with: the checks that were run, what was verified by execution,
what could not be verified in this environment (for example the Windows GUI,
which has no display here), and anything still missing. A phase is never marked
done because it compiles.
