# AGENTS.md
# RequestLab — Complete Engineering Architecture & Development Specification

**Version:** 1.0  
**Project:** RequestLab  
**Application Type:** Native Desktop API Development Studio  
**Language:** Rust  
**UI Framework:** GPUI  
**Target Platform:** Windows 10 / Windows 11 x64  
**Development Scope:** Phase 1–4  
**Architecture:** Modular Rust Workspace  
**Agent Model:** Single Autonomous Coding Agent

---

# 1. YOUR ROLE

You are the sole autonomous senior software engineer responsible for designing, implementing, testing, debugging, and maintaining RequestLab.

You must operate as:

- Senior Rust Engineer.
- GPUI Desktop Application Engineer.
- Software Architect.
- Networking Engineer.
- Database Engineer.
- UI/UX Engineer.
- Security Engineer.
- QA Engineer.
- Windows Release Engineer.

There are NO separate development agents.

You own the entire codebase and are responsible for every module.

You must understand how the entire application works before changing interconnected modules.

Do not pretend to delegate work to nonexistent agents.

Do not produce only architectural documents or implementation plans when actual code changes are required.

Your primary responsibility is to deliver a fully functional application.

# 2. PRODUCT VISION

RequestLab is a native API development environment designed as a modern alternative to:

- Postman
- Insomnia
- Bruno

RequestLab focuses on:

1. Fast HTTP request execution.
2. Professional response inspection.
3. Persistent request collections.
4. Nested collection folders.
5. Request history.
6. Environment management.
7. Environment variable resolution.
8. Local-first storage.
9. A polished native Windows interface.

## Product principles

- Native desktop experience.
- Performance-oriented Rust implementation.
- Offline-first architecture.
- Professional developer experience.
- Minimal resource consumption.
- Strong data integrity.
- Secure credential handling.
- Predictable behavior.
- Modular architecture.
- No unnecessary abstractions.

RequestLab must be a real application, not a UI demonstration.

# 3. TARGET PLATFORM

Primary target:

- Windows 10 x64.
- Windows 11 x64.

Development environment:

- Rust stable toolchain.
- Cargo.
- GPUI.
- Windows x64 compilation.

The application must launch as a normal desktop program.

It must not require a browser.

It must not require a separate backend server.

It must not require the user to run a terminal to use the release application.

# 4. TECHNOLOGY STACK

Use this technology stack:

| Component | Technology |
|---|---|
| Language | Rust |
| Desktop UI | GPUI |
| Async runtime | Tokio |
| HTTP client | Reqwest |
| HTTP protocol implementation | Hyper through compatible dependencies |
| TLS | Rustls |
| Serialization | Serde |
| JSON | serde_json |
| Database | SQLite |
| Database interface | SQLx |
| Error handling | thiserror + anyhow |
| Logging | tracing |
| Identifiers | UUID |
| URL parsing | url |
| Date/time | chrono or time |
| Testing | Rust tests + integration tests |

## Technology rules

1. Use stable Rust.
2. Use compatible dependency versions.
3. Pin the GPUI version.
4. Verify GPUI APIs against the selected version.
5. Never invent methods or component APIs.
6. Avoid unnecessary dependencies.
7. Avoid duplicate libraries.
8. Prefer Rustls.
9. Do not introduce Electron.
10. Do not introduce Tauri.
11. Do not introduce React or browser-based UI.
12. Do not replace GPUI with egui or iced.
13. Do not introduce a separate backend application.

If a dependency cannot work with the selected GPUI version, investigate a compatible alternative and document the decision.

# 5. COMPLETE PROJECT ARCHITECTURE

Use a modular Cargo workspace.

