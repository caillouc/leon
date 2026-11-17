//! Schema migrations, tracked with SQLite's `user_version`.
//!
//! Append only: never edit a migration that has shipped; add a new one.

use rusqlite::Connection;

use crate::StoreError;

/// Migration `n` (1-based) brings the schema from version `n - 1` to `n`.
const MIGRATIONS: &[&str] = &[
    // 1: projects (NAV-2).
    "CREATE TABLE projects (
        id INTEGER PRIMARY KEY,
        name TEXT NOT NULL,
        path TEXT NOT NULL UNIQUE,
        added_at INTEGER NOT NULL
    ) STRICT;",
    // 2: conversations, agent sessions and their transcripts (P5).
    "CREATE TABLE conversations (
        id INTEGER PRIMARY KEY,
        project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
        title TEXT NOT NULL,
        pinned INTEGER NOT NULL DEFAULT 0,
        archived INTEGER NOT NULL DEFAULT 0,
        unread INTEGER NOT NULL DEFAULT 0,
        updated_at INTEGER NOT NULL
    ) STRICT;
    CREATE TABLE agent_sessions (
        id INTEGER PRIMARY KEY,
        conversation_id INTEGER NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
        started_by INTEGER REFERENCES agent_sessions(id) ON DELETE SET NULL,
        backend TEXT NOT NULL,
        agent TEXT,
        model TEXT,
        effort TEXT,
        permission TEXT NOT NULL,
        working_dir TEXT NOT NULL,
        cli_session TEXT
    ) STRICT;
    CREATE TABLE entries (
        session_id INTEGER NOT NULL REFERENCES agent_sessions(id) ON DELETE CASCADE,
        seq INTEGER NOT NULL,
        at INTEGER NOT NULL,
        entry TEXT NOT NULL,
        PRIMARY KEY (session_id, seq)
    ) STRICT;",
    // 3: the folder a conversation moved to (D3, P10).
    "ALTER TABLE conversations ADD COLUMN working_dir TEXT;",
    // 4: projects folded in the left pane (NAV-2).
    "ALTER TABLE projects ADD COLUMN collapsed INTEGER NOT NULL DEFAULT 0;",
];

/// Latest schema version this build knows.
pub(crate) fn latest() -> u32 {
    u32::try_from(MIGRATIONS.len()).unwrap_or(u32::MAX)
}

pub(crate) fn version(conn: &Connection) -> Result<u32, StoreError> {
    Ok(conn.pragma_query_value(None, "user_version", |row| row.get(0))?)
}

/// Applies every missing migration, each in its own transaction.
pub(crate) fn run(conn: &mut Connection) -> Result<(), StoreError> {
    let found = version(conn)?;
    let known = latest();
    if found > known {
        return Err(StoreError::TooNew { found, known });
    }
    for (version, sql) in (1u32..)
        .zip(MIGRATIONS)
        .filter(|(version, _)| *version > found)
    {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", version)?;
        tx.commit()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    use super::{latest, run, version};
    use crate::StoreError;

    #[test]
    fn fresh_database_reaches_latest() {
        let mut conn = Connection::open_in_memory().unwrap();
        run(&mut conn).unwrap();
        assert_eq!(version(&conn).unwrap(), latest());
    }

    #[test]
    fn running_twice_is_harmless() {
        let mut conn = Connection::open_in_memory().unwrap();
        run(&mut conn).unwrap();
        run(&mut conn).unwrap();
        assert_eq!(version(&conn).unwrap(), latest());
    }

    #[test]
    fn newer_database_is_refused() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "user_version", latest() + 1)
            .unwrap();
        assert!(matches!(run(&mut conn), Err(StoreError::TooNew { .. })));
    }
}
