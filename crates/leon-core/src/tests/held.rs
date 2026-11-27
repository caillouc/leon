//! Coming back to CLIs that kept working while Leon was closed (I24).

use super::*;
use crate::ports::NewConversation;

/// A conversation recorded before Leon quit: the user's message, then the
/// start of the agent's answer.
fn recorded(storage: &FakeStorage) -> ConversationId {
    let flow = storage
        .create_flow(ProjectId(1), "Fix the retry bug", 0)
        .unwrap();
    let conversation = storage
        .create_conversation(&NewConversation {
            flow: flow.id,
            started_by: None,
            settings: settings(BackendKind::ClaudeCode),
            working_dir: PathBuf::from("/code/api"),
        })
        .unwrap();
    for (at, entry) in [
        (
            1,
            Entry::User {
                text: "Fix it".into(),
            },
        ),
        (
            2,
            Entry::Tool {
                id: "t1".into(),
                name: "Ran".into(),
                detail: "cargo test".into(),
                purpose: None,
                output: None,
                failed: false,
            },
        ),
    ] {
        storage
            .append_entry(conversation.id, &TimedEntry { at, entry })
            .unwrap();
    }
    conversation.id
}

#[tokio::test(flavor = "multi_thread")]
async fn a_turn_running_while_leon_was_closed_is_replayed() {
    let storage = Arc::new(FakeStorage::default());
    storage
        .add_project("api", Path::new("/code/api"), 0)
        .unwrap();
    let id = recorded(&storage);
    let backends = Arc::new(FakeBackends::default());
    backends.held.lock().unwrap().push((id, true));
    let core = start(
        &tokio::runtime::Handle::current(),
        ports(
            Arc::clone(&storage) as Arc<dyn Storage>,
            Arc::clone(&backends) as Arc<dyn Backends>,
        ),
        Settings::default(),
    );
    // Working, with only what started the turn: the rest comes again.
    let s = until(&core, "working", |s| {
        s.conversations
            .get(&id)
            .is_some_and(|c| c.status == Status::Running)
    })
    .await;
    assert_eq!(s.flows[0].status, Status::Running);
    let kept: Vec<Entry> = storage
        .entries(id)
        .unwrap()
        .into_iter()
        .map(|e| e.entry)
        .collect();
    assert_eq!(
        kept,
        [Entry::User {
            text: "Fix it".into()
        }]
    );
    let h = Harness {
        core,
        storage,
        backends,
        git: Arc::new(super::repo::FakeGit::default()),
    };
    emit(&h, 0, BackendEvent::Text("Fixed: retries now wait.".into()));
    emit(
        &h,
        0,
        BackendEvent::TurnEnded {
            stopped: false,
            usage: None,
        },
    );
    let s = until(&h.core, "replayed", |s| {
        s.conversations
            .get(&id)
            .is_some_and(|c| c.status == Status::Idle)
    })
    .await;
    h.core.send(Intent::View(Some(s.flows[0].flow.id)));
    let s = until(&h.core, "transcript", |s| s.transcripts.contains_key(&id)).await;
    let texts: Vec<&Entry> = s.transcripts[&id]
        .entries
        .iter()
        .map(|e| &e.entry)
        .collect();
    assert_eq!(
        texts[0],
        &Entry::User {
            text: "Fix it".into()
        }
    );
    assert!(
        texts.contains(&&Entry::Agent {
            text: "Fixed: retries now wait.".into()
        }),
        "{texts:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn an_idle_cli_found_again_stays_idle() {
    let storage = Arc::new(FakeStorage::default());
    storage
        .add_project("api", Path::new("/code/api"), 0)
        .unwrap();
    let id = recorded(&storage);
    let backends = Arc::new(FakeBackends::default());
    backends.held.lock().unwrap().push((id, false));
    let core = start(
        &tokio::runtime::Handle::current(),
        ports(
            Arc::clone(&storage) as Arc<dyn Storage>,
            Arc::clone(&backends) as Arc<dyn Backends>,
        ),
        Settings::default(),
    );
    let s = until(&core, "loaded", |s| s.loaded).await;
    assert_eq!(s.conversations[&id].status, Status::Idle);
    assert_eq!(storage.entries(id).unwrap().len(), 2, "nothing replayed");
    assert_eq!(backends.started.lock().unwrap().len(), 1, "live again");
}

/// An agent answering another's request while Leon restarts: the asker
/// still waits for it (FLOW-1, CONV-16), not "Done".
#[tokio::test(flavor = "multi_thread")]
async fn a_request_still_answered_across_a_restart_is_still_waited_for() {
    let storage = Arc::new(FakeStorage::default());
    storage
        .add_project("api", Path::new("/code/api"), 0)
        .unwrap();
    let planner = recorded(&storage);
    let flow = storage.list_flows().unwrap()[0].id;
    let reviewer = storage
        .create_conversation(&NewConversation {
            flow,
            started_by: Some(planner),
            settings: settings(BackendKind::ClaudeCode),
            working_dir: PathBuf::from("/code/api"),
        })
        .unwrap()
        .id;
    let asked = Entry::FromAgent {
        from: planner,
        text: "Review the diff".into(),
        answer: false,
    };
    storage
        .append_entry(
            reviewer,
            &TimedEntry {
                at: 3,
                entry: asked,
            },
        )
        .unwrap();
    let backends = Arc::new(FakeBackends::default());
    backends.held.lock().unwrap().push((reviewer, true));
    let core = start(
        &tokio::runtime::Handle::current(),
        ports(
            Arc::clone(&storage) as Arc<dyn Storage>,
            Arc::clone(&backends) as Arc<dyn Backends>,
        ),
        Settings::default(),
    );
    let s = until(&core, "working", |s| {
        s.conversations
            .get(&reviewer)
            .is_some_and(|c| c.status == Status::Running)
    })
    .await;
    assert_eq!(s.conversations[&planner].waiting_for, [reviewer]);
}

/// What waited for a busy agent when Leon quit comes back (I30): to an
/// agent idle by then, it goes at once; to one still in its turn, it waits
/// for the turn's end, shown queued.
#[tokio::test(flavor = "multi_thread")]
async fn messages_waiting_when_leon_quit_are_not_lost() {
    use crate::ports::{Sent, Waiting, WaitingFor};
    let storage = Arc::new(FakeStorage::default());
    storage
        .add_project("api", Path::new("/code/api"), 0)
        .unwrap();
    let planner = recorded(&storage);
    let flow = storage.list_flows().unwrap()[0].id;
    let started = |by| {
        storage
            .create_conversation(&NewConversation {
                flow,
                started_by: Some(by),
                settings: settings(BackendKind::ClaudeCode),
                working_dir: PathBuf::from("/code/api"),
            })
            .unwrap()
            .id
    };
    let (coder, reviewer) = (started(planner), started(planner));
    let asked = WaitingFor {
        queued: Vec::new(),
        inbox: vec![Waiting {
            from: planner,
            text: "Review the diff".into(),
            sent: Sent::Request,
        }],
    };
    storage.save_waiting(coder, &asked).unwrap();
    let yours = WaitingFor {
        queued: vec![("Also the docs".into(), Vec::new())],
        inbox: Vec::new(),
    };
    storage.save_waiting(reviewer, &yours).unwrap();
    let backends = Arc::new(FakeBackends::default());
    backends.held.lock().unwrap().push((reviewer, true));
    let core = start(
        &tokio::runtime::Handle::current(),
        ports(
            Arc::clone(&storage) as Arc<dyn Storage>,
            Arc::clone(&backends) as Arc<dyn Backends>,
        ),
        Settings::default(),
    );
    let s = until(&core, "sent to the idle coder", |s| {
        s.conversations
            .get(&coder)
            .is_some_and(|c| c.status == Status::Running)
    })
    .await;
    // The coder works on the request: the planner waits for it.
    assert!(s.conversations[&planner].waiting_for.contains(&coder));
    assert_eq!(
        storage.entries(coder).unwrap().last().map(|e| &e.entry),
        Some(&Entry::FromAgent {
            from: planner,
            text: "Review the diff".into(),
            answer: false,
        })
    );
    // The reviewer still answers: the user's message waits, queued.
    assert_eq!(
        s.conversations[&reviewer].queued,
        [("Also the docs".to_owned(), Vec::new())]
    );
    assert_eq!(storage.waiting().unwrap(), [(reviewer, yours)]);
}

/// A conversation whose last turn failed shows its error again after a
/// restart, in its flow too, until a message is sent to it.
#[tokio::test(flavor = "multi_thread")]
async fn an_error_survives_a_restart() {
    let h = harness();
    let id = new_flow(&h, BackendKind::ClaudeCode).await;
    next_command(&h, 0).await;
    emit(&h, 0, BackendEvent::Failed("Spend limit reached".into()));
    emit(
        &h,
        0,
        BackendEvent::TurnEnded {
            stopped: false,
            usage: None,
        },
    );
    until(&h.core, "failed", |s| {
        s.conversations[&id].status == Status::Error
    })
    .await;
    let saved = |storage: &FakeStorage| {
        storage
            .list_conversations()
            .unwrap()
            .into_iter()
            .find(|c| c.id == id)
            .is_some_and(|c| c.failed)
    };
    assert!(saved(&h.storage), "saved as failed");
    // Leon restarts on the same data.
    let again = start(
        &tokio::runtime::Handle::current(),
        ports(
            Arc::clone(&h.storage) as Arc<dyn Storage>,
            Arc::new(FakeBackends::default()) as Arc<dyn Backends>,
        ),
        Settings::default(),
    );
    let s = until(&again, "loaded", |s| s.loaded).await;
    assert_eq!(s.conversations[&id].status, Status::Error);
    assert_eq!(s.flows[0].status, Status::Error);
    // A new message: no longer failed.
    h.core.send(Intent::Send {
        conversation: id,
        text: "Try again".into(),
        images: Vec::new(),
    });
    until(&h.core, "running", |s| {
        s.conversations[&id].status == Status::Running
    })
    .await;
    assert!(!saved(&h.storage), "cleared by the next message");
}

#[test]
fn a_turn_starts_at_the_last_message_sent() {
    use crate::held::turn_start;
    let user = Entry::User { text: "go".into() };
    let agent = Entry::Agent {
        text: "done".into(),
    };
    let asked = Entry::FromAgent {
        from: ConversationId(1),
        text: "check".into(),
        answer: false,
    };
    let cases: [(&[&Entry], usize); 4] = [
        (&[], 0),
        (&[&user], 1),
        (&[&user, &agent], 1),
        (&[&user, &agent, &asked, &agent], 3),
    ];
    for (entries, kept) in cases {
        assert_eq!(turn_start(entries), kept, "{entries:?}");
    }
}

/// What is unread stays unread across a restart (NAV-4): the flow's "To
/// review" and its conversation's mark come back from the store.
#[tokio::test(flavor = "multi_thread")]
async fn unread_marks_survive_a_restart() {
    let storage = Arc::new(FakeStorage::default());
    storage
        .add_project("api", Path::new("/code/api"), 0)
        .unwrap();
    let id = recorded(&storage);
    let mut flow = storage.list_flows().unwrap().remove(0);
    flow.unread = true;
    storage.update_flow(&flow).unwrap();
    let mut conversation = storage.list_conversations().unwrap().remove(0);
    conversation.unread = true;
    storage.update_conversation(&conversation).unwrap();
    let core = start(
        &tokio::runtime::Handle::current(),
        ports(
            Arc::clone(&storage) as Arc<dyn Storage>,
            Arc::new(FakeBackends::default()) as Arc<dyn Backends>,
        ),
        Settings::default(),
    );
    let s = until(&core, "loaded", |s| s.loaded).await;
    assert!(s.flows[0].flow.unread);
    assert!(s.conversations[&id].conversation.unread);
    // Opened, the window active: nothing read until a tab is.
    core.send(Intent::WindowActive(true));
    core.send(Intent::View(Some(flow.id)));
    core.send(Intent::FoldGroup("x".into(), true));
    let s = until(&core, "handled", |s| s.folded_groups.contains("x")).await;
    assert!(s.flows[0].flow.unread, "viewing the Flow tab reads nothing");
    assert!(storage.list_flows().unwrap()[0].unread);
}