```text
requestlab/
│
├── Cargo.toml
├── Cargo.lock
├── AGENTS.md
├── README.md
├── LICENSE
│
├── assets/
│   ├── icons/
│   ├── fonts/
│   └── branding/
│
├── docs/
│   ├── ARCHITECTURE.md
│   ├── DATABASE_SCHEMA.md
│   ├── DATA_MODELS.md
│   └── BUILD.md
│
├── crates/
│   │
│   ├── requestlab-app/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs
│   │       ├── application.rs
│   │       ├── window.rs
│   │       └── lifecycle.rs
│   │
│   ├── requestlab-ui/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── layout/
│   │       ├── components/
│   │       ├── theme/
│   │       ├── navigation/
│   │       ├── request/
│   │       ├── response/
│   │       ├── collections/
│   │       ├── history/
│   │       ├── environments/
│   │       └── dialogs/
│   │
│   ├── requestlab-core/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── application_state.rs
│   │       ├── commands.rs
│   │       ├── events.rs
│   │       ├── services/
│   │       └── errors.rs
│   │
│   ├── requestlab-http/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── client.rs
│   │       ├── builder.rs
│   │       ├── execution.rs
│   │       ├── authentication.rs
│   │       ├── headers.rs
│   │       ├── body.rs
│   │       ├── response.rs
│   │       ├── timing.rs
│   │       └── errors.rs
│   │
│   ├── requestlab-storage/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── database.rs
│   │       ├── migrations.rs
│   │       ├── repositories/
│   │       └── errors.rs
│   │
│   ├── requestlab-collections/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── manager.rs
│   │       ├── folders.rs
│   │       └── requests.rs
│   │
│   ├── requestlab-environments/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── manager.rs
│   │       ├── resolver.rs
│   │       └── validation.rs
│   │
│   └── requestlab-models/
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs
│           ├── request.rs
│           ├── response.rs
│           ├── collection.rs
│           ├── history.rs
│           ├── environment.rs
│           └── common.rs
│
├── tests/
│   ├── http_integration.rs
│   ├── storage_integration.rs
│   ├── collection_integration.rs
│   └── environment_integration.rs
│
└── .gitignore
```

This is the target structure, not permission to create empty files.

Only create modules that have meaningful responsibilities.

# 6. CRATE RESPONSIBILITIES

## 6.1 requestlab-app

Purpose: Windows desktop application entry point.

Responsibilities:

- Initialize GPUI.
- Initialize application state.
- Create the main window.
- Initialize local storage.
- Initialize services.
- Handle application startup.
- Handle shutdown.
- Handle startup errors.
- Configure application metadata.

The application must handle missing or corrupted database files gracefully.

## 6.2 requestlab-ui

Purpose: Entire native graphical interface.

Responsibilities:

- Window layout.
- Navigation.
- Request editor.
- Response viewer.
- Collection explorer.
- History.
- Environment selector.
- Settings.
- Theme management.
- Notifications.
- Dialogs.

The UI must not directly execute SQL queries.

The UI must not directly perform blocking network operations.

Use GPUI-supported state and event mechanisms.

## 6.3 requestlab-core

Purpose: Central application coordination.

Responsibilities:

- Application state.
- Service orchestration.
- Request execution coordination.
- Event handling.
- UI-to-service communication.
- State synchronization.
- Error propagation.

The core must coordinate independent modules without coupling every crate to every other crate.

## 6.4 requestlab-http

Purpose: HTTP execution engine.

Responsibilities:

- HTTP client configuration.
- Request construction.
- Request execution.
- Authentication.
- Header handling.
- Request body handling.
- Response capture.
- Timing.
- Cancellation.
- Network error handling.

All network operations must be asynchronous.

## 6.5 requestlab-storage

Purpose: Local persistence.

Responsibilities:

- SQLite initialization.
- Database migrations.
- Connection pooling.
- CRUD repositories.
- Transactions.
- Database error handling.
- Storage integrity.

All database operations must execute asynchronously where supported.

## 6.6 requestlab-collections

Purpose: Collection organization.

Responsibilities:

- Collection CRUD.
- Folder CRUD.
- Nested folders.
- Saved requests.
- Request movement.
- Request duplication.
- Reordering.

## 6.7 requestlab-environments

Purpose: Environment configuration and variable resolution.

Responsibilities:

- Environment CRUD.
- Variable management.
- Active environment selection.
- Variable interpolation.
- Validation.
- Undefined variable detection.
- Secret classification.

## 6.8 requestlab-models

Purpose: Shared data structures.

Responsibilities:

- Request models.
- Response models.
- Collection models.
- History models.
- Environment models.
- Shared identifiers.
- Serialization types.

Shared models must not depend on GPUI.

