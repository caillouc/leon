//! The core's [`Storage`] port on SQLite.

use std::path::{Path, PathBuf};

use leon_core::ports::{NewSession, PortError, Storage};
use leon_domain::{
    AgentSession, AgentSessionId, BackendKind, Conversation, ConversationId, Project, ProjectId,
    SessionSettings, TimedEntry,
};
use rusqlite::{Row, params};

use crate::Store;

fn err(context: &'static str) -> impl Fn(rusqlite::Error) -> PortError {
    move |e| PortError::new(context, e)
}

fn path_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn project_row(row: &Row<'_>) -> rusqlite::Result<Project> {
    Ok(Project {
        id: ProjectId(row.get(0)?),
        name: row.get(1)?,
        path: PathBuf::from(row.get::<_, String>(2)?),
    })
}

fn conversation_row(row: &Row<'_>) -> rusqlite::Result<Conversation> {
    Ok(Conversation {
        id: ConversationId(row.get(0)?),
        project: ProjectId(row.get(1)?),
        title: row.get(2)?,
        pinned: row.get(3)?,
        archived: row.get(4)?,
        unread: row.get(5)?,
        updated_at: row.get(6)?,
        working_dir: row.get::<_, Option<String>>(7)?.map(PathBuf::from),
    })
}

const SESSION_COLUMNS: &str = "id, conversation_id, started_by, backend, agent, model, effort, permission, working_dir, cli_session";

fn session_row(row: &Row<'_>) -> rusqlite::Result<AgentSession> {
    let backend: String = row.get(3)?;
    Ok(AgentSession {
        id: AgentSessionId(row.get(0)?),
        conversation: ConversationId(row.get(1)?),
        started_by: row.get::<_, Option<i64>>(2)?.map(AgentSessionId),
        settings: SessionSettings {
            backend: BackendKind::from_id(&backend).unwrap_or(BackendKind::ClaudeCode),
            agent: row.get(4)?,
            model: row.get(5)?,
            effort: row.get(6)?,
            permission: row.get(7)?,
        },
        working_dir: PathBuf::from(row.get::<_, String>(8)?),
        cli_session: row.get(9)?,
    })
}

impl Storage for Store {
    fn list_projects(&self) -> Result<Vec<Project>, PortError> {
        const C: &str = "listing projects";
        let conn = self.lock();
        let mut stmt = conn
            .prepare_cached("SELECT id, name, path FROM projects ORDER BY name COLLATE NOCASE, id")
            .map_err(err(C))?;
        let rows = stmt.query_map([], project_row).map_err(err(C))?;
        rows.collect::<Result<_, _>>().map_err(err(C))
    }

    fn add_project(&self, name: &str, path: &Path, now: i64) -> Result<Project, PortError> {
        const C: &str = "adding a project";
        let conn = self.lock();
        conn.execute(
            "INSERT INTO projects (name, path, added_at) VALUES (?1, ?2, ?3) ON CONFLICT(path) DO NOTHING",
            params![name, path_text(path), now],
        )
        .map_err(err(C))?;
        conn.query_row(
            "SELECT id, name, path FROM projects WHERE path = ?1",
            [path_text(path)],
            project_row,
        )
        .map_err(err(C))
    }

    fn list_conversations(&self) -> Result<Vec<Conversation>, PortError> {
        const C: &str = "listing conversations";
        let conn = self.lock();
        let mut stmt = conn
            .prepare_cached(
                "SELECT id, project_id, title, pinned, archived, unread, updated_at, working_dir FROM conversations
                 ORDER BY updated_at DESC, id DESC",
            )
            .map_err(err(C))?;
        let rows = stmt.query_map([], conversation_row).map_err(err(C))?;
        rows.collect::<Result<_, _>>().map_err(err(C))
    }

    fn list_sessions(&self) -> Result<Vec<AgentSession>, PortError> {
        const C: &str = "listing agent sessions";
        let conn = self.lock();
        let mut stmt = conn
            .prepare_cached(&format!(
                "SELECT {SESSION_COLUMNS} FROM agent_sessions ORDER BY id"
            ))
            .map_err(err(C))?;
        let rows = stmt.query_map([], session_row).map_err(err(C))?;
        rows.collect::<Result<_, _>>().map_err(err(C))
    }

