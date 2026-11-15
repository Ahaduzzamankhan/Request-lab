//! SQL for request history.

use requestlab_models::{HistoryEntry, HistoryQuery, HistoryStatusFilter, HttpMethod, Request};
use sqlx::sqlite::SqliteRow;
use sqlx::SqlitePool;
use sqlx::Row;
use uuid::Uuid;

use super::{opt_text_at, text_at, uuid_at};
use crate::errors::StorageError;

const COLUMNS: &str =
    "id, method, url, request_json, status_code, duration_ms, error_message, created_at";

fn history_from_row(row: &SqliteRow) -> Result<HistoryEntry, StorageError> {
    let method_raw = text_at(row, "method")?;
    let request_json = text_at(row, "request_json")?;
    let request: Request = serde_json::from_str(&request_json)
        .map_err(|err| StorageError::InvalidData(format!("request_json: {err}")))?;
    Ok(HistoryEntry {
        id: uuid_at(row, "id")?,
        method: HttpMethod::parse(&method_raw)
            .ok_or_else(|| StorageError::InvalidData(format!("unknown method {method_raw}")))?,
        url: text_at(row, "url")?,
        request,
        status_code: row.try_get::<Option<i64>, _>("status_code")?.map(|code| code as u16),
        duration_ms: row
            .try_get::<Option<i64>, _>("duration_ms")?
            .map(|ms| ms.max(0) as u64),
        error_message: opt_text_at(row, "error_message")?,
        created_at: text_at(row, "created_at")?,
    })
}

fn status_key(filter: HistoryStatusFilter) -> &'static str {
    match filter {
        HistoryStatusFilter::All => "all",
        HistoryStatusFilter::Success => "success",
        HistoryStatusFilter::Redirect => "redirect",
        HistoryStatusFilter::ClientError => "client",
        HistoryStatusFilter::ServerError => "server",
        HistoryStatusFilter::Failed => "failed",
    }
}

pub async fn insert(pool: &SqlitePool, entry: &HistoryEntry) -> Result<(), StorageError> {
    let request_json = serde_json::to_string(&entry.request)
        .map_err(|err| StorageError::InvalidData(format!("request: {err}")))?;
    sqlx::query(
        "INSERT INTO request_history
            (id, method, url, request_json, status_code, duration_ms, error_message, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(entry.id.to_string())
    .bind(entry.method.as_str())
    .bind(&entry.url)
    .bind(request_json)
    .bind(entry.status_code.map(i64::from))
    .bind(entry.duration_ms.map(|ms| ms as i64))
    .bind(&entry.error_message)
    .bind(&entry.created_at)
    .execute(pool)
    .await?;
    Ok(())
}

/// Query history with search, method and status filtering. Always bounded.
pub async fn list(
    pool: &SqlitePool,
    query: &HistoryQuery,
) -> Result<Vec<HistoryEntry>, StorageError> {
    let search = query.search.trim().to_ascii_lowercase();
    let method = query.method.map(|method| method.as_str().to_string());
    let status = status_key(query.status);
    let rows = sqlx::query(&format!(
        "SELECT {COLUMNS} FROM request_history
         WHERE (?1 = ''
                OR instr(lower(url), ?1) > 0
                OR instr(lower(method), ?1) > 0
                OR instr(lower(coalesce(error_message, '')), ?1) > 0)
           AND (?2 IS NULL OR method = ?2)
           AND (?3 = 'all'
                OR (?3 = 'failed' AND status_code IS NULL)
                OR (?3 = 'success' AND status_code BETWEEN 200 AND 299)
                OR (?3 = 'redirect' AND status_code BETWEEN 300 AND 399)
                OR (?3 = 'client' AND status_code BETWEEN 400 AND 499)
                OR (?3 = 'server' AND status_code BETWEEN 500 AND 599))
         ORDER BY created_at DESC, rowid DESC
         LIMIT ?4 OFFSET ?5"
    ))
    .bind(search)
    .bind(method)
    .bind(status)
    .bind(query.limit.clamp(1, 1000))
    .bind(query.offset.max(0))
    .fetch_all(pool)
    .await?;
    rows.iter().map(history_from_row).collect()
}

pub async fn get(pool: &SqlitePool, id: Uuid) -> Result<HistoryEntry, StorageError> {
    let row = sqlx::query(&format!(
        "SELECT {COLUMNS} FROM request_history WHERE id = ?"
    ))
    .bind(id.to_string())
    .fetch_optional(pool)
    .await?
    .ok_or(StorageError::NotFound {
        entity: "History entry",
    })?;
    history_from_row(&row)
}

pub async fn delete(pool: &SqlitePool, id: Uuid) -> Result<(), StorageError> {
    let affected = sqlx::query("DELETE FROM request_history WHERE id = ?")
        .bind(id.to_string())
        .execute(pool)
        .await?
        .rows_affected();
    if affected == 0 {
        return Err(StorageError::NotFound {
            entity: "History entry",
        });
    }
    Ok(())
}

pub async fn clear(pool: &SqlitePool) -> Result<u64, StorageError> {
    let affected = sqlx::query("DELETE FROM request_history")
        .execute(pool)
        .await?
        .rows_affected();
    Ok(affected)
}

pub async fn count(pool: &SqlitePool) -> Result<i64, StorageError> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM request_history")
        .fetch_one(pool)
        .await?;
    Ok(count)
}

/// Keep only the newest `keep` rows, enforcing bounded growth.
pub async fn prune(pool: &SqlitePool, keep: i64) -> Result<u64, StorageError> {
    let keep = keep.max(0);
    let affected = sqlx::query(
        "DELETE FROM request_history
         WHERE id NOT IN (
             SELECT id FROM request_history ORDER BY created_at DESC, rowid DESC LIMIT ?
         )",
    )
    .bind(keep)
    .execute(pool)
    .await?
    .rows_affected();
    Ok(affected)
}
