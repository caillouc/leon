//! The core's [`Storage`] port on SQLite.

use std::path::{Path, PathBuf};

use leon_core::ports::{NewConversation, PortError, Storage};
use leon_domain::{
    BackendKind, Conversation, ConversationId, ConversationSettings, Flow, FlowId, GhKind, GhRef,
    Project, ProjectId, TimedEntry,
};
use rusqlite::{Row, params};

use crate::Store;

fn err(context: &'static str) -> impl Fn(rusqlite::Error) -> PortError {
    move |e| PortError::new(context, e)
}

fn path_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// The `folders` column: a JSON array of paths; unreadable, none.
fn folders(json: &str) -> Vec<PathBuf> {
    serde_json::from_str::<Vec<String>>(json)
        .map_or_default(|paths| paths.into_iter().map(PathBuf::from).collect())
}

fn project_row(row: &Row<'_>) -> rusqlite::Result<Project> {
    Ok(Project {
        id: ProjectId(row.get(0)?),
        name: row.get(1)?,
        path: PathBuf::from(row.get::<_, String>(2)?),
        collapsed: row.get(3)?,
    })
}

/// How a link's kind is stored.
fn kind_text(kind: GhKind) -> &'static str {
    match kind {
        GhKind::Issue => "issue",
        GhKind::PullRequest => "pull_request",
    }
}

/// A stored link; an unknown kind is skipped.
fn link_row(row: &Row<'_>) -> rusqlite::Result<(i64, Option<GhRef>)> {
    let kind: String = row.get(1)?;
    let repo: String = row.get(2)?;
    let number = u64::try_from(row.get::<_, i64>(3)?).unwrap_or_default();
    let link = match kind.as_str() {
        "issue" => Some(GhRef::issue(&repo, number)),
        "pull_request" => Some(GhRef::pull_request(&repo, number)),
        _ => None,
    };
    Ok((row.get(0)?, link))
}

fn flow_row(row: &Row<'_>) -> rusqlite::Result<Flow> {
    Ok(Flow {
        id: FlowId(row.get(0)?),
        project: ProjectId(row.get(1)?),
        title: row.get(2)?,
        pinned: row.get(3)?,
        archived: row.get(4)?,
        unread: row.get(5)?,
        updated_at: row.get(6)?,
        folders: folders(&row.get::<_, String>(7)?),
        links: Vec::new(),
    })
}

const CONVERSATION_COLUMNS: &str = "id, flow_id, started_by, backend, agent, model, effort, permission, working_dir, cli_session, unread";

fn conversation_row(row: &Row<'_>) -> rusqlite::Result<Conversation> {
    let backend: String = row.get(3)?;
    Ok(Conversation {
        id: ConversationId(row.get(0)?),
        flow: FlowId(row.get(1)?),
        started_by: row.get::<_, Option<i64>>(2)?.map(ConversationId),
        settings: ConversationSettings {
            backend: BackendKind::from_id(&backend).unwrap_or(BackendKind::ClaudeCode),
            agent: row.get(4)?,
            model: row.get(5)?,
            effort: row.get(6)?,
            permission: row.get(7)?,
        },
        working_dir: PathBuf::from(row.get::<_, String>(8)?),
        cli_session: row.get(9)?,
        unread: row.get(10)?,
    })
}