# 7. DEPENDENCY DIRECTION

Maintain the following architectural dependency direction:

```text
          requestlab-app
                 │
          requestlab-ui
                 │
          requestlab-core
          /      |       \
         /       |        \
      HTTP    Storage   Environments
         \       |        /
          \      |       /
          requestlab-models
```

The collections crate coordinates collection-specific operations through defined interfaces and storage repositories.

Rules:

- Models must remain independent.
- UI must not depend directly on SQLx implementation details.
- HTTP must not depend on GPUI.
- Storage must not depend on GPUI.
- Core coordinates services.
- Avoid circular dependencies.
- Do not expose internal database implementation details to UI components.

# 8. SHARED DATA MODELS

Implement strongly typed models.

## HTTP method

```rust
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
    Options,
}
```

## Request model

Conceptual structure:

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

## Query parameter

```rust
pub struct QueryParameter {
    pub id: Uuid,
    pub key: String,
    pub value: String,
    pub enabled: bool,
}
```

## Header entry

```rust
pub struct HeaderEntry {
    pub id: Uuid,
    pub key: String,
    pub value: String,
    pub enabled: bool,
}
```

## Request body

Support:

- None.
- JSON.
- Raw text.
- URL encoded form.

Use an appropriate enum rather than ambiguous string flags.

## Authentication

Support:

- None.
- Basic.
- Bearer.
- API Key.

Keep authentication values separate from ordinary request display data where possible.

## Response model

Capture:

- Status code.
- Status text when available.
- Response headers.
- Response body.
- Content type.
- Response size.
- Duration.
- Completion timestamp.
- Request error when applicable.

Represent binary and textual response bodies appropriately.

## Collection model

Support:

- Collection identifier.
- Collection name.
- Description.
- Creation time.
- Modification time.

## Folder model

Support:

- Folder identifier.
- Parent folder identifier.
- Collection identifier.
- Name.
- Ordering index.

## Environment model

Support:

- Environment identifier.
- Environment name.
- Variables.
- Active state.
- Modification time.

All identifiers must be stable.

# 9. APPLICATION STATE ARCHITECTURE

Keep application state separated by responsibility.

Conceptual state:

```text
ApplicationState
│
├── NavigationState
├── WorkspaceState
│   ├── OpenRequests
│   ├── ActiveRequest
│   └── RequestTabs
│
├── RequestEditorState
│
├── ResponseState
│
├── CollectionState
│
├── HistoryState
│
├── EnvironmentState
│
└── SettingsState
```

Requirements:

- Use GPUI-supported entities and notifications.
- Avoid arbitrary global mutable state.
- Avoid one enormous application state object.
- Avoid unnecessary state duplication.
- Keep persistent models separate from temporary editor state.
- Prevent stale asynchronous results from overwriting newer request state.
- Associate request execution results with the correct request execution identifier.

# 10. PHASE 1 — HTTP REQUEST BUILDER

Implement a real HTTP client.

## Supported methods

- GET
- POST
- PUT
- PATCH
- DELETE
- HEAD
- OPTIONS

## URL editor

Requirements:

- Editable URL.
- URL validation.
- URL parsing.
- Localhost support.
- Query parameter editing.
- Proper URL encoding.
- Environment variable support.

## Request tabs

Implement:

- Params.
- Headers.
- Body.
- Auth.

## Headers

Users must be able to:

- Add headers.
- Edit headers.
- Delete headers.
- Enable and disable headers.
- Duplicate headers.

Validate header names and values.

## Request body

Implement:

- None.
- JSON.
- Raw text.
- Form URL encoded.

Validate JSON before execution.

## Authentication

Implement:

### Basic authentication

Support username and password.

### Bearer authentication

Support bearer tokens.

### API key authentication

Support key/value configuration and header or query placement.

## Request execution

The Send button must:

1. Validate the URL.
2. Validate the request configuration.
3. Resolve environment variables.
4. Build the request.
5. Execute it asynchronously.
6. Capture the response.
7. Measure timing.
8. Update the response state.
9. Record the execution in history.

Implement:

- Configurable timeouts.
- Cancellation.
- Redirect handling.
- Connection error handling.
- TLS validation.
- Duplicate execution protection.

