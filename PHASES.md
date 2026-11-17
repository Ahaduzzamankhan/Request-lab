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

## Phase 4 — Authentication suite

Digest, OAuth 2.0 (authorization code + client credentials + password), OAuth 1.0
where practical, AWS Signature v4, and custom auth headers, with credentials kept
out of logs, history and exports. CLI flags mirror the GUI fields.

Acceptance: each flow is tested against a local server or a deterministic
signature vector; secrets are redacted everywhere they could be printed.

## Phase 5 — Response inspector

Pretty/raw/HTML/XML/text/binary views, collapsible JSON tree, in-response search,
syntax highlighting, download and copy, cookies, headers, timing, and the
test-results tab. Large responses stay bounded and off the UI thread.

Acceptance: headless tests render each content type; a 10 MB JSON body does not
block the UI or exceed the documented memory bound.

## Phase 6 — Collections, workspaces and variables

Workspaces, collection descriptions, drag-and-drop ordering, collection-level
auth and variables, local/global/environment variables with autocomplete,
import/export of RequestLab and Postman formats, and a collection runner.

Acceptance: SQLite migrations preserve existing data; a mixed Postman import
round-trips; every variable scope resolves in URL, headers, body and auth.

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
