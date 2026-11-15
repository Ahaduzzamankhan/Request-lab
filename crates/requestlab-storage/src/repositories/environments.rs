//! SQL for environments and their variables.

use requestlab_models::{Environment, EnvironmentVariable};
use sqlx::sqlite::SqliteRow;
use sqlx::SqlitePool;
use uuid::Uuid;

use super::{bool_at, text_at, uuid_at};
use crate::errors::StorageError;

const ENV_COLUMNS: &str = "id, name, created_at, updated_at";
const VAR_COLUMNS: &str = "id, environment_id, key, value, enabled, is_secret";

fn variable_from_row(row: &SqliteRow) -> Result<EnvironmentVariable, StorageError> {
    Ok(EnvironmentVariable {
        id: uuid_at(row, "id")?,
        key: text_at(row, "key")?,
        value: text_at(row, "value")?,
        enabled: bool_at(row, "enabled")?,
        is_secret: bool_at(row, "is_secret")?,
    })
}

/// All environments, each with its variables in display order.
pub async fn list(pool: &SqlitePool) -> Result<Vec<Environment>, StorageError> {
    let rows = sqlx::query(&format!(
        "SELECT {ENV_COLUMNS} FROM environments ORDER BY name COLLATE NOCASE"
    ))
    .fetch_all(pool)
    .await?;

    let mut environments = Vec::with_capacity(rows.len());
    for row in &rows {
        let id = uuid_at(row, "id")?;
        environments.push(Environment {
            id,
            name: text_at(row, "name")?,
            variables: variables_for(pool, id).await?,
            created_at: text_at(row, "created_at")?,
            updated_at: text_at(row, "updated_at")?,
        });
    }
    Ok(environments)
}

pub async fn get(pool: &SqlitePool, id: Uuid) -> Result<Environment, StorageError> {
    let row = sqlx::query(&format!(
        "SELECT {ENV_COLUMNS} FROM environments WHERE id = ?"
    ))
    .bind(id.to_string())
    .fetch_optional(pool)
    .await?
    .ok_or(StorageError::NotFound {
        entity: "Environment",
    })?;
    Ok(Environment {
        id,
        name: text_at(&row, "name")?,
        variables: variables_for(pool, id).await?,
        created_at: text_at(&row, "created_at")?,
        updated_at: text_at(&row, "updated_at")?,
    })
}

pub async fn variables_for(
    pool: &SqlitePool,
    environment_id: Uuid,
) -> Result<Vec<EnvironmentVariable>, StorageError> {
    let rows = sqlx::query(&format!(
        "SELECT {VAR_COLUMNS} FROM environment_variables WHERE environment_id = ? ORDER BY position, rowid"
    ))
    .bind(environment_id.to_string())
    .fetch_all(pool)
    .await?;
    rows.iter().map(variable_from_row).collect()
}

/// Insert an environment with all of its variables in one transaction.
pub async fn insert(pool: &SqlitePool, environment: &Environment) -> Result<(), StorageError> {
    let mut tx = pool.begin().await?;
    sqlx::query("INSERT INTO environments (id, name, created_at, updated_at) VALUES (?, ?, ?, ?)")
        .bind(environment.id.to_string())
        .bind(&environment.name)
        .bind(&environment.created_at)
        .bind(&environment.updated_at)
        .execute(&mut *tx)
        .await?;
    insert_variables(&mut tx, environment.id, &environment.variables).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn rename(
    pool: &SqlitePool,
    id: Uuid,
    name: &str,
    updated_at: &str,
) -> Result<(), StorageError> {
    let affected = sqlx::query("UPDATE environments SET name = ?, updated_at = ? WHERE id = ?")
        .bind(name)
        .bind(updated_at)
        .bind(id.to_string())
        .execute(pool)
        .await?
        .rows_affected();
    if affected == 0 {
        return Err(StorageError::NotFound {
            entity: "Environment",
        });
    }
    Ok(())
}

pub async fn delete(pool: &SqlitePool, id: Uuid) -> Result<(), StorageError> {
    let affected = sqlx::query("DELETE FROM environments WHERE id = ?")
        .bind(id.to_string())
        .execute(pool)
        .await?
        .rows_affected();
    if affected == 0 {
        return Err(StorageError::NotFound {
            entity: "Environment",
        });
    }
    Ok(())
}

/// Replace the full variable list of an environment, atomically.
pub async fn replace_variables(
    pool: &SqlitePool,
    environment_id: Uuid,
    variables: &[EnvironmentVariable],
    updated_at: &str,
) -> Result<(), StorageError> {
    let mut tx = pool.begin().await?;
    let exists: Option<String> = sqlx::query_scalar("SELECT id FROM environments WHERE id = ?")
        .bind(environment_id.to_string())
        .fetch_optional(&mut *tx)
        .await?;
    if exists.is_none() {
        return Err(StorageError::NotFound {
            entity: "Environment",
        });
    }
    sqlx::query("DELETE FROM environment_variables WHERE environment_id = ?")
        .bind(environment_id.to_string())
        .execute(&mut *tx)
        .await?;
    insert_variables(&mut tx, environment_id, variables).await?;
    sqlx::query("UPDATE environments SET updated_at = ? WHERE id = ?")
        .bind(updated_at)
        .bind(environment_id.to_string())
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

async fn insert_variables(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    environment_id: Uuid,
    variables: &[EnvironmentVariable],
) -> Result<(), StorageError> {
    for (index, variable) in variables.iter().enumerate() {
        sqlx::query(
            "INSERT INTO environment_variables
                (id, environment_id, key, value, enabled, is_secret, position)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(variable.id.to_string())
        .bind(environment_id.to_string())
        .bind(&variable.key)
        .bind(&variable.value)
        .bind(i64::from(variable.enabled))
        .bind(i64::from(variable.is_secret))
        .bind(index as i64)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}
