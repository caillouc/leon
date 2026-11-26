//! The messages waiting for a conversation's turn to end, on SQLite (I30):
//! the user's queued ones first (`kind` "user"), then other
//! conversations', in order.

use std::collections::BTreeMap;
use std::path::PathBuf;

use leon_core::ports::{PortError, Sent, Waiting, WaitingFor};
use leon_domain::ConversationId;
use rusqlite::{Connection, params};

fn err(context: &'static str) -> impl Fn(rusqlite::Error) -> PortError {
    move |e| PortError::new(context, e)
}

/// A row's `kind`.
fn kind(sent: Sent) -> &'static str {
    match sent {
        Sent::Request => "request",
        Sent::Answer => "answer",
        Sent::ForUser => "for_user",
    }
}

fn sent(kind: &str) -> Option<Sent> {
    match kind {
        "request" => Some(Sent::Request),
        "answer" => Some(Sent::Answer),
        "for_user" => Some(Sent::ForUser),
        _ => None,
    }
}

/// Every conversation's, by id; a row of a kind this build does not know
/// is left out.
pub(crate) fn waiting(conn: &Connection) -> Result<Vec<(ConversationId, WaitingFor)>, PortError> {
    const C: &str = "reading the waiting messages";
    type Row = (i64, Option<i64>, String, String, String);
    let rows: Vec<Row> = conn
        .prepare_cached(
            "SELECT conversation_id, from_id, kind, text, images FROM waiting_messages
             ORDER BY conversation_id, position",
        )
        .map_err(err(C))?
        .query_map([], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })
        .map_err(err(C))?
        .collect::<Result<_, _>>()
        .map_err(err(C))?;
    let mut all: BTreeMap<i64, WaitingFor> = BTreeMap::new();
    for (conversation, from, kind, text, images) in rows {
        let waiting = all.entry(conversation).or_default();
        match (from, sent(&kind)) {
            (None, _) if kind == "user" => {
                let images: Vec<PathBuf> = serde_json::from_str(&images).unwrap_or_default();
                waiting.queued.push((text, images));
            }
            (Some(from), Some(sent)) => waiting.inbox.push(Waiting {
                from: ConversationId(from),
                text,
                sent,
            }),
            _ => {}
        }
    }
    Ok(all
        .into_iter()
        .map(|(id, waiting)| (ConversationId(id), waiting))
        .collect())
}

pub(crate) fn save(
    conn: &mut Connection,
    id: ConversationId,
    waiting: &WaitingFor,
) -> Result<(), PortError> {
    const C: &str = "saving the waiting messages";
    let tx = conn.transaction().map_err(err(C))?;
    tx.execute(
        "DELETE FROM waiting_messages WHERE conversation_id = ?1",
        [id.0],
    )
    .map_err(err(C))?;
    let queued = waiting.queued.iter().map(|(text, images)| {
        let images = serde_json::to_string(images).unwrap_or_else(|_| "[]".to_owned());
        (None, "user", text.as_str(), images)
    });
    let inbox = waiting.inbox.iter().map(|w| {
        (
            Some(w.from.0),
            kind(w.sent),
            w.text.as_str(),
            "[]".to_owned(),
        )
    });
    for (position, (from, kind, text, images)) in queued.chain(inbox).enumerate() {
        tx.execute(
            "INSERT INTO waiting_messages (conversation_id, position, from_id, kind, text, images)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                id.0,
                i64::try_from(position).unwrap_or(i64::MAX),
                from,
                kind,
                text,
                images
            ],
        )
        .map_err(err(C))?;
    }
    tx.commit().map_err(err(C))
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use leon_core::ports::{NewConversation, Sent, Storage, Waiting, WaitingFor};
    use leon_domain::{BackendKind, ConversationSettings, ProjectId};

    use crate::Store;

    /// Saved, read back after a restart in the same order; saved empty, it
    /// is gone; its flow deleted, its messages go with it.
    #[test]
    fn waiting_messages_survive_a_restart() {
        let store = Store::open_in_memory().unwrap();
        store.add_project("api", Path::new("/code/api"), 0).unwrap();
        let flow = store.create_flow(ProjectId(1), "Fix", 0).unwrap();
        let new = |started_by| NewConversation {
            flow: flow.id,
            started_by,
            settings: ConversationSettings {
                backend: BackendKind::ClaudeCode,
                agent: None,
                model: None,
                effort: None,
                permission: "auto".into(),
            },
            working_dir: PathBuf::from("/code/api"),
        };
        let planner = store.create_conversation(&new(None)).unwrap().id;
        let coder = store.create_conversation(&new(Some(planner))).unwrap().id;
        let waiting = WaitingFor {
            queued: vec![
                ("Also the docs".into(), vec![PathBuf::from("/i/1.png")]),
                ("and the tests".into(), Vec::new()),
            ],
            inbox: vec![
                Waiting {
                    from: planner,
                    text: "Review it".into(),
                    sent: Sent::Request,
                },
                Waiting {
                    from: planner,
                    text: "Done".into(),
                    sent: Sent::Answer,
                },
            ],
        };
        store.save_waiting(coder, &waiting).unwrap();
        assert_eq!(store.waiting().unwrap(), [(coder, waiting)]);
        let only = WaitingFor {
            queued: Vec::new(),
            inbox: vec![Waiting {
                from: planner,
                text: "Next".into(),
                sent: Sent::ForUser,
            }],
        };
        store.save_waiting(coder, &only).unwrap();
        assert_eq!(store.waiting().unwrap(), [(coder, only)]);
        store.save_waiting(planner, &WaitingFor::default()).unwrap();
        assert_eq!(store.waiting().unwrap().len(), 1);
        store.delete_flow(flow.id).unwrap();
        assert_eq!(store.waiting().unwrap(), []);
    }
}
