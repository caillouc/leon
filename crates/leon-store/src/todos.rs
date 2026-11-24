//! A flow's to-dos on SQLite (TODO-1).

use leon_core::ports::PortError;
use leon_domain::{ConversationId, FlowId, Todo, TodoId};
use rusqlite::{Connection, Row, params};

fn err(context: &'static str) -> impl Fn(rusqlite::Error) -> PortError {
    move |e| PortError::new(context, e)
}

fn todo_row(row: &Row<'_>) -> rusqlite::Result<Todo> {
    Ok(Todo {
        id: TodoId(row.get(0)?),
        flow: FlowId(row.get(1)?),
        what: row.get(2)?,
        done: row.get(3)?,
        by: row.get::<_, Option<i64>>(4)?.map(ConversationId),
        added_at: row.get(5)?,
        when: row.get(6)?,
    })
}

/// Every to-do, in the order added.
pub(crate) fn todos(conn: &Connection) -> Result<Vec<Todo>, PortError> {
    const C: &str = "reading the to-dos";
    conn.prepare_cached(
        "SELECT id, flow_id, text, done, added_by, added_at, waits_for FROM todos ORDER BY id",
    )
    .map_err(err(C))?
    .query_map([], todo_row)
    .map_err(err(C))?
    .collect::<Result<_, _>>()
    .map_err(err(C))
}

/// Adds a to-do; returns it.
pub(crate) fn add(
    conn: &Connection,
    flow: FlowId,
    (when, what): (&str, &str),
    by: Option<ConversationId>,
    now: i64,
) -> Result<Todo, PortError> {
    const C: &str = "adding a to-do";
    conn.execute(
        "INSERT INTO todos (flow_id, waits_for, text, added_by, added_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![flow.0, when, what, by.map(|c| c.0), now],
    )
    .map_err(err(C))?;
    Ok(Todo {
        id: TodoId(conn.last_insert_rowid()),
        flow,
        when: when.to_owned(),
        what: what.to_owned(),
        done: false,
        by,
        added_at: now,
    })
}

/// Marks a to-do done or open.
pub(crate) fn set_done(conn: &Connection, id: TodoId, done: bool) -> Result<(), PortError> {
    conn.execute(
        "UPDATE todos SET done = ?2 WHERE id = ?1",
        params![id.0, done],
    )
    .map(|_| ())
    .map_err(err("marking a to-do"))
}

/// Changes a to-do's when and what.
pub(crate) fn edit(
    conn: &Connection,
    id: TodoId,
    (when, what): (&str, &str),
) -> Result<(), PortError> {
    conn.execute(
        "UPDATE todos SET waits_for = ?2, text = ?3 WHERE id = ?1",
        params![id.0, when, what],
    )
    .map(|_| ())
    .map_err(err("editing a to-do"))
}

/// Removes a to-do.
pub(crate) fn remove(conn: &Connection, id: TodoId) -> Result<(), PortError> {
    conn.execute("DELETE FROM todos WHERE id = ?1", [id.0])
        .map(|_| ())
        .map_err(err("removing a to-do"))
}

/// Puts a to-do back as it was, its id kept (TODO-1).
pub(crate) fn restore(conn: &Connection, todo: &Todo) -> Result<(), PortError> {
    conn.execute(
        "INSERT OR REPLACE INTO todos (id, flow_id, waits_for, text, done, added_by, added_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            todo.id.0,
            todo.flow.0,
            todo.when,
            todo.what,
            todo.done,
            todo.by.map(|c| c.0),
            todo.added_at
        ],
    )
    .map(|_| ())
    .map_err(err("restoring a to-do"))
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use leon_core::ports::{NewConversation, Storage};
    use leon_domain::{BackendKind, ConversationSettings};

    use crate::Store;

    #[test]
    fn todos_round_trip_and_go_with_their_flow() {
        let store = Store::open_in_memory().unwrap();
        let project = store.add_project("api", Path::new("/code/api"), 1).unwrap();
        let flow = store.create_flow(project.id, "Fix retry", 1).unwrap();
        let other = store.create_flow(project.id, "Docs", 1).unwrap();
        let agent = store
            .create_conversation(&NewConversation {
                flow: flow.id,
                started_by: None,
                settings: ConversationSettings {
                    backend: BackendKind::Codex,
                    agent: Some("planner".into()),
                    model: None,
                    effort: None,
                    permission: "never ask".into(),
                },
                working_dir: PathBuf::from("/code/api"),
            })
            .unwrap();
        let mine = store
            .add_todo(flow.id, ("", "Run the migration"), None, 5)
            .unwrap();
        let its = store
            .add_todo(flow.id, ("Once QA is green", "Merge"), Some(agent.id), 6)
            .unwrap();
        store
            .add_todo(other.id, ("Monday", "Publish"), None, 7)
            .unwrap();
        store.set_todo_done(mine.id, true).unwrap();
        let todos = store.todos().unwrap();
        let shown: Vec<_> = todos
            .iter()
            .map(|t| (t.when.as_str(), t.what.as_str(), t.done, t.by))
            .collect();
        assert_eq!(
            shown,
            [
                ("", "Run the migration", true, None),
                ("Once QA is green", "Merge", false, Some(agent.id)),
                ("Monday", "Publish", false, None),
            ]
        );
        assert_eq!(todos[1], its);
        store
            .edit_todo(its.id, ("Once QA is green and Tom agrees", "Merge it"))
            .unwrap();
        let edited = store.todos().unwrap();
        assert_eq!(
            (edited[1].when.as_str(), edited[1].what.as_str()),
            ("Once QA is green and Tom agrees", "Merge it")
        );
        store.remove_todo(its.id).unwrap();
        assert_eq!(store.todos().unwrap().len(), 2);
        store.delete_flow(flow.id).unwrap();
        assert_eq!(store.todos().unwrap().len(), 1);
    }

    /// A to-do put back after its tick or removal is undone is as it was,
    /// its id and place kept (TODO-1).
    #[test]
    fn a_restored_todo_is_as_it_was() {
        let store = Store::open_in_memory().unwrap();
        let project = store.add_project("api", Path::new("/code/api"), 1).unwrap();
        let flow = store.create_flow(project.id, "Fix retry", 1).unwrap();
        let first = store
            .add_todo(flow.id, ("Monday", "Merge"), None, 5)
            .unwrap();
        store.add_todo(flow.id, ("", "Deploy"), None, 6).unwrap();
        let before = store.todos().unwrap();
        store.set_todo_done(first.id, true).unwrap();
        store.restore_todo(&first).unwrap();
        assert_eq!(store.todos().unwrap(), before);
        store.remove_todo(first.id).unwrap();
        store.restore_todo(&first).unwrap();
        assert_eq!(store.todos().unwrap(), before);
    }
}
