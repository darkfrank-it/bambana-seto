use chrono::Utc;
use sqlx::{Error as SqlxError, FromRow, SqlitePool, sqlite::SqlitePoolOptions};
use std::fs::OpenOptions;
use std::path::Path;

#[derive(Debug, Clone, FromRow)]
pub struct StoredSession {
    pub id: i64,
    pub description: String,
    pub start_time: i64,
    pub end_time: Option<i64>,
}

pub fn ensure_sqlite_dir_and_db(database_url: &str) -> Result<(), std::io::Error> {
    let path_str = database_url
        .strip_prefix("sqlite://")
        .or_else(|| database_url.strip_prefix("sqlite:"))
        .unwrap_or(database_url);

    let db_path = Path::new(path_str);

    if let Some(parent) = db_path.parent()
        && !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }

    // Aprire una connessione forza la creazione del DB
    if !db_path.exists() {
        OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(db_path)?;
    }

    Ok(())
}

pub async fn open_db(database_url: &str) -> sqlx::Result<SqlitePool> {
    ensure_sqlite_dir_and_db(database_url).map_err(SqlxError::Io)?;

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

    // Indice parziale per garantire una sola sessione aperta alla volta.
    // NULL non è mai "uguale" a un altro NULL in un indice unico SQLite,
    // quindi un indice diretto su end_time non applicherebbe alcun vincolo:
    // indicizziamo invece un'espressione costante, così tutte le righe con
    // end_time NULL condividono lo stesso valore indicizzato.
    sqlx::query(r#"DROP INDEX IF EXISTS idx_one_open_session"#)
        .execute(&pool)
        .await?;

    if let Err(err) = sqlx::query(
        r#"
        CREATE UNIQUE INDEX idx_one_open_session
        ON sessions((1))
        WHERE end_time IS NULL
        "#,
    )
    .execute(&pool)
    .await
    {
        // Se nel DB esistono già più sessioni aperte contemporaneamente
        // (a causa del vincolo precedente, mai realmente applicato), non
        // blocchiamo l'avvio dell'app: il vincolo resterà semplicemente
        // non applicato finché i dati non vengono ripuliti manualmente.
        log::warn!("Failed to (re)create idx_one_open_session index: {err}");
    }

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

pub async fn insert_session(
    pool: &SqlitePool,
    description: &str,
    start_time: i64,
) -> sqlx::Result<i64> {
    let result = sqlx::query("INSERT INTO sessions (description, start_time) VALUES (?, ?)")
        .bind(description)
        .bind(start_time)
        .execute(pool)
        .await?;

    Ok(result.last_insert_rowid())
}

pub async fn update_open_session(
    pool: &SqlitePool,
    id: i64,
    description: &str,
) -> sqlx::Result<bool> {
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

pub async fn end_open_session(pool: &SqlitePool, id: i64, end_time: i64) -> sqlx::Result<bool> {
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

pub async fn delete_session(pool: &SqlitePool, session_id: i64) -> sqlx::Result<bool> {
    let result = sqlx::query(r#"DELETE FROM sessions WHERE id = ?"#)
        .bind(session_id)
        .execute(pool)
        .await?;

    Ok(result.rows_affected() > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_DB_COUNTER: AtomicU64 = AtomicU64::new(0);

    // Wraps a pool backed by a temp file and removes the file once the test
    // is done, so repeated test runs don't leave junk in the temp dir.
    struct TestDb {
        pool: SqlitePool,
        path: PathBuf,
    }

    impl std::ops::Deref for TestDb {
        type Target = SqlitePool;
        fn deref(&self) -> &SqlitePool {
            &self.pool
        }
    }

    impl TestDb {
        // SQLite on Windows can't delete a file while a connection still has
        // it open, so the pool must be closed before removing the file.
        async fn close(self) {
            self.pool.close().await;
            let _ = std::fs::remove_file(&self.path);
        }
    }

    async fn test_pool() -> TestDb {
        let id = TEST_DB_COUNTER.fetch_add(1, Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!(
            "bambana_seto_dbmgr_test_{}_{}.db",
            std::process::id(),
            id
        ));
        let url = format!("sqlite:{}", path.display());
        let pool = open_db(&url).await.expect("open test db");
        TestDb { pool, path }
    }

    #[tokio::test]
    async fn insert_and_load_recent_session() {
        let pool = test_pool().await;
        let now = Utc::now().timestamp();

        let id = insert_session(&pool, "task a", now).await.unwrap();
        assert!(id > 0);

        let sessions = load_recent_sessions(&pool).await.unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, id);
        assert_eq!(sessions[0].description, "task a");
        assert_eq!(sessions[0].start_time, now);
        assert_eq!(sessions[0].end_time, None);
        pool.close().await;
    }

    #[tokio::test]
    async fn load_recent_sessions_excludes_sessions_older_than_7_days() {
        let pool = test_pool().await;
        let now = Utc::now().timestamp();
        let eight_days_ago = now - 8 * 24 * 60 * 60;

        let old_id = insert_session(&pool, "old task", eight_days_ago)
            .await
            .unwrap();
        end_open_session(&pool, old_id, eight_days_ago + 60)
            .await
            .unwrap();
        insert_session(&pool, "recent task", now).await.unwrap();

        let sessions = load_recent_sessions(&pool).await.unwrap();

        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].description, "recent task");
        pool.close().await;
    }

    #[tokio::test]
    async fn only_one_open_session_is_allowed_at_a_time() {
        let pool = test_pool().await;
        let now = Utc::now().timestamp();

        let first_id = insert_session(&pool, "task a", now).await.unwrap();

        // A second open session (end_time still NULL) must violate the
        // partial unique index and be rejected.
        let second = insert_session(&pool, "task b", now + 10).await;
        assert!(second.is_err());

        // Closing the first session frees up the slot for a new one.
        end_open_session(&pool, first_id, now + 60).await.unwrap();
        let third = insert_session(&pool, "task c", now + 120).await;
        assert!(third.is_ok());
        pool.close().await;
    }

    #[tokio::test]
    async fn end_open_session_returns_false_for_unknown_id() {
        let pool = test_pool().await;
        let updated = end_open_session(&pool, 999, Utc::now().timestamp())
            .await
            .unwrap();
        assert!(!updated);
        pool.close().await;
    }

    #[tokio::test]
    async fn update_open_session_returns_false_for_unknown_id() {
        let pool = test_pool().await;
        let updated = update_open_session(&pool, 999, "new description")
            .await
            .unwrap();
        assert!(!updated);
        pool.close().await;
    }

    #[tokio::test]
    async fn update_open_session_start_time_returns_false_for_unknown_id() {
        let pool = test_pool().await;
        let updated = update_open_session_start_time(&pool, 999, Utc::now().timestamp())
            .await
            .unwrap();
        assert!(!updated);
        pool.close().await;
    }

    #[tokio::test]
    async fn delete_session_removes_existing_session() {
        let pool = test_pool().await;
        let now = Utc::now().timestamp();
        let id = insert_session(&pool, "task a", now).await.unwrap();

        let deleted = delete_session(&pool, id).await.unwrap();
        assert!(deleted);

        let sessions = load_recent_sessions(&pool).await.unwrap();
        assert!(sessions.is_empty());
        pool.close().await;
    }

    #[tokio::test]
    async fn delete_session_returns_false_for_unknown_id() {
        let pool = test_pool().await;
        let deleted = delete_session(&pool, 999).await.unwrap();
        assert!(!deleted);
        pool.close().await;
    }
}