    fn create_conversation(
        &self,
        project: ProjectId,
        title: &str,
        now: i64,
    ) -> Result<Conversation, PortError> {
        const C: &str = "creating a conversation";
        let conn = self.lock();
        conn.execute(
            "INSERT INTO conversations (project_id, title, updated_at) VALUES (?1, ?2, ?3)",
            params![project.0, title, now],
        )
        .map_err(err(C))?;
        Ok(Conversation {
            id: ConversationId(conn.last_insert_rowid()),
            project,
            title: title.to_owned(),
            pinned: false,
            archived: false,
            unread: false,
            updated_at: now,
            working_dir: None,
        })
    }

    fn update_conversation(&self, c: &Conversation) -> Result<(), PortError> {
        self.lock()
            .execute(
                "UPDATE conversations SET title = ?2, pinned = ?3, archived = ?4, unread = ?5, updated_at = ?6,
                 working_dir = ?7 WHERE id = ?1",
                params![
                    c.id.0,
                    c.title,
                    c.pinned,
                    c.archived,
                    c.unread,
                    c.updated_at,
                    c.working_dir.as_deref().map(path_text)
                ],
            )
            .map(drop)
            .map_err(err("saving a conversation"))
    }

    fn delete_conversation(&self, id: ConversationId) -> Result<(), PortError> {
        self.lock()
            .execute("DELETE FROM conversations WHERE id = ?1", [id.0])
            .map(drop)
            .map_err(err("deleting a conversation"))
    }

    fn create_session(&self, new: &NewSession) -> Result<AgentSession, PortError> {
        const C: &str = "creating an agent session";
        let conn = self.lock();
        let s = &new.settings;
        conn.execute(
            "INSERT INTO agent_sessions (conversation_id, started_by, backend, agent, model, effort, permission, working_dir)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                new.conversation.0,
                new.started_by.map(|id| id.0),
                s.backend.id(),
                s.agent,
                s.model,
                s.effort,
                s.permission,
                path_text(&new.working_dir)
            ],
        )
        .map_err(err(C))?;
        Ok(AgentSession {
            id: AgentSessionId(conn.last_insert_rowid()),
            conversation: new.conversation,
            started_by: new.started_by,
            settings: new.settings.clone(),
            working_dir: new.working_dir.clone(),
            cli_session: None,
        })
    }

    fn update_session(&self, a: &AgentSession) -> Result<(), PortError> {
        let s = &a.settings;
        self.lock()
            .execute(
                "UPDATE agent_sessions SET backend = ?2, agent = ?3, model = ?4, effort = ?5, permission = ?6,
                 working_dir = ?7, cli_session = ?8 WHERE id = ?1",
                params![a.id.0, s.backend.id(), s.agent, s.model, s.effort, s.permission, path_text(&a.working_dir), a.cli_session],
            )
            .map(drop)
            .map_err(err("saving an agent session"))
    }

    fn append_entry(&self, session: AgentSessionId, entry: &TimedEntry) -> Result<(), PortError> {
        const C: &str = "saving a transcript entry";
        let json = serde_json::to_string(&entry.entry).map_err(|e| PortError::new(C, e))?;
        let conn = self.lock();
        let next: i64 = conn
            .query_row(
                "SELECT COALESCE(MAX(seq), 0) + 1 FROM entries WHERE session_id = ?1",
                [session.0],
                |r| r.get(0),
            )
            .map_err(err(C))?;
        conn.execute(
            "INSERT INTO entries (session_id, seq, at, entry) VALUES (?1, ?2, ?3, ?4)",
            params![session.0, next, entry.at, json],
        )
        .map(drop)
        .map_err(err(C))
    }

    fn replace_entries(
        &self,
        session: AgentSessionId,
        entries: &[TimedEntry],
    ) -> Result<(), PortError> {
        const C: &str = "rewriting a transcript";
        let mut conn = self.lock();
        let tx = conn.transaction().map_err(err(C))?;
        tx.execute("DELETE FROM entries WHERE session_id = ?1", [session.0])
            .map_err(err(C))?;
        for (seq, entry) in (1i64..).zip(entries) {
            let json = serde_json::to_string(&entry.entry).map_err(|e| PortError::new(C, e))?;
            tx.execute(
                "INSERT INTO entries (session_id, seq, at, entry) VALUES (?1, ?2, ?3, ?4)",
                params![session.0, seq, entry.at, json],
            )
            .map_err(err(C))?;
        }
        tx.commit().map_err(err(C))
    }

    fn entries(&self, session: AgentSessionId) -> Result<Vec<TimedEntry>, PortError> {
        const C: &str = "reading a transcript";
        let conn = self.lock();
        let mut stmt = conn
            .prepare_cached("SELECT at, entry FROM entries WHERE session_id = ?1 ORDER BY seq")
            .map_err(err(C))?;
        let rows = stmt
            .query_map([session.0], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(err(C))?;
        let mut out = Vec::new();
        for row in rows {
            let (at, json) = row.map_err(err(C))?;
            // An entry written by a newer Leon is skipped rather than failing
            // the whole transcript.
            if let Ok(entry) = serde_json::from_str(&json) {
                out.push(TimedEntry { at, entry });
            }
        }
        Ok(out)
    }
}