Never block the UI thread.

Do not silently swallow errors.

# 11. PHASE 2 — RESPONSE VIEWER

Implement a complete response inspector.

## Response information

Display:

- HTTP status code.
- Status text where available.
- Response time.
- Response size.
- Response content type.

## Response tabs

- Pretty.
- Raw.
- Preview where supported.
- Headers.
- Cookies where available.

## JSON inspector

Implement:

- Pretty printing.
- Syntax highlighting.
- Expandable objects.
- Expandable arrays.
- Search.
- Copy full JSON.
- Copy individual values where supported.
- Correct value-type presentation.

Support:

- Null.
- Boolean.
- Number.
- String.
- Object.
- Array.

## Content types

Handle:

- JSON.
- Plain text.
- XML.
- HTML.
- Empty response.
- Binary response.

Never assume every response is JSON.

Do not render untrusted HTML as executable application content.

Binary responses should support safe saving to a user-selected location.

## Large response handling

- Avoid blocking the UI.
- Limit expensive previews.
- Avoid building enormous JSON trees unnecessarily.
- Preserve access to the raw response.
- Clearly indicate when a preview is limited.

# 12. PHASE 3 — COLLECTIONS AND HISTORY

## Collections

Users must be able to:

- Create collections.
- Rename collections.
- Delete collections.
- Create folders.
- Rename folders.
- Delete folders.
- Create nested folders.
- Save requests.
- Rename requests.
- Duplicate requests.
- Delete requests.
- Move requests.
- Reorder requests.

Use confirmation dialogs for destructive actions.

Implement reasonable nesting-depth limits.

## Request history

Record request attempts automatically.

Store:

- Request method.
- Request URL.
- Request headers where appropriate.
- Request body where appropriate.
- Timestamp.
- Response status.
- Duration.
- Error information.

Features:

- Search history.
- Filter by method.
- Filter by status.
- Replay requests.
- Clear history.
- Delete individual entries.

History retention must be configurable or bounded.

Do not store huge response bodies indefinitely.

Never persist credentials unnecessarily.

## Persistence

All important data must survive application restarts.

Use SQLite.

Implement:

- Schema migrations.
- Transactions.
- Foreign keys.
- Appropriate indexes.
- Stable identifiers.
- Error recovery.

# 13. PHASE 4 — ENVIRONMENT MANAGEMENT

## Environment management

Users must be able to:

- Create environments.
- Rename environments.
- Delete environments.
- Duplicate environments.
- Select active environments.
- Add variables.
- Edit variables.
- Delete variables.
- Enable and disable variables.
- Mark variables as secret.

Example:

```text
Environment: Development

BASE_URL = http://localhost:3000
API_TOKEN = example-token
API_VERSION = v1
```

## Variable syntax

```text
{{BASE_URL}}
{{API_TOKEN}}
{{API_VERSION}}
```

Resolve variables in:

- Request URLs.
- Query parameters.
- Headers.
- Request bodies.

## Resolution rules

1. Resolve variables before request construction.
2. Do not modify the saved original request.
3. Detect missing variables.
4. Detect malformed expressions.
5. Show useful errors.
6. Preserve original editor text.
7. Correctly handle variable values requiring URL encoding.
8. Define and test escaping behavior.
9. Avoid accidental recursive interpolation.
10. Prevent secrets from leaking into logs.

## Secret handling

Clearly distinguish secret variables.

Do not claim SQLite text storage is encrypted.

Use Windows-protected credential storage when feasible.

Document the security boundary and fallback behavior.

# 14. SQLITE DATABASE DESIGN

Use versioned migrations.

Suggested tables:

## collections

```sql
CREATE TABLE collections (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
```

## folders

```sql
CREATE TABLE folders (
    id TEXT PRIMARY KEY,
    collection_id TEXT NOT NULL,
    parent_id TEXT,
    name TEXT NOT NULL,
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY(collection_id) REFERENCES collections(id)
        ON DELETE CASCADE,
    FOREIGN KEY(parent_id) REFERENCES folders(id)
        ON DELETE CASCADE
);
```

## saved_requests

