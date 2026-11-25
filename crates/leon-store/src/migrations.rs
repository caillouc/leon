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
    // 2: flows, conversations and their transcripts (P5).
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
    // 3: the folder a flow moved to (D3, P10).
    "ALTER TABLE conversations ADD COLUMN working_dir TEXT;",
    // 4: projects folded in the left pane (NAV-2).
    "ALTER TABLE projects ADD COLUMN collapsed INTEGER NOT NULL DEFAULT 0;",
    // 5: the issue a flow was started for, `owner/name#12` (GH-2).
    "ALTER TABLE conversations ADD COLUMN issue TEXT;",
    // 6: groups folded in the GitHub lists: board columns, repositories (GH-2,
    // GH-3).
    "CREATE TABLE folded_groups (key TEXT PRIMARY KEY) STRICT;",
    // 7: every folder a flow works in, a JSON array of paths, instead
    // of one (CTX-2).
    "ALTER TABLE conversations ADD COLUMN folders TEXT NOT NULL DEFAULT '[]';
    UPDATE conversations SET folders = json_array(working_dir) WHERE working_dir IS NOT NULL;
    ALTER TABLE conversations DROP COLUMN working_dir;",
    // 8: which conversation's answer is not read yet (NAV-4, FLOW-4).
    "ALTER TABLE agent_sessions ADD COLUMN unread INTEGER NOT NULL DEFAULT 0;",
    // 9: the tables take the app's words (D8): a left-pane entry is a flow,
    // an agent's own conversation in it a conversation. Renaming keeps the
    // rows and points the foreign keys at the new names.
    "ALTER TABLE conversations RENAME TO flows;
    ALTER TABLE agent_sessions RENAME COLUMN conversation_id TO flow_id;
    ALTER TABLE agent_sessions RENAME TO conversations;
    ALTER TABLE entries RENAME COLUMN session_id TO conversation_id;",
    // 10: every issue and pull request linked to a flow, in the order they
    // were linked, instead of only the issue it was started for (CTX-2).
    "CREATE TABLE flow_links (
        flow_id INTEGER NOT NULL REFERENCES flows(id) ON DELETE CASCADE,
        kind TEXT NOT NULL,
        repo TEXT NOT NULL,
        number INTEGER NOT NULL,
        PRIMARY KEY (flow_id, kind, repo, number)
    ) STRICT;
    INSERT INTO flow_links (flow_id, kind, repo, number)
      SELECT id, 'issue', substr(issue, 1, instr(issue, '#') - 1),
             CAST(substr(issue, instr(issue, '#') + 1) AS INTEGER)
      FROM flows WHERE instr(issue, '#') > 1 ORDER BY id;
    ALTER TABLE flows DROP COLUMN issue;",
    // 11: the folder each conversation works in, so the right pane shows
    // only the worktrees an agent works in, across restarts (CTX-2, I20).
    "ALTER TABLE conversations ADD COLUMN folder TEXT;",
    // 12: each project's place in the left pane, which the user changes by
    // dragging (NAV-2, I25); first the order they showed in, by name.
    "ALTER TABLE projects ADD COLUMN position INTEGER NOT NULL DEFAULT 0;
    UPDATE projects SET position = (
      SELECT COUNT(*) FROM projects AS other
      WHERE other.name < projects.name COLLATE NOCASE
         OR (other.name = projects.name COLLATE NOCASE AND other.id < projects.id)
    );",
    // 13: a flow's to-dos, added by the user or an agent (TODO-1).
    "CREATE TABLE todos (
        id INTEGER PRIMARY KEY,
        flow_id INTEGER NOT NULL REFERENCES flows(id) ON DELETE CASCADE,
        text TEXT NOT NULL,
        done INTEGER NOT NULL DEFAULT 0,
        added_by INTEGER REFERENCES conversations(id) ON DELETE SET NULL,
        added_at INTEGER NOT NULL
    ) STRICT;",
    // 14: what a to-do waits for, its "when" (its "what" is `text`) (TODO-1).
    "ALTER TABLE todos ADD COLUMN waits_for TEXT NOT NULL DEFAULT '';",
    // 15: each flow's group's place on the To do page, which the user
    // changes by dragging (TODO-3); first the order it showed in, the most
    // recent first.
    "ALTER TABLE flows ADD COLUMN todo_position INTEGER NOT NULL DEFAULT 0;
    UPDATE flows SET todo_position = (
      SELECT COUNT(*) FROM flows AS other
      WHERE other.updated_at > flows.updated_at
         OR (other.updated_at = flows.updated_at AND other.id > flows.id)
    );",
    // 16: when each flow was archived (NAV-8), so Leon deletes it a month
    // later; one archived before is counted from now, never at once.
    "ALTER TABLE flows ADD COLUMN archived_at INTEGER;
    UPDATE flows SET archived_at = CAST(strftime('%s', 'now') AS INTEGER) * 1000
    WHERE archived = 1;",
    // 17: what each CLI last said of its plan (DOCTOR-2, USE-1), shown
    // after a restart until it says again: its windows as JSON, its
    // credits, when.
    "CREATE TABLE plan_usage (
        backend TEXT PRIMARY KEY,
        windows TEXT NOT NULL,
        credits TEXT,
        at INTEGER NOT NULL
    ) STRICT;",
    // 18: the conversations whose tab the user closed (FLOW-3).
    "ALTER TABLE conversations ADD COLUMN tab_closed INTEGER NOT NULL DEFAULT 0;",
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

    use super::{MIGRATIONS, latest, run, version};
    use crate::StoreError;

    /// Renaming the tables keeps every row and the cascades: deleting a
    /// flow still deletes its conversations and their entries.
    #[test]
    fn tables_take_the_new_names_and_keep_their_rows() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", true).unwrap();
        for sql in &MIGRATIONS[..8] {
            conn.execute_batch(sql).unwrap();
        }
        conn.pragma_update(None, "user_version", 8).unwrap();
        conn.execute_batch(
            "INSERT INTO projects (id, name, path, added_at) VALUES (1, 'api', '/code/api', 0);
             INSERT INTO conversations (id, project_id, title, updated_at) VALUES (1, 1, 'Fix', 0);
             INSERT INTO agent_sessions (id, conversation_id, backend, permission, working_dir)
               VALUES (1, 1, 'codex', 'auto', '/code/api');
             INSERT INTO entries (session_id, seq, at, entry) VALUES (1, 1, 0, '{}');",
        )
        .unwrap();
        run(&mut conn).unwrap();
        let count = |sql: &str| -> i64 { conn.query_row(sql, [], |row| row.get(0)).unwrap() };
        assert_eq!(count("SELECT COUNT(*) FROM flows WHERE title = 'Fix'"), 1);
        assert_eq!(
            count("SELECT COUNT(*) FROM conversations WHERE flow_id = 1"),
            1
        );
        assert_eq!(
            count("SELECT COUNT(*) FROM entries WHERE conversation_id = 1"),
            1
        );
        conn.execute("DELETE FROM flows WHERE id = 1", []).unwrap();
        assert_eq!(count("SELECT COUNT(*) FROM conversations"), 0);
        assert_eq!(count("SELECT COUNT(*) FROM entries"), 0);
    }

    #[test]
    fn a_flows_folder_becomes_its_first_folder() {
        let mut conn = Connection::open_in_memory().unwrap();
        for sql in &MIGRATIONS[..6] {
            conn.execute_batch(sql).unwrap();
        }
        conn.pragma_update(None, "user_version", 6).unwrap();
        conn.execute_batch(
            "INSERT INTO projects (id, name, path, added_at) VALUES (1, 'api', '/code/api', 0);
             INSERT INTO conversations (id, project_id, title, updated_at, working_dir)
               VALUES (1, 1, 'moved', 0, '/code/api/.worktrees/fix'), (2, 1, 'stayed', 0, NULL);",
        )
        .unwrap();
        run(&mut conn).unwrap();
        let folders: Vec<String> = conn
            .prepare("SELECT folders FROM flows ORDER BY id")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(folders, [r#"["/code/api/.worktrees/fix"]"#, "[]"]);
    }

    /// The issue a flow was started for becomes its first link; the flow
    /// and its links go together.
    #[test]
    fn a_flows_issue_becomes_its_first_link() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", true).unwrap();
        for sql in &MIGRATIONS[..9] {
            conn.execute_batch(sql).unwrap();
        }
        conn.pragma_update(None, "user_version", 9).unwrap();
        conn.execute_batch(
            "INSERT INTO projects (id, name, path, added_at) VALUES (1, 'api', '/code/api', 0);
             INSERT INTO flows (id, project_id, title, updated_at, issue)
               VALUES (1, 1, 'from an issue', 0, 'acme/api#12'), (2, 1, 'plain', 0, NULL);",
        )
        .unwrap();
        run(&mut conn).unwrap();
        let links: Vec<(i64, String, String, i64)> = conn
            .prepare("SELECT flow_id, kind, repo, number FROM flow_links")
            .unwrap()
            .query_map([], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(links, [(1, "issue".into(), "acme/api".into(), 12)]);
        conn.execute("DELETE FROM flows WHERE id = 1", []).unwrap();
        let left: i64 = conn
            .query_row("SELECT COUNT(*) FROM flow_links", [], |row| row.get(0))
            .unwrap();
        assert_eq!(left, 0);
    }

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

    /// Projects keep the order they showed in, by name, as their place.
    #[test]
    fn projects_get_their_place_by_name() {
        let mut conn = Connection::open_in_memory().unwrap();
        for sql in &MIGRATIONS[..11] {
            conn.execute_batch(sql).unwrap();
        }
        conn.pragma_update(None, "user_version", 11).unwrap();
        conn.execute_batch(
            "INSERT INTO projects (id, name, path, added_at) VALUES
               (1, 'web', '/code/web', 0), (2, 'API', '/code/api', 0), (3, 'docs', '/code/docs', 0);",
        )
        .unwrap();
        run(&mut conn).unwrap();
        let names: Vec<String> = conn
            .prepare("SELECT name FROM projects ORDER BY position")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(names, ["API", "docs", "web"]);
    }

    /// Flows keep the order the To do page showed them in, the most recent
    /// first, as their place there (TODO-3).
    #[test]
    fn flows_get_their_to_do_place_by_last_activity() {
        let mut conn = Connection::open_in_memory().unwrap();
        for sql in &MIGRATIONS[..14] {
            conn.execute_batch(sql).unwrap();
        }
        conn.pragma_update(None, "user_version", 14).unwrap();
        conn.execute_batch(
            "INSERT INTO projects (id, name, path, added_at) VALUES (1, 'api', '/code/api', 0);
             INSERT INTO flows (id, project_id, title, updated_at) VALUES
               (1, 1, 'old', 10), (2, 1, 'new', 30), (3, 1, 'tie', 10);",
        )
        .unwrap();
        run(&mut conn).unwrap();
        let titles: Vec<String> = conn
            .prepare("SELECT title FROM flows ORDER BY todo_position")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(titles, ["new", "tie", "old"]);
    }

    /// A flow archived before Leon knew when is counted from the
    /// migration, so it is not deleted at once (NAV-8).
    #[test]
    fn flows_already_archived_count_from_now() {
        let mut conn = Connection::open_in_memory().unwrap();
        for sql in &MIGRATIONS[..15] {
            conn.execute_batch(sql).unwrap();
        }
        conn.pragma_update(None, "user_version", 15).unwrap();
        conn.execute_batch(
            "INSERT INTO projects (id, name, path, added_at) VALUES (1, 'api', '/code/api', 0);
             INSERT INTO flows (id, project_id, title, updated_at, archived) VALUES
               (1, 1, 'archived', 10, 1), (2, 1, 'live', 10, 0);",
        )
        .unwrap();
        run(&mut conn).unwrap();
        let at: Vec<Option<i64>> = conn
            .prepare("SELECT archived_at FROM flows ORDER BY id")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        // 2026-01-01, well before any run of this test.
        assert!(at[0].is_some_and(|t| t > 1_767_225_600_000), "{at:?}");
        assert_eq!(at[1], None);
    }
}
