//! Turns, questions, stop and settings changes.

use super::*;

#[tokio::test(flavor = "multi_thread")]
async fn a_turn_streams_then_lands_in_the_transcript() {
    let h = harness();
    let id = new_flow(&h, BackendKind::Codex).await;
    let spec = h.backends.started.lock().unwrap()[0].spec.clone();
    assert_eq!(spec.working_dir, PathBuf::from("/code/api"));
    // `~/.agents/AGENTS.md` goes to every conversation (EXT-8).
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
    assert_eq!(s.flows[0].status, Status::Running);
    assert_eq!(s.flows[0].flow.title, "Fix the retry bug");

    emit(
        &h,
        0,
        BackendEvent::Tool {
            id: "t1".into(),
            name: "Ran".into(),
            detail: "cargo test".into(),
            purpose: None,
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
    let s = until(&h.core, "idle", |s| s.flows[0].status == Status::Idle).await;
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
                purpose: None,
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
        h.storage.conversations.lock().unwrap()[0]
            .cli_session
            .as_deref(),
        Some("thread-1")
    );
    assert_eq!(h.storage.entries.lock().unwrap()[&id].len(), 4);
    // Shown on its Flow only: the answer was not read.
    assert!(s.flows[0].flow.unread);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_answer_read_on_its_agents_tab_is_read() {
    let h = harness();
    // The user reads the agent's messages (intents reach the core in order).
    h.core.send(Intent::Read(Some(ConversationId(1))));
    new_flow(&h, BackendKind::Codex).await;
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
        s.flows[0].status == Status::Idle && !s.transcripts.is_empty()
    })
    .await;
    assert!(!s.flows[0].flow.unread);
    assert!(s.notifications.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn an_answer_off_screen_marks_unread_and_notifies() {
    let h = harness();
    let id = new_flow(&h, BackendKind::Codex).await;
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
    let s = until(&h.core, "unread", |s| s.flows[0].flow.unread).await;
    assert_eq!(s.notifications.len(), 1);
    assert_eq!(s.notifications[0].title, "planner answered");
    let flow = s.flows[0].flow.id;
    // Showing it (its Flow) does not read it; opening an agent's tab does.
    assert!(s.conversations[&id].conversation.unread);
    h.core.send(Intent::View(Some(flow)));
    h.core.send(Intent::Read(Some(id)));
    let s = until(&h.core, "read", |s| !s.flows[0].flow.unread).await;
    assert!(!s.conversations[&id].conversation.unread);
    assert_eq!(entries(&s, id).len(), 3);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_answer_while_leon_is_in_the_background_is_unread() {
    let h = harness();
    // The flow stays selected, but the user is in another app.
    // Sent first: intents reach the core in order, CLI events on their own.
    h.core.send(Intent::WindowActive(false));
    new_flow(&h, BackendKind::Codex).await;
    emit(&h, 0, BackendEvent::Text("Done.".into()));
    emit(
        &h,
        0,
        BackendEvent::TurnEnded {
            stopped: false,
            usage: None,
        },
    );
    let s = until(&h.core, "unread", |s| s.flows[0].flow.unread).await;
    assert_eq!(s.notifications.len(), 1);
    // Coming back does not hide it: it stays to review until opened.
    h.core.send(Intent::WindowActive(true));
    let s = until(&h.core, "back", |s| s.flows[0].flow.unread).await;
    assert!(s.flows[0].flow.unread);
}

#[tokio::test(flavor = "multi_thread")]
async fn stop_and_questions_reach_the_cli() {
    let h = harness();
    let id = new_flow(&h, BackendKind::Codex).await;
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
        s.flows[0].status == Status::NeedsInput
    })
    .await;
    assert_eq!(
        s.conversations[&id]
            .question
            .as_ref()
            .map(|q| q.detail.as_str()),
        Some("rm -rf build")
    );
    h.core.send(Intent::Answer {
        conversation: id,
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
async fn switching_backend_restarts_with_the_flow_so_far() {
    let h = harness();
    let id = new_flow(&h, BackendKind::Codex).await;
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
    until(&h.core, "idle", |s| s.flows[0].status == Status::Idle).await;

    h.core.send(Intent::ChangeSettings {
        conversation: id,
        settings: settings(BackendKind::ClaudeCode),
    });
    h.core.send(Intent::Send {
        conversation: id,
        text: "Go ahead".into(),
        images: Vec::new(),
    });
    started(&h, 2).await;
    let spec = h.backends.started.lock().unwrap()[1].spec.clone();
    assert_eq!(spec.settings.backend, BackendKind::ClaudeCode);
    assert_eq!(spec.resume, None);
    let BackendCommand::Send(crate::ports::Prompt { text: message, .. }) =
        next_command(&h, 1).await
    else {
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
    core.send(Intent::NewFlow {
        project: ProjectId(1),
        settings: settings(BackendKind::Codex),
        text: "hi".into(),
        images: Vec::new(),
        issue: None,
    });
    let s = until(&core, "error", |s| {
        s.flows.first().is_some_and(|c| c.status == Status::Error)
    })
    .await;
    let id = s.flows[0].conversations[0];
    assert!(
        matches!(entries(&s, id).last(), Some(Entry::Error { text }) if text.contains("not installed"))
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_cli_writes_the_title() {
    let h = harness();
    *h.backends.title.lock().unwrap() = Some("Webhook retry fix".into());
    new_flow(&h, BackendKind::Codex).await;
    until(&h.core, "title", |s| {
        s.flows[0].flow.title == "Webhook retry fix"
    })
    .await;
    assert_eq!(
        h.storage.list_flows().unwrap()[0].title,
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
        efforts: Vec::new(),
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

/// A CLI's own list of models replaces the built-in one once it answers;
/// a model's own efforts show when its CLI lists them (I31).
#[tokio::test(flavor = "multi_thread")]
async fn the_models_come_from_the_cli() {
    use crate::ports::ModelOption;
    let storage = Arc::new(FakeStorage::default());
    let backends = Arc::new(FakeBackends::default());
    let model = |id: &str, efforts: &[&str]| ModelOption {
        id: id.into(),
        label: id.to_uppercase(),
        aliases: Vec::new(),
        efforts: efforts.iter().map(|e| (*e).to_owned()).collect(),
    };
    *backends.claude_models.lock().unwrap() = Some(vec![
        model("claude-new-6", &["low", "high"]),
        model("claude-old-4", &[]),
    ]);
    let core = start(
        &tokio::runtime::Handle::current(),
        ports(
            Arc::clone(&storage) as Arc<dyn Storage>,
            Arc::clone(&backends) as Arc<dyn Backends>,
        ),
        Settings::default(),
    );
    let s = until(&core, "listed", |s| {
        s.options
            .get(&BackendKind::ClaudeCode)
            .is_some_and(|o| o.models.len() == 2)
    })
    .await;
    let options = &s.options[&BackendKind::ClaudeCode];
    assert_eq!(options.models[0].id, "claude-new-6");
    assert_eq!(options.efforts_for(None), ["low", "high"]);
    // A model listing none: the backend's levels.
    assert_eq!(options.efforts_for(Some("claude-old-4")), options.efforts);
}

#[tokio::test(flavor = "multi_thread")]
async fn removing_a_project_forgets_it_and_its_flows() {
    let h = harness();
    new_flow(&h, BackendKind::Codex).await;
    h.core.send(Intent::RemoveProject(ProjectId(1)));
    let s = until(&h.core, "project removed", |s| s.projects.is_empty()).await;
    assert!(s.flows.is_empty());
    assert!(s.conversations.is_empty());
    assert!(h.storage.projects.lock().unwrap().is_empty());
    assert!(h.storage.flows.lock().unwrap().is_empty());
}

/// What a CLI says of its plan — windows and credits — is kept for that
/// CLI with when it said it, the latest replacing the one before, and saved
/// for the next launch (DOCTOR-2, USE-1).
#[tokio::test(flavor = "multi_thread")]
async fn a_clis_latest_plan_usage_is_kept() {
    let h = harness();
    new_flow(&h, BackendKind::Codex).await;
    started(&h, 1).await;
    let window = |used| leon_domain::LimitWindow {
        minutes: 300,
        used,
        resets_at: Some(1_000),
        model: None,
    };
    let said = |used, credits: Option<&str>| BackendEvent::Limits {
        windows: vec![window(used)],
        credits: credits.map(str::to_owned),
    };
    emit(&h, 0, said(10, Some("1300")));
    until(&h.core, "first usage", |s| !s.usage.is_empty()).await;
    emit(&h, 0, said(42, None));
    let s = until(&h.core, "latest usage", |s| {
        s.usage.first().is_some_and(|u| u.windows[0].used == 42)
    })
    .await;
    let expected = leon_domain::PlanUsage {
        backend: BackendKind::Codex,
        windows: vec![window(42)],
        credits: None,
        at: 1_000,
    };
    assert_eq!(*s.usage, std::slice::from_ref(&expected));
    assert_eq!(*h.storage.plan_usage.lock().unwrap(), [expected]);
}

/// What a CLI said of its plan before Leon started shows at once; one that
/// can tell without a turn (Codex) is asked at launch (DOCTOR-2).
#[tokio::test(flavor = "multi_thread")]
async fn plan_usage_shows_from_the_start() {
    let storage = Arc::new(FakeStorage::default());
    let old = leon_domain::PlanUsage {
        backend: BackendKind::ClaudeCode,
        windows: Vec::new(),
        credits: None,
        at: 500,
    };
    storage.plan_usage.lock().unwrap().push(old.clone());
    let core = start(
        &tokio::runtime::Handle::current(),
        ports(
            Arc::clone(&storage) as Arc<dyn Storage>,
            Arc::new(FakeBackends::default()),
        ),
        Settings::default(),
    );
    core.send(Intent::RunDoctor);
    let s = until(&core, "both", |s| s.usage.len() == 2).await;
    assert_eq!(s.usage[0], old);
    assert_eq!(s.usage[1].backend, BackendKind::Codex);
    assert_eq!(s.usage[1].credits.as_deref(), Some("900"));
}

/// Projects take the order the user drags them into, kept by the store
/// (NAV-2).
#[tokio::test(flavor = "multi_thread")]
async fn projects_take_the_order_given() {
    let h = harness();
    until(&h.core, "loaded", |s| s.loaded).await;
    h.core.send(Intent::AddProject(PathBuf::from("/code/web")));
    let s = until(&h.core, "two projects", |s| s.projects.len() == 2).await;
    let (api, web) = (s.projects[0].id, s.projects[1].id);
    h.core.send(Intent::OrderProjects(vec![web, api]));
    let s = until(&h.core, "ordered", |s| s.projects[0].id == web).await;
    assert_eq!(s.projects[1].id, api);
    let stored: Vec<_> = h
        .storage
        .list_projects()
        .unwrap()
        .iter()
        .map(|p| p.id)
        .collect();
    assert_eq!(stored, [web, api]);
}

#[tokio::test(flavor = "multi_thread")]
async fn folded_groups_are_saved() {
    let h = harness();
    until(&h.core, "loaded", |s| s.loaded).await;
    h.core.send(Intent::FoldGroup("In review".into(), true));
    let s = until(&h.core, "folded", |s| s.folded_groups.contains("In review")).await;
    assert_eq!(s.folded_groups.len(), 1);
    h.core.send(Intent::FoldGroup("In review".into(), false));
    until(&h.core, "unfolded", |s| s.folded_groups.is_empty()).await;
}

/// Images put in a message are saved with the conversation's files and go
/// to the CLI with the text; one that cannot be saved is left out (CONV-18).
#[tokio::test(flavor = "multi_thread")]
async fn images_go_with_the_message() {
    use crate::ports::{Image, Prompt};
    let h = harness();
    let id = new_flow(&h, BackendKind::Codex).await;
    next_command(&h, 0).await;
    emit(
        &h,
        0,
        BackendEvent::TurnEnded {
            stopped: false,
            usage: None,
        },
    );
    until(&h.core, "idle", |s| s.flows[0].status == Status::Idle).await;
    h.core.send(Intent::Send {
        conversation: id,
        text: "Why does [Image #1] differ from [Image #2]?".into(),
        images: vec![
            Image::Pasted {
                bytes: std::sync::Arc::from(&b"png"[..]),
                ext: "png",
            },
            Image::File(PathBuf::from("/tmp/broken")),
            Image::File(PathBuf::from("/Users/me/shot.jpg")),
        ],
    });
    assert_eq!(
        next_command(&h, 0).await,
        BackendCommand::Send(Prompt {
            text: "Why does [Image #1] differ from [Image #2]?".into(),
            images: vec![
                PathBuf::from(format!("/data/images/{}/1.png", id.0)),
                PathBuf::from(format!("/data/images/{}/shot.jpg", id.0)),
            ],
        })
    );
}

/// A queued message edited goes back to its place; edited after the
/// answer ended, it is sent at once (CONV-19).
#[tokio::test(flavor = "multi_thread")]
async fn an_edited_queued_message_goes_back_to_its_place() {
    let h = harness();
    let id = new_flow(&h, BackendKind::Codex).await;
    next_command(&h, 0).await;
    for text in ["first", "second", "third"] {
        h.core.send(Intent::Send {
            conversation: id,
            text: text.into(),
            images: Vec::new(),
        });
    }
    let queued = |s: &crate::Snapshot| -> Vec<String> {
        s.conversations[&id]
            .queued
            .iter()
            .map(|(text, _)| text.clone())
            .collect()
    };
    until(&h.core, "queued", |s| queued(s).len() == 3).await;
    h.core.send(Intent::Unqueue(id, 1));
    until(&h.core, "out to edit", |s| queued(s).len() == 2).await;
    let requeue = |text: &str| {
        h.core.send(Intent::Requeue {
            conversation: id,
            at: 1,
            text: text.into(),
            images: Vec::new(),
        });
    };
    requeue("second, edited");
    let s = until(&h.core, "back", |s| queued(s).len() == 3).await;
    assert_eq!(queued(&s), ["first", "second, edited", "third"]);
    h.core.send(Intent::Unqueue(id, 1));
    until(&h.core, "out again", |s| queued(s).len() == 2).await;
    emit(
        &h,
        0,
        BackendEvent::TurnEnded {
            stopped: false,
            usage: None,
        },
    );
    assert_eq!(
        next_command(&h, 0).await,
        BackendCommand::Send("first\n\nthird".into())
    );
    emit(
        &h,
        0,
        BackendEvent::TurnEnded {
            stopped: false,
            usage: None,
        },
    );
    until(&h.core, "idle", |s| s.flows[0].status == Status::Idle).await;
    requeue("second, at last");
    assert_eq!(
        next_command(&h, 0).await,
        BackendCommand::Send("second, at last".into())
    );
}

/// Messages sent while the agent answers wait, each shown queued, and go
/// as one when the answer ends; one removed does not go (CONV-19).
#[tokio::test(flavor = "multi_thread")]
async fn a_message_sent_while_the_agent_answers_goes_when_it_ends() {
    let h = harness();
    let id = new_flow(&h, BackendKind::Codex).await;
    next_command(&h, 0).await;
    let send = |text: &str| {
        h.core.send(Intent::Send {
            conversation: id,
            text: text.into(),
            images: Vec::new(),
        });
    };
    send("Also check the tests");
    send("and the docs");
    let queued = |s: &crate::Snapshot| -> Vec<String> {
        s.conversations[&id]
            .queued
            .iter()
            .map(|(text, _)| text.clone())
            .collect()
    };
    let s = until(&h.core, "queued", |s| queued(s).len() == 2).await;
    assert_eq!(queued(&s), ["Also check the tests", "and the docs"]);
    assert_eq!(s.flows[0].status, Status::Running);
    // Saved, so a restart keeps them (I30).
    assert_eq!(h.storage.waiting().unwrap()[0].1.queued.len(), 2);
    let end = || {
        emit(
            &h,
            0,
            BackendEvent::TurnEnded {
                stopped: false,
                usage: None,
            },
        );
    };
    end();
    assert_eq!(
        next_command(&h, 0).await,
        BackendCommand::Send("Also check the tests\n\nand the docs".into())
    );
    let s = until(&h.core, "sent", |s| s.conversations[&id].queued.is_empty()).await;
    assert_eq!(s.flows[0].status, Status::Running);
    assert_eq!(h.storage.waiting().unwrap(), [], "sent: no longer saved");
    // Removed before the end: nothing goes.
    send("Never mind");
    send("Keep this one");
    until(&h.core, "queued again", |s| queued(s).len() == 2).await;
    h.core.send(Intent::Unqueue(id, 0));
    let s = until(&h.core, "removed", |s| queued(s).len() == 1).await;
    assert_eq!(queued(&s), ["Keep this one"]);
    h.core.send(Intent::Unqueue(id, 0));
    until(&h.core, "removed", |s| queued(s).is_empty()).await;
    end();
    let s = until(&h.core, "idle", |s| s.flows[0].status == Status::Idle).await;
    assert!(
        !entries(&s, id)
            .iter()
            .any(|e| matches!(e, Entry::User { text } if text == "Never mind"))
    );
}