impl Storage for Store {
    fn list_projects(&self) -> Result<Vec<Project>, PortError> {
        const C: &str = "listing projects";
        let conn = self.lock();
        let mut stmt = conn
            .prepare_cached(
                "SELECT id, name, path, collapsed FROM projects ORDER BY name COLLATE NOCASE, id",
            )
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
            "SELECT id, name, path, collapsed FROM projects WHERE path = ?1",
            [path_text(path)],
            project_row,
        )
        .map_err(err(C))
    }

    fn set_project_collapsed(&self, id: ProjectId, collapsed: bool) -> Result<(), PortError> {
        self.lock()
            .execute(
                "UPDATE projects SET collapsed = ?2 WHERE id = ?1",
                params![id.0, collapsed],
            )
            .map(drop)
            .map_err(err("folding a project"))
    }

    fn list_flows(&self) -> Result<Vec<Flow>, PortError> {
        const C: &str = "listing flows";
        let conn = self.lock();
        let mut stmt = conn
            .prepare_cached(
                "SELECT id, project_id, title, pinned, archived, unread, updated_at, folders FROM flows
                 ORDER BY updated_at DESC, id DESC",
            )
            .map_err(err(C))?;
        let mut flows: Vec<Flow> = stmt
            .query_map([], flow_row)
            .map_err(err(C))?
            .collect::<Result<_, _>>()
            .map_err(err(C))?;
        let mut links = conn
            .prepare_cached("SELECT flow_id, kind, repo, number FROM flow_links ORDER BY rowid")
            .map_err(err(C))?;
        for row in links.query_map([], link_row).map_err(err(C))? {
            let (id, link) = row.map_err(err(C))?;
            if let (Some(flow), Some(link)) = (flows.iter_mut().find(|f| f.id.0 == id), link) {
                flow.links.push(link);
            }
        }
        Ok(flows)
    }

    fn list_conversations(&self) -> Result<Vec<Conversation>, PortError> {
        const C: &str = "listing conversations";
        let conn = self.lock();
        let mut stmt = conn
            .prepare_cached(&format!(
                "SELECT {CONVERSATION_COLUMNS} FROM conversations ORDER BY id"
            ))
            .map_err(err(C))?;
        let rows = stmt.query_map([], conversation_row).map_err(err(C))?;
        rows.collect::<Result<_, _>>().map_err(err(C))
    }

    fn create_flow(&self, project: ProjectId, title: &str, now: i64) -> Result<Flow, PortError> {
        const C: &str = "creating a flow";
        let conn = self.lock();
        conn.execute(
            "INSERT INTO flows (project_id, title, updated_at) VALUES (?1, ?2, ?3)",
            params![project.0, title, now],
        )
        .map_err(err(C))?;
        Ok(Flow {
            id: FlowId(conn.last_insert_rowid()),
            project,
            title: title.to_owned(),
            pinned: false,
            archived: false,
            unread: false,
            updated_at: now,
            folders: Vec::new(),
            links: Vec::new(),
        })
    }

    fn update_flow(&self, c: &Flow) -> Result<(), PortError> {
        const C: &str = "saving a flow";
        let mut conn = self.lock();
        let tx = conn.transaction().map_err(err(C))?;
        tx.execute(
            "UPDATE flows SET title = ?2, pinned = ?3, archived = ?4, unread = ?5, updated_at = ?6,
             folders = ?7 WHERE id = ?1",
            params![
                c.id.0,
                c.title,
                c.pinned,
                c.archived,
                c.unread,
                c.updated_at,
                serde_json::Value::from(c.folders.iter().map(|p| path_text(p)).collect::<Vec<_>>())
                    .to_string(),
            ],
        )
        .map_err(err(C))?;
        // Its links, in their order.
        tx.execute("DELETE FROM flow_links WHERE flow_id = ?1", [c.id.0])
            .map_err(err(C))?;
        for link in &c.links {
            tx.execute(
                "INSERT OR IGNORE INTO flow_links (flow_id, kind, repo, number) VALUES (?1, ?2, ?3, ?4)",
                params![
                    c.id.0,
                    kind_text(link.kind),
                    link.repo,
                    i64::try_from(link.number).unwrap_or(i64::MAX)
                ],
            )
            .map_err(err(C))?;
        }
        tx.commit().map_err(err(C))
    }

    fn folded_groups(&self) -> Result<Vec<String>, PortError> {
        const C: &str = "reading the folded groups";
        let conn = self.lock();
        let mut stmt = conn
            .prepare("SELECT key FROM folded_groups ORDER BY key")
            .map_err(err(C))?;
        stmt.query_map([], |row| row.get(0))
            .map_err(err(C))?
            .collect::<Result<Vec<String>, _>>()
            .map_err(err(C))
    }

    fn set_folded_group(&self, key: &str, folded: bool) -> Result<(), PortError> {
        let sql = if folded {
            "INSERT OR IGNORE INTO folded_groups (key) VALUES (?1)"
        } else {
            "DELETE FROM folded_groups WHERE key = ?1"
        };
        self.lock()
            .execute(sql, [key])
            .map(drop)
            .map_err(err("folding a group"))
    }

    fn remove_project(&self, id: ProjectId) -> Result<(), PortError> {
        // Its flows, conversations and entries go with it (ON DELETE CASCADE).
        self.lock()
            .execute("DELETE FROM projects WHERE id = ?1", [id.0])
            .map(drop)
            .map_err(err("removing a project"))
    }

    fn delete_flow(&self, id: FlowId) -> Result<(), PortError> {
        self.lock()
            .execute("DELETE FROM flows WHERE id = ?1", [id.0])
            .map(drop)
            .map_err(err("deleting a flow"))
    }

    fn create_conversation(&self, new: &NewConversation) -> Result<Conversation, PortError> {
        const C: &str = "creating a conversation";
        let conn = self.lock();
        let s = &new.settings;
        conn.execute(
            "INSERT INTO conversations (flow_id, started_by, backend, agent, model, effort, permission, working_dir)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                new.flow.0,
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
        Ok(Conversation {
            id: ConversationId(conn.last_insert_rowid()),
            flow: new.flow,
            started_by: new.started_by,
            settings: new.settings.clone(),
            working_dir: new.working_dir.clone(),
            cli_session: None,
            unread: false,
        })
    }

    fn update_conversation(&self, a: &Conversation) -> Result<(), PortError> {
        let s = &a.settings;
        self.lock()
            .execute(
                "UPDATE conversations SET backend = ?2, agent = ?3, model = ?4, effort = ?5, permission = ?6,
                 working_dir = ?7, cli_session = ?8, unread = ?9 WHERE id = ?1",
                params![a.id.0, s.backend.id(), s.agent, s.model, s.effort, s.permission, path_text(&a.working_dir), a.cli_session, a.unread],
            )
            .map(drop)
            .map_err(err("saving a conversation"))
    }

    fn append_entry(
        &self,
        conversation: ConversationId,
        entry: &TimedEntry,
    ) -> Result<(), PortError> {
        const C: &str = "saving a transcript entry";
        let json = serde_json::to_string(&entry.entry).map_err(|e| PortError::new(C, e))?;
        let conn = self.lock();
        let next: i64 = conn
            .query_row(
                "SELECT COALESCE(MAX(seq), 0) + 1 FROM entries WHERE conversation_id = ?1",
                [conversation.0],
                |r| r.get(0),
            )
            .map_err(err(C))?;
        conn.execute(
            "INSERT INTO entries (conversation_id, seq, at, entry) VALUES (?1, ?2, ?3, ?4)",
            params![conversation.0, next, entry.at, json],
        )
        .map(drop)
        .map_err(err(C))
    }

    fn replace_entries(
        &self,
        conversation: ConversationId,
        entries: &[TimedEntry],
    ) -> Result<(), PortError> {
        const C: &str = "rewriting a transcript";
        let mut conn = self.lock();
        let tx = conn.transaction().map_err(err(C))?;
        tx.execute(
            "DELETE FROM entries WHERE conversation_id = ?1",
            [conversation.0],
        )
        .map_err(err(C))?;
        for (seq, entry) in (1i64..).zip(entries) {
            let json = serde_json::to_string(&entry.entry).map_err(|e| PortError::new(C, e))?;
            tx.execute(
                "INSERT INTO entries (conversation_id, seq, at, entry) VALUES (?1, ?2, ?3, ?4)",
                params![conversation.0, seq, entry.at, json],
            )
            .map_err(err(C))?;
        }
        tx.commit().map_err(err(C))
    }

    fn entries(&self, conversation: ConversationId) -> Result<Vec<TimedEntry>, PortError> {
        const C: &str = "reading a transcript";
        let conn = self.lock();
        let mut stmt = conn
            .prepare_cached("SELECT at, entry FROM entries WHERE conversation_id = ?1 ORDER BY seq")
            .map_err(err(C))?;
        let rows = stmt
            .query_map([conversation.0], |row| {
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
    /// Whether a flow exists (used by tests and checks).
    #[cfg(test)]
    fn has_flow(&self, id: FlowId) -> bool {
        use rusqlite::OptionalExtension as _;
        self.lock()
            .query_row("SELECT 1 FROM flows WHERE id = ?1", [id.0], |_| Ok(()))
            .optional()
            .ok()
            .flatten()
            .is_some()
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use leon_core::ports::{NewConversation, Storage};
    use leon_domain::{BackendKind, ConversationSettings, Entry, TimedEntry};

    use crate::Store;

    fn settings() -> ConversationSettings {
        ConversationSettings {
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
        store.set_project_collapsed(again.id, true).unwrap();
        let folded: Vec<_> = store
            .list_projects()
            .unwrap()
            .into_iter()
            .map(|p| p.collapsed)
            .collect();
        assert_eq!(folded, [false, true]);
    }

    #[test]
    fn flow_conversation_and_entries_round_trip() {
        let store = Store::open_in_memory().unwrap();
        let project = store.add_project("api", Path::new("/code/api"), 1).unwrap();
        let mut conv = store.create_flow(project.id, "Fix retry", 10).unwrap();
        let mut conversation = store
            .create_conversation(&NewConversation {
                flow: conv.id,
                started_by: None,
                settings: settings(),
                working_dir: PathBuf::from("/code/api"),
            })
            .unwrap();
        conversation.cli_session = Some("thread-1".into());
        conversation.unread = true;
        store.update_conversation(&conversation).unwrap();
        conv.pinned = true;
        conv.folders = vec![
            PathBuf::from("/code/api/.worktrees/fix"),
            PathBuf::from("/code/web/.worktrees/fix"),
        ];
        conv.links = vec![
            leon_domain::GhRef::issue("acme/api", 12),
            leon_domain::GhRef::pull_request("acme/web", 7),
        ];
        store.update_flow(&conv).unwrap();
        store
            .append_entry(
                conversation.id,
                &TimedEntry {
                    at: 11,
                    entry: Entry::User { text: "hi".into() },
                },
            )
            .unwrap();
        store
            .append_entry(
                conversation.id,
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
                conversation.id,
                &TimedEntry {
                    at: 13,
                    entry: Entry::FromAgent {
                        from: conversation.id,
                        text: "done".into(),
                        answer: true,
                    },
                },
            )
            .unwrap();

        assert_eq!(store.list_flows().unwrap(), vec![conv.clone()]);
        assert_eq!(
            store.list_conversations().unwrap(),
            vec![conversation.clone()]
        );
        let entries = store.entries(conversation.id).unwrap();
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
                from: conversation.id,
                text: "done".into(),
                answer: true,
            }
        );

        store
            .replace_entries(conversation.id, &entries[..1])
            .unwrap();
        assert_eq!(
            store.entries(conversation.id).unwrap(),
            entries[..1].to_vec()
        );
        store.append_entry(conversation.id, &entries[2]).unwrap();
        assert_eq!(store.entries(conversation.id).unwrap().len(), 2);

        store.delete_flow(conv.id).unwrap();
        assert!(!store.has_flow(conv.id));
        assert_eq!(store.entries(conversation.id).unwrap(), Vec::new());
    }

    #[test]
    fn removing_a_project_removes_its_flows_not_others() {
        let store = Store::open_in_memory().unwrap();
        let api = store.add_project("api", Path::new("/code/api"), 1).unwrap();
        let web = store.add_project("web", Path::new("/code/web"), 1).unwrap();
        let gone = store.create_flow(api.id, "Fix", 2).unwrap();
        let kept = store.create_flow(web.id, "Keep", 2).unwrap();
        let conversation = store
            .create_conversation(&NewConversation {
                flow: gone.id,
                started_by: None,
                settings: settings(),
                working_dir: PathBuf::from("/code/api"),
            })
            .unwrap();
        let entry = TimedEntry {
            at: 3,
            entry: Entry::User { text: "hi".into() },
        };
        store.append_entry(conversation.id, &entry).unwrap();

        store.remove_project(api.id).unwrap();
        let names: Vec<_> = store
            .list_projects()
            .unwrap()
            .into_iter()
            .map(|p| p.name)
            .collect();
        assert_eq!(names, ["web"]);
        assert!(!store.has_flow(gone.id));
        assert!(store.has_flow(kept.id));
        assert_eq!(store.entries(conversation.id).unwrap(), Vec::new());
    }

    #[test]
    fn folded_groups_are_kept() {
        let store = Store::open_in_memory().unwrap();
        assert_eq!(store.folded_groups().unwrap(), Vec::<String>::new());
        store.set_folded_group("In review", true).unwrap();
        store.set_folded_group("repo:acme/web", true).unwrap();
        store.set_folded_group("In review", true).unwrap();
        store.set_folded_group("repo:acme/web", false).unwrap();
        assert_eq!(store.folded_groups().unwrap(), ["In review"]);
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