```sql
CREATE TABLE saved_requests (
    id TEXT PRIMARY KEY,
    collection_id TEXT NOT NULL,
    folder_id TEXT,
    name TEXT NOT NULL,
    request_json TEXT NOT NULL,
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY(collection_id) REFERENCES collections(id)
        ON DELETE CASCADE,
    FOREIGN KEY(folder_id) REFERENCES folders(id)
        ON DELETE SET NULL
);
```

## request_history

```sql
CREATE TABLE request_history (
    id TEXT PRIMARY KEY,
    method TEXT NOT NULL,
    url TEXT NOT NULL,
    request_json TEXT NOT NULL,
    status_code INTEGER,
    duration_ms INTEGER,
    error_message TEXT,
    created_at TEXT NOT NULL
);
```

## environments

```sql
CREATE TABLE environments (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
```

## environment_variables

```sql
CREATE TABLE environment_variables (
    id TEXT PRIMARY KEY,
    environment_id TEXT NOT NULL,
    key TEXT NOT NULL,
    value TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    is_secret INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY(environment_id) REFERENCES environments(id)
        ON DELETE CASCADE
);
```

## application_settings

```sql
CREATE TABLE application_settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
```

These are initial schema proposals. Verify indexes, constraints, foreign keys, and migration behavior before implementation.

Never change a released schema without a migration.

Enable SQLite foreign key enforcement on every connection.

Use parameterized SQL queries.

Do not concatenate untrusted input into SQL statements.

# 15. USER INTERFACE ARCHITECTURE

The UI must follow a professional desktop developer-tool layout.

## Main application layout

```text
┌───────────────────────────────────────────────────────────┐
│ RequestLab       Search       Environment       Settings   │
├───────────────┬───────────────────────────────────────────┤
│               │                                           │
│ Collections   │ GET  https://api.example.com/users        │
│               │                                           │
│ My APIs       │ Params Headers Body Auth                  │
│  ├ Users      │                                           │
│  ├ Products   │ Request configuration                     │
│  └ Auth       │                                           │
│               ├───────────────────────────────────────────┤
│ History       │ Response                                   │
│               │                                           │
│ Environments  │ Pretty Raw Headers Cookies                │
│               │                                           │
│ Settings      │ JSON response viewer                      │
│               │                                           │
└───────────────┴───────────────────────────────────────────┘
```

## Design direction

Use:

- Charcoal dark theme.
- Newspaper-white light theme.
- Balanced rounded corners.
- Modern sans-serif typography.
- Monospace code typography.
- Restrained accent colors.
- Consistent borders.
- Clear hierarchy.
- Compact but comfortable spacing.

Avoid:

- Generic dashboard design.
- Excessive empty space.
- Unnecessary gradients.
- Oversized cards.
- Random icon usage.
- Inconsistent corner radii.
- Decorative controls without functionality.

## Interaction requirements

Implement:

- Working navigation.
- Working tabs.
- Resizable panels.
- Keyboard shortcuts where practical.
- Context menus.
- Tooltips.
- Loading indicators.
- Error indicators.
- Empty states.
- Confirmation dialogs.
- Notifications.

Every visible interactive control must perform a meaningful operation.

# 16. REQUEST EXECUTION DATA FLOW

Use the following execution pipeline:

```text
User edits request
       |
       v
Request Editor State
       |
       v
Request Validation
       |
       v
Environment Resolution
       |
       v
Authentication Preparation
       |
       v
HTTP Request Builder
       |
       v
Asynchronous HTTP Execution
       |
       v
Response Capture
       |
       v
Response Parsing
       |
       +------------------+
       |                  |
       v                  v
Response Viewer      History Storage
```

Requirements:

- Keep original request data unchanged.
- Use a separate resolved execution representation.
- Track execution identifiers.
- Handle cancellation.
- Handle network failures.
- Avoid stale UI updates.
- Save history independently of UI rendering.

# 17. APPLICATION DATA FLOW

```text
               GPUI Interface
                     |
                     v
              Application Core
                     |
          +----------+----------+
          |          |          |
          v          v          v
       HTTP       Collections  Environments
      Engine          |          |
          |           |          |
          +-----------+----------+
                      |
                      v
                 Storage Layer
                      |
                      v
                   SQLite
```

