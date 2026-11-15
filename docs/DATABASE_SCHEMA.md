# Database Schema

RequestLab stores everything in one SQLite file. The schema version lives in
`PRAGMA user_version`; migrations are an append-only list in
`crates/requestlab-storage/src/migrations.rs` and each one runs inside a
transaction.

Connection settings applied to every connection:

```sql
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;
```

## Tables (schema version 1)

### collections

```sql
CREATE TABLE collections (
    id          TEXT PRIMARY KEY NOT NULL,
    name        TEXT NOT NULL,
    description TEXT,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);
```

### folders

```sql
CREATE TABLE folders (
    id            TEXT PRIMARY KEY NOT NULL,
    collection_id TEXT NOT NULL,
    parent_id     TEXT,
    name          TEXT NOT NULL,
    sort_order    INTEGER NOT NULL DEFAULT 0,
    created_at    TEXT NOT NULL,
    updated_at    TEXT NOT NULL,
    FOREIGN KEY (collection_id) REFERENCES collections (id) ON DELETE CASCADE,
    FOREIGN KEY (parent_id) REFERENCES folders (id) ON DELETE CASCADE
);
```

`parent_id` is `NULL` for a top-level folder. Self-referencing cascade keeps the
subtree consistent when a folder is deleted. The nesting depth is limited in
`requestlab-collections::folders`, before any write.

### saved_requests

```sql
CREATE TABLE saved_requests (
    id            TEXT PRIMARY KEY NOT NULL,
    collection_id TEXT NOT NULL,
    folder_id     TEXT,
    name          TEXT NOT NULL,
    request_json  TEXT NOT NULL,
    sort_order    INTEGER NOT NULL DEFAULT 0,
    created_at    TEXT NOT NULL,
    updated_at    TEXT NOT NULL,
    FOREIGN KEY (collection_id) REFERENCES collections (id) ON DELETE CASCADE,
    FOREIGN KEY (folder_id) REFERENCES folders (id) ON DELETE SET NULL
);
```

`request_json` is a serialised `Request`. Deleting a folder keeps its requests
and moves them to the collection root (`ON DELETE SET NULL`).

### request_history

```sql
CREATE TABLE request_history (
    id            TEXT PRIMARY KEY NOT NULL,
    method        TEXT NOT NULL,
    url           TEXT NOT NULL,
    request_json  TEXT NOT NULL,
    status_code   INTEGER,
    duration_ms   INTEGER,
    error_message TEXT,
    created_at    TEXT NOT NULL
);
```

`request_json` stores a *redacted* request (credential headers replaced, body
truncated). `status_code` and `error_message` are mutually exclusive: a
transport failure has an error and no status.

### environments / environment_variables

```sql
CREATE TABLE environments (
    id         TEXT PRIMARY KEY NOT NULL,
    name       TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE environment_variables (
    id             TEXT PRIMARY KEY NOT NULL,
    environment_id TEXT NOT NULL,
    key            TEXT NOT NULL,
    value          TEXT NOT NULL,
    enabled        INTEGER NOT NULL DEFAULT 1,
    is_secret      INTEGER NOT NULL DEFAULT 0,
    position       INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY (environment_id) REFERENCES environments (id) ON DELETE CASCADE
);
```

`position` preserves the user's ordering. Secrets are stored as plain text; this
is documented in the UI and is not presented as encryption.

### application_settings

```sql
CREATE TABLE application_settings (
    key   TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
);
```

Known keys: `timeout_seconds`, `history_retention` and `theme`. Unknown keys are
preserved so future versions can add settings without a migration.

## Indexes

```sql
CREATE INDEX idx_folders_collection         ON folders (collection_id, parent_id, sort_order);
CREATE INDEX idx_saved_requests_collection  ON saved_requests (collection_id, folder_id, sort_order);
CREATE INDEX idx_saved_requests_folder      ON saved_requests (folder_id, sort_order);
CREATE INDEX idx_history_created            ON request_history (created_at DESC);
CREATE INDEX idx_history_method             ON request_history (method);
CREATE INDEX idx_history_status             ON request_history (status_code);
CREATE INDEX idx_env_vars_environment       ON environment_variables (environment_id, position);
```

These cover the queries the app actually runs: building each collection tree,
listing history newest-first with optional method/status filters, and loading an
environment's variables in order.

## Relationships

```
collections 1 ── n folders           (cascade)
folders     1 ── n folders           (cascade, parent/child)
collections 1 ── n saved_requests    (cascade)
folders     1 ── n saved_requests    (set null)
environments 1 ── n environment_variables (cascade)
request_history, application_settings   (standalone)
```

## Migration history

| Version | Change |
|---|---|
| 1 | Initial schema: collections, folders, saved_requests, request_history, environments, environment_variables (with `position`), application_settings, plus the indexes above |

To add a migration: append a new `Migration { version: 2, statements: &[...] }`
entry, keep every earlier statement untouched, run the storage tests, and
update this table.

## Recovery

`open_or_recover` attempts to open the database and apply migrations. If SQLite
reports corruption, the file (and its `-wal`/`-shm` siblings) is renamed to
`requestlab.sqlite.corrupt-<timestamp>` and a fresh database is created. The
recovery path is returned to the caller and surfaced in the UI, so data loss is
never silent.
