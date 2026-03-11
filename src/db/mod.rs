pub mod queries;
pub mod schema;

use std::path::Path;
use std::sync::Mutex;

use rusqlite::Connection;

pub struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    pub fn init(db_path: &Path) -> anyhow::Result<Self> {
        let conn = Connection::open(db_path)?;
        schema::init_schema(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn with_conn<F, T>(&self, f: F) -> anyhow::Result<T>
    where
        F: FnOnce(&Connection) -> anyhow::Result<T>,
    {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("db lock poisoned: {e}"))?;
        f(&conn)
    }

    /// Execute a closure inside a DEFERRED transaction.
    /// Automatically commits on success, rolls back on error.
    pub fn with_tx<F, T>(&self, f: F) -> anyhow::Result<T>
    where
        F: FnOnce(&Connection) -> anyhow::Result<T>,
    {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("db lock poisoned: {e}"))?;
        let tx = conn.unchecked_transaction()?;
        let result = f(&tx)?;
        tx.commit()?;
        Ok(result)
    }
}
