//! SQLite storage for Leon's own data (P5): projects now, flows and
//! their events later.
//!
//! Owns the schema and its migrations. Implements the core's storage ports.
//!
//! Must not: depend on another adapter or on the UI; parse a CLI's own
//! transcript files.

mod migrations;
mod storage;

use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use rusqlite::Connection;

/// Something went wrong opening or migrating the database.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// SQLite reported an error.
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// The file was written by a newer Leon.
    #[error("the database has schema version {found}; this Leon knows up to {known}. Update Leon.")]
    TooNew {
        /// Version stored in the file.
        found: u32,
        /// Latest version this build knows.
        known: u32,
    },
}

/// Leon's database. Cheap to share behind an `Arc`; calls block, so the core
/// runs them off its async workers.
#[derive(Debug)]
pub struct Store {
    conn: Mutex<Connection>,
}

impl Store {
    /// Opens (or creates) the database file and brings its schema up to date.
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        Self::init(conn)
    }

    /// An empty database in memory, for tests and previews.
    pub fn open_in_memory() -> Result<Self, StoreError> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(mut conn: Connection) -> Result<Self, StoreError> {
        conn.pragma_update(None, "foreign_keys", true)?;
        migrations::run(&mut conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// The connection. A panic while holding it cannot leave SQLite in a bad
    /// state (each write is one statement or one transaction), so a poisoned
    /// lock is recovered.
    fn lock(&self) -> MutexGuard<'_, Connection> {
        self.conn
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}
