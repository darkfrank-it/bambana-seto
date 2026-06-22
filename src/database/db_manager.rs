use chrono::Utc;
use sqlx::{sqlite::SqlitePoolOptions, FromRow, SqlitePool, Error as SqlxError};
use std::path::Path;

#[derive(Debug, Clone, FromRow)]
pub struct StoredSession {
    pub id: i64,
    pub description: String,
    pub start_time: i64,
    pub end_time: Option<i64>,
}

pub fn ensure_sqlite_dir(database_url: &str) -> Result<(), std::io::Error> {
    let path_str = database_url
        .strip_prefix("sqlite://")
        .or_else(|| database_url.strip_prefix("sqlite:"))
        .unwrap_or(database_url);

    let db_path = Path::new(path_str);

    if let Some(parent) = db_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }

    Ok(())
}

pub async fn open_db(database_url: &str) -> sqlx::Result<SqlitePool> {
    ensure_sqlite_dir(database_url).map_err(SqlxError::Io)?;

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(database_url)
        .await?;

    // Tabella
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS sessions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            description TEXT NOT NULL,
            start_time INTEGER NOT NULL,
            end_time INTEGER
        )
        "#,
    )
    .execute(&pool)
    .await?;

    // Indice per range su start_time
    sqlx::query(
        r#"
        CREATE INDEX IF NOT EXISTS idx_sessions_start_time
        ON sessions(start_time)
        "#,
    )
    .execute(&pool)
    .await?;

    // Indice parziale per end_time IS NULL
    sqlx::query(
        r#"
        CREATE UNIQUE INDEX IF NOT EXISTS idx_one_open_session
        ON sessions(end_time)
        WHERE end_time IS NULL
        "#,
    )
    .execute(&pool)
    .await?;

    Ok(pool)
}

pub async fn load_recent_sessions(pool: &SqlitePool) -> sqlx::Result<Vec<StoredSession>> {
    let now = Utc::now().timestamp();

    let seven_days_ago = now - 7 * 24 * 60 * 60;

    let sessions = sqlx::query_as::<_, StoredSession>(
        r#"
        SELECT id, description, start_time, end_time
        FROM sessions
        WHERE start_time >= ?
        ORDER BY start_time DESC
        "#,
    )
    .bind(seven_days_ago)
    .fetch_all(pool)
    .await?;

    Ok(sessions)
}

// pub async fn get_open_session(pool: &SqlitePool) -> sqlx::Result<Option<StoredSession>> {
//     let session = sqlx::query_as::<_, StoredSession>(
//         r#"
//         SELECT id, description, start_time, end_time
//         FROM sessions
//         WHERE end_time IS NULL
//         LIMIT 1
//         "#
//     )
//     .fetch_optional(pool)
//     .await?;

//     Ok(session)
// }

pub async fn insert_session(
    pool: &SqlitePool,
    description: &str,
    start_time: i64,
) -> sqlx::Result<i64> {
    let result = sqlx::query(
        "INSERT INTO sessions (description, start_time) VALUES (?, ?)"
    )
    .bind(description)
    .bind(start_time)
    .execute(pool)
    .await?;

    Ok(result.last_insert_rowid())
}

pub async fn update_open_session(
    pool: &SqlitePool,
    id: i64,
    description: &str) -> sqlx::Result<bool> {
    let result = sqlx::query(
        r#"UPDATE sessions
           SET description = ?
           WHERE id = ?"#,
    )
    .bind(description)
    .bind(id)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

pub async fn end_open_session(
    pool: &SqlitePool,
    id: i64,
    end_time: i64,
) -> sqlx::Result<bool> {
    let result = sqlx::query(
        r#"UPDATE sessions
           SET end_time = ?
           WHERE id = ?"#,
    )
    .bind(end_time)
    .bind(id)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

pub async fn update_open_session_start_time(
    pool: &SqlitePool,
    id: i64,
    new_start_time: i64,
) -> sqlx::Result<bool> {
    let result = sqlx::query(
        r#"UPDATE sessions
           SET start_time = ?
           WHERE id = ?"#,
    )
    .bind(new_start_time)
    .bind(id)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

pub async fn delete_session(
    pool: &SqlitePool,
    session_id: i64,
) -> sqlx::Result<bool> {
    let result = sqlx::query(
        r#"DELETE FROM sessions WHERE id = ?"#,
    )
    .bind(session_id)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}
