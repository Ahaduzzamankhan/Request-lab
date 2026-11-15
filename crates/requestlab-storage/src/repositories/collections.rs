//! SQL for collections, folders and saved requests.

use requestlab_models::{Collection, Folder, Request, SavedRequest};
use sqlx::SqlitePool;
use uuid::Uuid;

use super::{opt_text_at, opt_uuid_at, text_at, uuid_at};
use crate::errors::StorageError;

const COLLECTION_COLUMNS: &str = "id, name, description, created_at, updated_at";
const FOLDER_COLUMNS: &str = "id, collection_id, parent_id, name, sort_order, created_at, updated_at";
const SAVED_COLUMNS: &str =
    "id, collection_id, folder_id, name, request_json, sort_order, created_at, updated_at";

fn collection_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<Collection, StorageError> {
    Ok(Collection {
        id: uuid_at(row, "id")?,
        name: text_at(row, "name")?,
        description: opt_text_at(row, "description")?,
        created_at: text_at(row, "created_at")?,
        updated_at: text_at(row, "updated_at")?,
    })
}

fn folder_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<Folder, StorageError> {
    Ok(Folder {
        id: uuid_at(row, "id")?,
        collection_id: uuid_at(row, "collection_id")?,
        parent_id: opt_uuid_at(row, "parent_id")?,
        name: text_at(row, "name")?,
        sort_order: row_sort(row)?,
        created_at: text_at(row, "created_at")?,
        updated_at: text_at(row, "updated_at")?,
    })
}

fn row_sort(row: &sqlx::sqlite::SqliteRow) -> Result<i64, StorageError> {
    use sqlx::Row;
    Ok(row.try_get("sort_order")?)
}

fn saved_request_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<SavedRequest, StorageError> {
    let request_json = text_at(row, "request_json")?;
    let request: Request = serde_json::from_str(&request_json)
        .map_err(|err| StorageError::InvalidData(format!("request_json: {err}")))?;
    Ok(SavedRequest {
        id: uuid_at(row, "id")?,
        collection_id: uuid_at(row, "collection_id")?,
        folder_id: opt_uuid_at(row, "folder_id")?,
        name: text_at(row, "name")?,
        request,
        sort_order: row_sort(row)?,
        created_at: text_at(row, "created_at")?,
        updated_at: text_at(row, "updated_at")?,
    })
}

// ---------------------------------------------------------------- collections

pub async fn list_collections(pool: &SqlitePool) -> Result<Vec<Collection>, StorageError> {
    let rows = sqlx::query(&format!(
        "SELECT {COLLECTION_COLUMNS} FROM collections ORDER BY name COLLATE NOCASE"
    ))
    .fetch_all(pool)
    .await?;
    rows.iter().map(collection_from_row).collect()
}

pub async fn get_collection(
    pool: &SqlitePool,
    id: Uuid,
) -> Result<Collection, StorageError> {
    let row = sqlx::query(&format!(
        "SELECT {COLLECTION_COLUMNS} FROM collections WHERE id = ?"
    ))
    .bind(id.to_string())
    .fetch_optional(pool)
    .await?
    .ok_or(StorageError::NotFound {
        entity: "Collection",
    })?;
    collection_from_row(&row)
}