impl Store {
    /// Whether a conversation exists (used by tests and checks).
    #[cfg(test)]
    fn has_conversation(&self, id: ConversationId) -> bool {
        use rusqlite::OptionalExtension as _;
        self.lock()
            .query_row("SELECT 1 FROM conversations WHERE id = ?1", [id.0], |_| {
                Ok(())
            })
            .optional()
            .ok()
            .flatten()
            .is_some()
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use leon_core::ports::{NewSession, Storage};
    use leon_domain::{BackendKind, Entry, SessionSettings, TimedEntry};

    use crate::Store;

    fn settings() -> SessionSettings {
        SessionSettings {
            backend: BackendKind::Codex,
            agent: Some("planner".into()),
            model: None,
            effort: Some("high".into()),
            permission: "never ask".into(),
        }
    }

    #[test]
    fn projects_are_sorted_and_unique_by_path() {
        let store = Store::open_in_memory().unwrap();
        store.add_project("web", Path::new("/code/web"), 1).unwrap();
        store.add_project("Api", Path::new("/code/api"), 2).unwrap();
        let again = store
            .add_project("other name", Path::new("/code/web"), 3)
            .unwrap();
        assert_eq!(again.name, "web");
        let names: Vec<_> = store
            .list_projects()
            .unwrap()
            .into_iter()
            .map(|p| p.name)
            .collect();
        assert_eq!(names, ["Api", "web"]);
    }

    #[test]
    fn conversation_session_and_entries_round_trip() {
        let store = Store::open_in_memory().unwrap();
        let project = store.add_project("api", Path::new("/code/api"), 1).unwrap();
        let mut conv = store
            .create_conversation(project.id, "Fix retry", 10)
            .unwrap();
        let mut session = store
            .create_session(&NewSession {
                conversation: conv.id,
                started_by: None,
                settings: settings(),
                working_dir: PathBuf::from("/code/api"),
            })
            .unwrap();
        session.cli_session = Some("thread-1".into());
        store.update_session(&session).unwrap();
        conv.pinned = true;
        conv.working_dir = Some(PathBuf::from("/code/api/.worktrees/fix"));
        store.update_conversation(&conv).unwrap();
        store
            .append_entry(
                session.id,
                &TimedEntry {
                    at: 11,
                    entry: Entry::User { text: "hi".into() },
                },
            )
            .unwrap();
        store
            .append_entry(
                session.id,
                &TimedEntry {
                    at: 12,
                    entry: Entry::Agent {
                        text: "hello".into(),
                    },
                },
            )
            .unwrap();
        store
            .append_entry(
                session.id,
                &TimedEntry {
                    at: 13,
                    entry: Entry::FromAgent {
                        from: session.id,
                        text: "done".into(),
                        answer: true,
                    },
                },
            )
            .unwrap();

        assert_eq!(store.list_conversations().unwrap(), vec![conv.clone()]);
        assert_eq!(store.list_sessions().unwrap(), vec![session.clone()]);
        let entries = store.entries(session.id).unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(
            entries[1].entry,
            Entry::Agent {
                text: "hello".into()
            }
        );
        assert_eq!(
            entries[2].entry,
            Entry::FromAgent {
                from: session.id,
                text: "done".into(),
                answer: true,
            }
        );

        store.replace_entries(session.id, &entries[..1]).unwrap();
        assert_eq!(store.entries(session.id).unwrap(), entries[..1].to_vec());
        store.append_entry(session.id, &entries[2]).unwrap();
        assert_eq!(store.entries(session.id).unwrap().len(), 2);

        store.delete_conversation(conv.id).unwrap();
        assert!(!store.has_conversation(conv.id));
        assert_eq!(store.entries(session.id).unwrap(), Vec::new());
    }

    #[test]
    fn data_survives_reopening_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.db");
        Store::open(&path)
            .unwrap()
            .add_project("api", Path::new("/code/api"), 1)
            .unwrap();
        assert_eq!(
            Store::open(&path).unwrap().list_projects().unwrap().len(),
            1
        );
    }
}
