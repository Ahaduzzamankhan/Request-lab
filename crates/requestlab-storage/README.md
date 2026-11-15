# requestlab-storage

SQLite persistence for collections, folders, saved requests, history,
environments and settings.

## Responsibility

Owns the database file, the schema, the migrations and the repositories. It is
the only crate that writes SQL.

## What lives here

- `database` - pool creation with `journal_mode = WAL`, `PRAGMA foreign_keys =
  ON` on every connection, `default_data_dir()` (`REQUESTLAB_DATA_DIR`,
  `%APPDATA%\RequestLab`, `$XDG_DATA_HOME/RequestLab`, `~/.local/share`), and
  `open_or_recover()` which moves a corrupt database aside instead of crashing.
- `migrations` - versioned migrations driven by `PRAGMA user_version`.
- `repositories/{collections,environments,history,settings}` - parameterised
  SQL only; no string interpolation of user input.

## Example

```rust
# async fn example() -> Result<(), Box<dyn std::error::Error>> {
use requestlab_storage as storage;
use requestlab_models::{HistoryQuery, HistoryStatusFilter};

let (db, recovered) = storage::open_or_recover(&storage::default_database_path()).await?;
assert!(recovered.is_none(), "a fresh database is not a recovery");
let rows = storage::repositories::history::list(db.pool(), &HistoryQuery {
    status: HistoryStatusFilter::Success,
    ..HistoryQuery::default()
}).await?;
println!("{} successful rows", rows.len());
# Ok(())
# }
```

## Schema and migration rules

See [../../docs/DATABASE_SCHEMA.md](../../docs/DATABASE_SCHEMA.md). Schema
changes always arrive as a new migration with a bumped `user_version`; existing
data is never dropped silently.

## Licence

MIT - see [LICENSE](LICENSE).