Use typed interfaces and well-defined ownership.

Do not let the UI directly access internal SQL queries.

# 18. SECURITY REQUIREMENTS

Security is mandatory even in the initial release.

Implement:

- TLS certificate validation.
- No disabled TLS verification by default.
- No secret logging.
- Credential redaction.
- Safe handling of authorization headers.
- Safe handling of API keys.
- Secure local credential storage where feasible.
- Safe binary downloads.
- Request timeout limits.
- Clear redirect behavior.
- No execution of response HTML as application code.

Avoid sending credentials to unintended redirect destinations.

Do not automatically execute downloaded files.

Do not expose sensitive information in crash reports.

# 19. ERROR HANDLING

Use meaningful typed errors.

Separate:

- Validation errors.
- Network errors.
- Timeout errors.
- Authentication errors.
- Serialization errors.
- Database errors.
- Environment resolution errors.
- UI/application errors.

Errors must be understandable to users.

Do not silently swallow failures.

Do not use broad panic-catching as a substitute for correct error handling.

Use tracing for diagnostics, with sensitive information redacted.

# 20. PERFORMANCE REQUIREMENTS

Prioritize:

- Asynchronous networking.
- Asynchronous database operations.
- Responsive GPUI rendering.
- Efficient JSON processing.
- Bounded history storage.
- Efficient collection loading.
- Minimal unnecessary cloning.
- Cancellation support.
- Lazy loading where useful.

Do not block the UI thread.

Do not load unlimited history into memory.

Do not create unnecessary background polling.

Do not optimize blindly without measurements.

# 21. WINDOWS APPLICATION REQUIREMENTS

The final product must:

- Launch normally on Windows.
- Use a native desktop window.
- Support resizing.
- Support minimize and maximize.
- Have a proper application icon.
- Have a correct window title.
- Store application data in an appropriate Windows application data directory.
- Handle missing database files.
- Handle corrupted database files safely.
- Shut down cleanly.
- Provide understandable startup errors.

Normal release users must not need a console window.

Build and verify the Windows x64 release target.

Do not claim Windows compatibility beyond the platforms actually tested.

# 23. TESTING ARCHITECTURE

Tests are required for all critical functionality.

## Unit tests

HTTP:

- URL validation.
- Query encoding.
- Header validation.
- Request construction.
- Authentication.
- JSON validation.

Response:

- JSON parsing.
- XML handling.
- Text handling.
- Empty response.
- Binary response.

Collections:

- Collection creation.
- Folder nesting.
- Request movement.
- Deletion.
- Duplication.

History:

- Recording.
- Filtering.
- Search.
- Replay data reconstruction.

Environments:

- Variable resolution.
- Missing variables.
- Disabled variables.
- Malformed expressions.
- Secret redaction.

## Integration tests

Use local test servers.

Test:

- GET requests.
- POST requests.
- HTTP error responses.
- Timeouts.
- Network failures.
- Request cancellation.
- SQLite persistence.
- Collection CRUD.
- History persistence.
- Environment switching.
- Variable resolution in URLs, headers, parameters, and bodies.

Do not depend on public APIs for automated tests.

# 24. DEVELOPMENT WORKFLOW

Follow this exact implementation sequence.

## STEP 1 — REPOSITORY INSPECTION

Before changing files:

- Inspect repository structure.
- Inspect Cargo.toml.
- Check Rust version.
- Check GPUI version.
- Check existing build status.
- Identify existing working functionality.

Do not overwrite existing work unnecessarily.

## STEP 2 — FOUNDATION

Implement:

- Cargo workspace.
- Dependency configuration.
- Shared models.
- Error types.
- Application entry point.
- GPUI window.
- Database initialization.

Compile immediately.

## STEP 3 — UI FOUNDATION

Implement:

- Theme.
- Design tokens.
- Main layout.
- Sidebar.
- Toolbar.
- Navigation.
- Request workspace.
- Response workspace.

Run the actual Windows application and inspect the interface.

## STEP 4 — PHASE 1

Implement the complete HTTP request builder.

Test against a local HTTP server.

## STEP 5 — PHASE 2

Implement the response viewer.

Test all supported response types.

## STEP 6 — PHASE 3

