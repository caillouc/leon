//! Turns, questions, stop and settings changes.

use super::*;

#[tokio::test(flavor = "multi_thread")]
async fn a_turn_streams_then_lands_in_the_transcript() {
    let h = harness();
    let id = new_conversation(&h, BackendKind::Codex).await;
    let spec = h.backends.started.lock().unwrap()[0].spec.clone();
    assert_eq!(spec.working_dir, PathBuf::from("/code/api"));
    assert_eq!(
        spec.agent.map(|a| a.instructions),
        Some("Plan carefully.".into())
    );
    assert_eq!(
        next_command(&h, 0).await,
        BackendCommand::Send("Fix the retry bug".into())
    );

    emit(
        &h,
        0,
        BackendEvent::Ready {
            cli_session: "thread-1".into(),
        },
    );
    emit(&h, 0, BackendEvent::TextDelta("Look".into()));
    let s = until(&h.core, "partial text", |s| {
        s.transcripts[&id].partial.as_deref() == Some("Look")
    })
    .await;
    assert_eq!(s.conversations[0].status, Status::Running);
    assert_eq!(s.conversations[0].conversation.title, "Fix the retry bug");

    emit(
        &h,
        0,
        BackendEvent::Tool {
            id: "t1".into(),
            name: "Ran".into(),
            detail: "cargo test".into(),
        },
    );
    emit(
        &h,
        0,
        BackendEvent::ToolDone {
            id: "t1".into(),
            output: "ok".into(),
            failed: false,
        },
    );
    emit(&h, 0, BackendEvent::Text("Looks fixed.".into()));
    emit(
        &h,
        0,
        BackendEvent::TurnEnded {
            stopped: false,
            usage: None,
        },
    );
    let s = until(&h.core, "idle", |s| {
        s.conversations[0].status == Status::Idle
    })
    .await;
    assert_eq!(
        entries(&s, id),
        [
            Entry::User {
                text: "Fix the retry bug".into()
            },
            Entry::Tool {
                id: "t1".into(),
                name: "Ran".into(),
                detail: "cargo test".into(),
                output: Some("ok".into()),
                failed: false
            },
            Entry::Agent {
                text: "Looks fixed.".into()
            },
            Entry::TurnEnd { usage: None },
        ]
    );
    assert_eq!(
        h.storage.sessions.lock().unwrap()[0].cli_session.as_deref(),
        Some("thread-1")
    );
    assert_eq!(h.storage.entries.lock().unwrap()[&id].len(), 4);
    assert!(
        !s.conversations[0].conversation.unread,
        "the conversation is on screen"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn an_answer_off_screen_marks_unread_and_notifies() {
    let h = harness();
    let id = new_conversation(&h, BackendKind::Codex).await;
    h.core.send(Intent::View(None));
    emit(&h, 0, BackendEvent::Text("Done.".into()));
    emit(
        &h,
        0,
        BackendEvent::TurnEnded {
            stopped: false,
            usage: None,
        },
    );
    let s = until(&h.core, "unread", |s| {
        s.conversations[0].conversation.unread
    })
    .await;
    assert_eq!(s.notifications.len(), 1);
    assert_eq!(s.notifications[0].title, "planner answered");
    h.core
        .send(Intent::View(Some(s.conversations[0].conversation.id)));
    until(&h.core, "read", |s| !s.conversations[0].conversation.unread).await;
    assert_eq!(entries(&s, id).len(), 3);
}

#[tokio::test(flavor = "multi_thread")]
async fn stop_and_questions_reach_the_cli() {
    let h = harness();
    let id = new_conversation(&h, BackendKind::Codex).await;
    next_command(&h, 0).await;
    h.core.send(Intent::Stop(id));
    assert_eq!(next_command(&h, 0).await, BackendCommand::Interrupt);

    emit(
        &h,
        0,
        BackendEvent::Question {
            id: "q1".into(),
            title: "Run it?".into(),
            detail: "rm -rf build".into(),
            options: vec!["Allow once".into(), "Deny".into()],
        },
    );
    let s = until(&h.core, "needs input", |s| {
        s.conversations[0].status == Status::NeedsInput
    })
    .await;
    assert_eq!(
        s.sessions[&id].question.as_ref().map(|q| q.detail.as_str()),
        Some("rm -rf build")
    );
    h.core.send(Intent::Answer {
        session: id,
        question: "q1".into(),
        answer: "Deny".into(),
    });
    assert_eq!(
        next_command(&h, 0).await,
        BackendCommand::Answer {
            id: "q1".into(),
            answer: "Deny".into()
        }
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn switching_backend_restarts_with_the_conversation_so_far() {
    let h = harness();
    let id = new_conversation(&h, BackendKind::Codex).await;
    emit(
        &h,
        0,
        BackendEvent::Ready {
            cli_session: "thread-1".into(),
        },
    );
    emit(&h, 0, BackendEvent::Text("Plan: raise the limit.".into()));
    emit(
        &h,
        0,
        BackendEvent::TurnEnded {
            stopped: false,
            usage: None,
        },
    );
    until(&h.core, "idle", |s| {
        s.conversations[0].status == Status::Idle
    })
    .await;

    h.core.send(Intent::ChangeSettings {
        session: id,
        settings: settings(BackendKind::ClaudeCode),
    });
    h.core.send(Intent::Send {
        session: id,
        text: "Go ahead".into(),
    });
    started(&h, 2).await;
    let spec = h.backends.started.lock().unwrap()[1].spec.clone();
    assert_eq!(spec.settings.backend, BackendKind::ClaudeCode);
    assert_eq!(spec.resume, None);
    let BackendCommand::Send(message) = next_command(&h, 1).await else {
        panic!("expected a message")
    };
    assert!(
        message.contains("Assistant: Plan: raise the limit."),
        "{message}"
    );
    assert!(message.ends_with("Go ahead"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_cli_that_cannot_start_shows_an_error() {
    struct Broken;
    impl Backends for Broken {
        fn start(&self, _spec: SessionSpec) -> Result<BackendChannel, PortError> {
            Err(PortError::new("starting Codex", "not installed"))
        }
        fn options(&self, _b: BackendKind) -> BackendOptions {
            BackendOptions::default()
        }
        fn title(&self, _b: BackendKind, _m: &str) -> Result<String, PortError> {
            Err(PortError::new("writing a title", "not installed"))
        }
    }
    let storage = Arc::new(FakeStorage::default());
    storage
        .add_project("api", Path::new("/code/api"), 0)
        .unwrap();
    let core = start(
        &tokio::runtime::Handle::current(),
        ports(storage, Arc::new(Broken)),
        Settings::default(),
    );
    until(&core, "loaded", |s| s.loaded).await;
    core.send(Intent::NewConversation {
        project: ProjectId(1),
        settings: settings(BackendKind::Codex),
        text: "hi".into(),
    });
    let s = until(&core, "error", |s| {
        s.conversations
            .first()
            .is_some_and(|c| c.status == Status::Error)
    })
    .await;
    let id = s.conversations[0].sessions[0];
    assert!(
        matches!(entries(&s, id).last(), Some(Entry::Error { text }) if text.contains("not installed"))
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_cli_writes_the_title() {
    let h = harness();
    *h.backends.title.lock().unwrap() = Some("Webhook retry fix".into());
    new_conversation(&h, BackendKind::Codex).await;
    until(&h.core, "title", |s| {
        s.conversations[0].conversation.title == "Webhook retry fix"
    })
    .await;
    assert_eq!(
        h.storage.list_conversations().unwrap()[0].title,
        "Webhook retry fix"
    );
}
