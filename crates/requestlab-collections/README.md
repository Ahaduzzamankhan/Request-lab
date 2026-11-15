# requestlab-collections

Collection, folder and saved-request logic.

## Responsibility

Coordinates the tree the user sees in the sidebar: collections contain folders,
folders nest to a bounded depth, and any node can hold saved requests. All
writes go through the storage repositories.

## What lives here

- `manager::CollectionService` - create/rename/delete/duplicate collections,
  create/rename/delete/move folders, save/update/duplicate/delete/move/reorder
  requests.
- `folders` - pure helpers for depth limits and cycle detection, used before any
  write so an invalid tree can never reach SQLite.
- `errors` - typed `CollectionError` values the UI turns into toasts.

## Example

```rust
# async fn example(service: &requestlab_collections::CollectionService) -> Result<(), Box<dyn std::error::Error>> {
use requestlab_models::{HttpMethod, Request};

let collection = service.create_collection("Payments").await?;
let folder = service.create_folder(collection.id, None, "Charges").await?;
service
    .save_request(
        collection.id,
        Some(folder.id),
        "Create charge",
        &Request::new(HttpMethod::Post, "https://api.example.com/charges"),
    )
    .await?;
# Ok(())
# }
```

## Invariants

- A folder can never be moved inside its own subtree.
- Nesting is capped (see `folders::MAX_DEPTH`) so the sidebar stays usable.
- Deleting a collection cascades through folders and saved requests via the
  database's foreign keys.

## Licence

MIT - see [LICENSE](LICENSE).
