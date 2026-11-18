//! Turns, questions, stop and settings changes.

use super::*;

#[tokio::test(flavor = "multi_thread")]
async fn a_turn_streams_then_lands_in_the_transcript() {
    let h = harness();
    let id = new_conversation(&h, BackendKind::Codex).await;
    let spec = h.backends.started.lock().unwrap()[0].spec.clone();
    assert_eq!(spec.working_dir, PathBuf::from("/code/api"));
    // `~/.agents/AGENTS.md` goes to every session (EXT-8).
    assert_eq!(spec.instructions.map(|i| i.text), Some("Be brief.".into()));
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
    // Shown on its Flow only: the answer was not read.
    assert!(s.conversations[0].conversation.unread);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_answer_read_on_its_agents_tab_is_read() {
    let h = harness();
    // The user reads the agent's messages (intents reach the core in order).
    h.core.send(Intent::Read(Some(ConversationId(1))));
    new_conversation(&h, BackendKind::Codex).await;
    emit(&h, 0, BackendEvent::Text("Done.".into()));
    emit(
        &h,
        0,
        BackendEvent::TurnEnded {
            stopped: false,
            usage: None,
        },
    );
    let s = until(&h.core, "answered", |s| {
        s.conversations[0].status == Status::Idle && !s.transcripts.is_empty()
    })
    .await;
    assert!(!s.conversations[0].conversation.unread);
    assert!(s.notifications.is_empty());
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
    let conversation = s.conversations[0].conversation.id;
    // Showing it (its Flow) does not read it; opening an agent's tab does.
    h.core.send(Intent::View(Some(conversation)));
    h.core.send(Intent::Read(Some(conversation)));
    until(&h.core, "read", |s| !s.conversations[0].conversation.unread).await;
    assert_eq!(entries(&s, id).len(), 3);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_answer_while_leon_is_in_the_background_is_unread() {
    let h = harness();
    // The conversation stays selected, but the user is in another app.
    // Sent first: intents reach the core in order, CLI events on their own.
    h.core.send(Intent::WindowActive(false));
    new_conversation(&h, BackendKind::Codex).await;
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
    // Coming back does not hide it: it stays to review until opened.
    h.core.send(Intent::WindowActive(true));
    let s = until(&h.core, "back", |s| s.conversations[0].conversation.unread).await;
    assert!(s.conversations[0].conversation.unread);
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
        fn doctor(&self) -> Vec<leon_domain::DoctorSection> {
            Vec::new()
        }
        fn connectors(&self, backend: BackendKind) -> leon_domain::CliConnectors {
            leon_domain::CliConnectors {
                backend,
                connectors: Vec::new(),
                problem: None,
            }
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
        issue: None,
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

#[test]
fn unchosen_model_and_effort_get_the_defaults() {
    use crate::ports::ModelOption;
    let model = |id: &str, label: &str| ModelOption {
        aliases: Vec::new(),
        id: id.into(),
        label: label.into(),
    };
    let options = BackendOptions {
        models: vec![model("m1", "Model 1"), model("m2", "Model 2")],
        efforts: vec!["medium".into(), "high".into()],
        default_effort: Some("high".into()),
        permissions: vec!["auto".into()],
    };
    let mut s = settings(BackendKind::Codex);
    options.complete(&mut s);
    assert_eq!(
        (s.model.as_deref(), s.effort.as_deref()),
        (Some("m1"), Some("high"))
    );
    let mut chosen = settings(BackendKind::Codex);
    chosen.model = Some("m2".into());
    chosen.effort = Some("medium".into());
    options.complete(&mut chosen);
    assert_eq!(
        (chosen.model.as_deref(), chosen.effort.as_deref()),
        (Some("m2"), Some("medium"))
    );
    assert_eq!(options.model_label("m2"), "Model 2");
    assert_eq!(options.model_label("other"), "other");
}

#[tokio::test(flavor = "multi_thread")]
async fn removing_a_project_forgets_it_and_its_conversations() {
    let h = harness();
    new_conversation(&h, BackendKind::Codex).await;
    h.core.send(Intent::RemoveProject(ProjectId(1)));
    let s = until(&h.core, "project removed", |s| s.projects.is_empty()).await;
    assert!(s.conversations.is_empty());
    assert!(s.sessions.is_empty());
    assert!(h.storage.projects.lock().unwrap().is_empty());
    assert!(h.storage.conversations.lock().unwrap().is_empty());
}
