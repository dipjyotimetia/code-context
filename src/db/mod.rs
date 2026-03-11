pub mod queries;
pub mod schema;

use std::path::Path;
use std::time::Duration;

use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::Connection;

/// Thread-safe SQLite connection pool.
///
/// Replaces the previous single `Mutex<Connection>`. SQLite in WAL mode supports
/// multiple concurrent readers, so a pool of 8 connections allows read-heavy
/// workloads (search, navigation, context tools) to execute in parallel without
/// serializing through a single mutex.
pub struct Database {
    pool: Pool<SqliteConnectionManager>,
}

impl Database {
    pub fn init(db_path: &Path) -> anyhow::Result<Self> {
        let manager =
            SqliteConnectionManager::file(db_path).with_init(schema::configure_connection);

        // In-memory databases (':memory:') create a separate, empty database for
        // each connection, so pool size must be 1 to ensure all callers share
        // the same in-memory schema.  This path is only hit in tests.
        let is_memory = db_path.to_str() == Some(":memory:");
        let pool = if is_memory {
            Pool::builder()
                .max_size(1)
                .connection_timeout(Duration::from_secs(30))
                .build(manager)?
        } else {
            Pool::builder()
                .max_size(8)
                .min_idle(Some(2))
                .connection_timeout(Duration::from_secs(30))
                .build(manager)?
        };

        // Run DDL migrations once with a dedicated connection.
        {
            let conn = pool.get()?;
            schema::init_schema(&conn)?;
        }

        Ok(Self { pool })
    }

    /// Run a read-only closure against a pooled connection.
    pub fn with_conn<F, T>(&self, f: F) -> anyhow::Result<T>
    where
        F: FnOnce(&Connection) -> anyhow::Result<T>,
    {
        let conn = self.pool.get()?;
        f(&conn)
    }

    /// Execute a closure inside a DEFERRED transaction.
    /// Automatically commits on success, rolls back on error.
    pub fn with_tx<F, T>(&self, f: F) -> anyhow::Result<T>
    where
        F: FnOnce(&Connection) -> anyhow::Result<T>,
    {
        let conn = self.pool.get()?;
        let tx = conn.unchecked_transaction()?;
        let result = f(&tx)?;
        tx.commit()?;
        Ok(result)
    }
}