pub async fn insert_collection(
    pool: &SqlitePool,
    collection: &Collection,
) -> Result<(), StorageError> {
    sqlx::query(
        "INSERT INTO collections (id, name, description, created_at, updated_at) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(collection.id.to_string())
    .bind(&collection.name)
    .bind(&collection.description)
    .bind(&collection.created_at)
    .bind(&collection.updated_at)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn rename_collection(
    pool: &SqlitePool,
    id: Uuid,
    name: &str,
    updated_at: &str,
) -> Result<(), StorageError> {
    let affected = sqlx::query("UPDATE collections SET name = ?, updated_at = ? WHERE id = ?")
        .bind(name)
        .bind(updated_at)
        .bind(id.to_string())
        .execute(pool)
        .await?
        .rows_affected();
    if affected == 0 {
        return Err(StorageError::NotFound {
            entity: "Collection",
        });
    }
    Ok(())
}

pub async fn update_collection_description(
    pool: &SqlitePool,
    id: Uuid,
    description: Option<&str>,
    updated_at: &str,
) -> Result<(), StorageError> {
    sqlx::query("UPDATE collections SET description = ?, updated_at = ? WHERE id = ?")
        .bind(description)
        .bind(updated_at)
        .bind(id.to_string())
        .execute(pool)
        .await?;
    Ok(())
}

/// Delete a collection. Folders and saved requests cascade.
pub async fn delete_collection(pool: &SqlitePool, id: Uuid) -> Result<(), StorageError> {
    let affected = sqlx::query("DELETE FROM collections WHERE id = ?")
        .bind(id.to_string())
        .execute(pool)
        .await?
        .rows_affected();
    if affected == 0 {
        return Err(StorageError::NotFound {
            entity: "Collection",
        });
    }
    Ok(())
}

// -------------------------------------------------------------------- folders

pub async fn list_folders(
    pool: &SqlitePool,
    collection_id: Uuid,
) -> Result<Vec<Folder>, StorageError> {
    let rows = sqlx::query(&format!(
        "SELECT {FOLDER_COLUMNS} FROM folders WHERE collection_id = ? ORDER BY sort_order, name"
    ))
    .bind(collection_id.to_string())
    .fetch_all(pool)
    .await?;
    rows.iter().map(folder_from_row).collect()
}

pub async fn get_folder(pool: &SqlitePool, id: Uuid) -> Result<Folder, StorageError> {
    let row = sqlx::query(&format!("SELECT {FOLDER_COLUMNS} FROM folders WHERE id = ?"))
        .bind(id.to_string())
        .fetch_optional(pool)
        .await?
        .ok_or(StorageError::NotFound { entity: "Folder" })?;
    folder_from_row(&row)
}

pub async fn insert_folder(pool: &SqlitePool, folder: &Folder) -> Result<(), StorageError> {
    sqlx::query(
        "INSERT INTO folders (id, collection_id, parent_id, name, sort_order, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(folder.id.to_string())
    .bind(folder.collection_id.to_string())
    .bind(folder.parent_id.map(|id| id.to_string()))
    .bind(&folder.name)
    .bind(folder.sort_order)
    .bind(&folder.created_at)
    .bind(&folder.updated_at)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn rename_folder(
    pool: &SqlitePool,
    id: Uuid,
    name: &str,
    updated_at: &str,
) -> Result<(), StorageError> {
    let affected = sqlx::query("UPDATE folders SET name = ?, updated_at = ? WHERE id = ?")
        .bind(name)
        .bind(updated_at)
        .bind(id.to_string())
        .execute(pool)
        .await?
        .rows_affected();
    if affected == 0 {
        return Err(StorageError::NotFound { entity: "Folder" });
    }
    Ok(())
}

pub async fn move_folder(
    pool: &SqlitePool,
    id: Uuid,
    parent_id: Option<Uuid>,
    updated_at: &str,
) -> Result<(), StorageError> {
    sqlx::query("UPDATE folders SET parent_id = ?, updated_at = ? WHERE id = ?")
        .bind(parent_id.map(|id| id.to_string()))
        .bind(updated_at)
        .bind(id.to_string())
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn set_folder_order(
    pool: &SqlitePool,
    id: Uuid,
    sort_order: i64,
) -> Result<(), StorageError> {
    sqlx::query("UPDATE folders SET sort_order = ? WHERE id = ?")
        .bind(sort_order)
        .bind(id.to_string())
        .execute(pool)
        .await?;
    Ok(())
}

/// Delete a folder; nested folders and requests cascade with it.
pub async fn delete_folder(pool: &SqlitePool, id: Uuid) -> Result<(), StorageError> {
    let affected = sqlx::query("DELETE FROM folders WHERE id = ?")
        .bind(id.to_string())
        .execute(pool)
        .await?
        .rows_affected();
    if affected == 0 {
        return Err(StorageError::NotFound { entity: "Folder" });
    }
    Ok(())
}

// ------------------------------------------------------------- saved requests

pub async fn list_saved_requests(
    pool: &SqlitePool,
    collection_id: Uuid,
) -> Result<Vec<SavedRequest>, StorageError> {
    let rows = sqlx::query(&format!(
        "SELECT {SAVED_COLUMNS} FROM saved_requests WHERE collection_id = ? ORDER BY sort_order, name"
    ))
    .bind(collection_id.to_string())
    .fetch_all(pool)
    .await?;
    rows.iter().map(saved_request_from_row).collect()
}

pub async fn get_saved_request(
    pool: &SqlitePool,
    id: Uuid,
) -> Result<SavedRequest, StorageError> {
    let row = sqlx::query(&format!(
        "SELECT {SAVED_COLUMNS} FROM saved_requests WHERE id = ?"
    ))
    .bind(id.to_string())
    .fetch_optional(pool)
    .await?
    .ok_or(StorageError::NotFound {
        entity: "Saved request",
    })?;
    saved_request_from_row(&row)
}

pub async fn insert_saved_request(
    pool: &SqlitePool,
    saved: &SavedRequest,
) -> Result<(), StorageError> {
    let request_json = serde_json::to_string(&saved.request)
        .map_err(|err| StorageError::InvalidData(format!("request: {err}")))?;
    sqlx::query(
        "INSERT INTO saved_requests
            (id, collection_id, folder_id, name, request_json, sort_order, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(saved.id.to_string())
    .bind(saved.collection_id.to_string())
    .bind(saved.folder_id.map(|id| id.to_string()))
    .bind(&saved.name)
    .bind(request_json)
    .bind(saved.sort_order)
    .bind(&saved.created_at)
    .bind(&saved.updated_at)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn update_saved_request(
    pool: &SqlitePool,
    id: Uuid,
    name: &str,
    request: &Request,
    folder_id: Option<Uuid>,
    updated_at: &str,
) -> Result<(), StorageError> {
    let request_json = serde_json::to_string(request)
        .map_err(|err| StorageError::InvalidData(format!("request: {err}")))?;
    let affected = sqlx::query(
        "UPDATE saved_requests
         SET name = ?, request_json = ?, folder_id = ?, updated_at = ?
         WHERE id = ?",
    )
    .bind(name)
    .bind(request_json)
    .bind(folder_id.map(|id| id.to_string()))
    .bind(updated_at)
    .bind(id.to_string())
    .execute(pool)
    .await?
    .rows_affected();
    if affected == 0 {
        return Err(StorageError::NotFound {
            entity: "Saved request",
        });
    }
    Ok(())
}

pub async fn delete_saved_request(pool: &SqlitePool, id: Uuid) -> Result<(), StorageError> {
    let affected = sqlx::query("DELETE FROM saved_requests WHERE id = ?")
        .bind(id.to_string())
        .execute(pool)
        .await?
        .rows_affected();
    if affected == 0 {
        return Err(StorageError::NotFound {
            entity: "Saved request",
        });
    }
    Ok(())
}

/// Assign a new sort order to a saved request.
pub async fn set_request_order(
    pool: &SqlitePool,
    id: Uuid,
    sort_order: i64,
) -> Result<(), StorageError> {
    sqlx::query("UPDATE saved_requests SET sort_order = ? WHERE id = ?")
        .bind(sort_order)
        .bind(id.to_string())
        .execute(pool)
        .await?;
    Ok(())
}

/// Rewrite sort orders in one transaction, keeping the given order.
pub async fn reorder_requests(
    pool: &SqlitePool,
    ordered_ids: &[Uuid],
) -> Result<(), StorageError> {
    let mut tx = pool.begin().await?;
    for (index, id) in ordered_ids.iter().enumerate() {
        sqlx::query("UPDATE saved_requests SET sort_order = ? WHERE id = ?")
            .bind(index as i64)
            .bind(id.to_string())
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Next free sort order inside a folder (or the collection root).
pub async fn next_request_order(
    pool: &SqlitePool,
    collection_id: Uuid,
    folder_id: Option<Uuid>,
) -> Result<i64, StorageError> {
    let max: Option<i64> = match folder_id {
        Some(folder_id) => {
            sqlx::query_scalar(
                "SELECT MAX(sort_order) FROM saved_requests WHERE collection_id = ? AND folder_id = ?",
            )
            .bind(collection_id.to_string())
            .bind(folder_id.to_string())
            .fetch_one(pool)
            .await?
        }
        None => {
            sqlx::query_scalar(
                "SELECT MAX(sort_order) FROM saved_requests WHERE collection_id = ? AND folder_id IS NULL",
            )
            .bind(collection_id.to_string())
            .fetch_one(pool)
            .await?
        }
    };
    Ok(max.unwrap_or(-1) + 1)
}

/// Next free sort order for a child folder.
pub async fn next_folder_order(
    pool: &SqlitePool,
    collection_id: Uuid,
    parent_id: Option<Uuid>,
) -> Result<i64, StorageError> {
    let max: Option<i64> = match parent_id {
        Some(parent_id) => {
            sqlx::query_scalar(
                "SELECT MAX(sort_order) FROM folders WHERE collection_id = ? AND parent_id = ?",
            )
            .bind(collection_id.to_string())
            .bind(parent_id.to_string())
            .fetch_one(pool)
            .await?
        }
        None => {
            sqlx::query_scalar(
                "SELECT MAX(sort_order) FROM folders WHERE collection_id = ? AND parent_id IS NULL",
            )
            .bind(collection_id.to_string())
            .fetch_one(pool)
            .await?
        }
    };
    Ok(max.unwrap_or(-1) + 1)
}
