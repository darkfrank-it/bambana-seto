use sqlx::{sqlite::SqlitePoolOptions, FromRow, SqlitePool, Error as SqlxError};
use std::fs::OpenOptions;
use std::path::Path;

#[derive(Debug, Clone, FromRow)]
pub struct StoredSession {
    pub id: i64,
    pub description: String,
    pub start_time: String,
    pub end_time: Option<String>,
}

fn ensure_sqlite_file(database_url: &str) -> Result<(), std::io::Error> {
    let path_str = database_url
        .strip_prefix("sqlite:")
        .map(|path| path.strip_prefix("//").unwrap_or(path))
        .unwrap_or(database_url);

    if path_str == ":memory:" || path_str.starts_with("file:memory") {
        return Ok(());
    }

    let db_path = Path::new(path_str);
    if let Some(parent) = db_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }

    if !db_path.exists() {
        OpenOptions::new().create(true).write(true).open(db_path)?;
    }

    Ok(())
}

pub async fn open_db(database_url: &str) -> sqlx::Result<SqlitePool> {
    ensure_sqlite_file(database_url).map_err(SqlxError::Io)?;

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(database_url)
        .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS sessions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            description TEXT NOT NULL,
            start_time TEXT NOT NULL,
            end_time TEXT
        )
        "#,
    )
    .execute(&pool)
    .await?;

    Ok(pool)
}

pub async fn insert_session(
    pool: &SqlitePool,
    description: &str,
    start_time: &str,
) -> sqlx::Result<i64> {
    let result = sqlx::query("INSERT INTO sessions (description, start_time) VALUES (?, ?)")
        .bind(description)
        .bind(start_time)
        .execute(pool)
        .await?;

    Ok(result.last_insert_rowid())
}

pub async fn update_last_open_session_end(
    pool: &SqlitePool,
    end_time: &str,
) -> sqlx::Result<bool> {
    let result = sqlx::query(
        r#"UPDATE sessions
           SET end_time = ?
           WHERE id = (
               SELECT id FROM sessions
               WHERE end_time IS NULL
               ORDER BY start_time DESC
               LIMIT 1
           )"#,
    )
    .bind(end_time)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

pub async fn update_last_open_session_description_and_end(
    pool: &SqlitePool,
    description: &str,
    end_time: &str,
) -> sqlx::Result<bool> {
    let result = sqlx::query(
        r#"UPDATE sessions
           SET description = ?, end_time = ?
           WHERE id = (
               SELECT id FROM sessions
               WHERE end_time IS NULL
               ORDER BY start_time DESC
               LIMIT 1
           )"#,
    )
    .bind(description)
    .bind(end_time)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

pub async fn load_recent_sessions(pool: &SqlitePool) -> sqlx::Result<Vec<StoredSession>> {
    let sessions = sqlx::query_as::<_, StoredSession>(
        r#"SELECT id, description, start_time, end_time
           FROM sessions
           WHERE start_time >= datetime('now','-7 days')
           ORDER BY start_time DESC"#,
    )
    .fetch_all(pool)
    .await?;

    Ok(sessions)
}

pub async fn get_open_session(pool: &SqlitePool) -> sqlx::Result<Option<StoredSession>> {
    let session = sqlx::query_as::<_, StoredSession>(
        r#"SELECT id, description, start_time, end_time
           FROM sessions
           WHERE end_time IS NULL
           ORDER BY start_time DESC
           LIMIT 1"#,
    )
    .fetch_optional(pool)
    .await?;

    Ok(session)
}

pub async fn update_session_end_by_id(
    pool: &SqlitePool,
    session_id: i64,
    end_time: &str,
) -> sqlx::Result<bool> {
    let result = sqlx::query(
        r#"UPDATE sessions
           SET end_time = ?
           WHERE id = ?"#,
    )
    .bind(end_time)
    .bind(session_id)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

pub async fn delete_session_by_id(
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

pub async fn update_open_session_start_time(
    pool: &SqlitePool,
    new_start_time: &str,
) -> sqlx::Result<bool> {
    let result = sqlx::query(
        r#"UPDATE sessions
           SET start_time = ?
           WHERE id = (
               SELECT id FROM sessions
               WHERE end_time IS NULL
               ORDER BY start_time DESC
               LIMIT 1
           )"#,
    )
    .bind(new_start_time)
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}