Implement SQLite persistence, collections, folders, and history.

Test restart persistence.

## STEP 7 — PHASE 4

Implement environment management and variable resolution.

Test missing and malformed variables.

## STEP 8 — INTEGRATION

Connect every UI feature to actual backend functionality.

Verify that saved data survives application restarts.

## STEP 9 — RELEASE

Build and verify the Windows release application.

Document the build output.

# 25. STRICT ENGINEERING RULES

These rules must never be violated.

1. Do not write the entire application in one Rust file.
2. Do not create fake implementations.
3. Do not use static demo responses as actual API responses.
4. Do not create decorative buttons that do nothing.
5. Do not use temporary memory storage for persistent features.
6. Do not block the UI thread.
7. Do not invent GPUI APIs.
8. Do not silently downgrade dependencies.
9. Do not suppress errors to make compilation appear successful.
10. Do not delete working code unnecessarily.
11. Do not duplicate existing functionality.
12. Do not expose credentials.
13. Do not claim tests passed without running them.
14. Do not claim Windows compatibility without verification.
15. Do not leave mandatory Phase 1–4 features as TODOs.
16. Do not create unnecessary abstractions.
17. Do not introduce unnecessary dependencies.
18. Do not rewrite large modules for minor fixes.
19. Do not make unrelated changes.
20. Do not declare completion without integration testing.

If an implementation is blocked by a GPUI limitation, investigate the actual limitation and document a practical solution.

# 26. REQUIRED DOCUMENTATION

Maintain:

## README.md

Include:

- Product overview.
- Features.
- Screenshots when available.
- Technology stack.
- Installation.
- Development setup.
- Windows build instructions.

## docs/ARCHITECTURE.md

Include:

- Crate responsibilities.
- Dependency direction.
- Application state.
- Request execution pipeline.
- Data flow.

## docs/DATABASE_SCHEMA.md

Include:

- Tables.
- Relationships.
- Constraints.
- Indexes.
- Migration history.

## docs/BUILD.md

Include:

- Rust installation.
- Windows requirements.
- Build commands.
- Release commands.
- Troubleshooting.

Keep documentation consistent with actual implementation.

# 27. DEFINITION OF DONE

RequestLab Phase 1–4 is complete only when:

[ ] Native Windows application launches.

[ ] GPUI interface is functional.

[ ] HTTP methods work.

[ ] URL editing works.

[ ] Query parameters work.

[ ] Headers work.

[ ] Request bodies work.

[ ] Authentication works.

[ ] Asynchronous request execution works.

[ ] Cancellation works.

[ ] Response status and timing work.

[ ] JSON response inspection works.

[ ] Raw response viewing works.

[ ] Headers can be inspected.

[ ] Collections work.

[ ] Nested folders work.

[ ] Requests can be saved and edited.

[ ] Request history works.

[ ] History search works.

[ ] History replay works.

[ ] SQLite persistence works.

[ ] Environments work.

[ ] Variable resolution works.

[ ] Missing variables produce errors.

[ ] Credentials are protected from accidental exposure.

[ ] Application remains responsive.

[ ] Automated tests pass.

[ ] Windows release build succeeds.

[ ] Documentation matches the implementation.

Do not mark an item complete unless it has been implemented and verified.

# 28. FINAL DELIVERY REPORT

When the implementation is complete, report:

1. Features implemented.
2. Workspace structure.
3. Important architectural decisions.
4. Database schema.
5. Dependencies introduced.
6. Tests executed.
7. Actual test results.
8. Compilation results.
9. Windows release build result.
10. Actual executable size.
11. Known limitations.
12. Remaining bugs.
13. Exact application build location.
14. Instructions for launching RequestLab.

Clearly distinguish:

- Implemented and verified.
- Implemented but not verified.
- Partially implemented.
- Not implemented.

Never exaggerate completion.

---

# FINAL DIRECTIVE

You are the sole engineering agent responsible for RequestLab.

Understand the architecture, inspect the existing codebase, implement features incrementally, compile frequently, test thoroughly, and maintain a professional native Windows application.

**The final objective is a fully functional RequestLab Phase 1–4 desktop application built with Rust and GPUI, not merely a convincing interface or a successful compilation.**
