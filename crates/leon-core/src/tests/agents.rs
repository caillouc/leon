//! Agents starting and messaging other agents (FLOW-1, FLOW-2, P15).

use leon_domain::McpServer;
use tokio::sync::oneshot;

use super::*;
use crate::ports::{AppToolCall, AppToolRequest, AppTools};

pub(super) struct FakeTools;

impl AppTools for FakeTools {
    fn doctor(&self) -> leon_domain::DoctorCheck {
        leon_domain::DoctorCheck::new(leon_domain::Health::Ok, "Tools for agents", "listening")
    }
    fn server(&self, conversation: ConversationId) -> McpServer {
        McpServer {
            name: "leon".into(),
            command: PathBuf::from("/bin/leon"),
            args: vec!["mcp".into()],
            env: vec![("LEON_SESSION".into(), conversation.0.to_string())],
            trusted: true,
        }
    }
    fn instructions(&self) -> &'static str {
        "Keep Leon up to date."
    }
}

pub(super) async fn call(
    h: &Harness,
    conversation: ConversationId,
    call: AppToolCall,
) -> Result<String, String> {
    let (reply, answer) = oneshot::channel();
    h.core
        .app_tools()
        .send(AppToolRequest {
            conversation,
            call,
            reply,
        })
        .unwrap();
    tokio::time::timeout(Duration::from_secs(2), answer)
        .await
        .unwrap()
        .unwrap()
}

fn end_turn(h: &Harness, index: usize, text: &str) {
    emit(h, index, BackendEvent::Text(text.into()));
    emit(
        h,
        index,
        BackendEvent::TurnEnded {
            stopped: false,
            usage: None,
        },
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_started_agent_answers_back_to_the_agent_that_asked() {
    let h = harness();
    let planner = new_flow(&h, BackendKind::ClaudeCode).await;
    next_command(&h, 0).await;
    let spec = h.backends.started.lock().unwrap()[0].spec.clone();
    assert_eq!(
        spec.mcp_servers[0].env,
        vec![("LEON_SESSION".into(), planner.0.to_string())]
    );

    let start = AppToolCall::StartAgent {
        agent: "coder".into(),
        prompt: "Implement step 1".into(),
        working_directory: None,
    };
    let reply = call(&h, planner, start).await.unwrap();
    assert!(
        reply.starts_with("Started coder as conversation 2."),
        "{reply}"
    );
    started(&h, 2).await;
    let coder = h.backends.started.lock().unwrap()[1].spec.clone();
    assert_eq!(coder.settings.agent.as_deref(), Some("coder"));
    assert_eq!(coder.working_dir, PathBuf::from("/code/api"));
    assert_eq!(
        next_command(&h, 1).await,
        BackendCommand::Send("[From planner · conversation 1]\n\nImplement step 1".into())
    );

    // The planner is still in its turn: the coder's answer waits for it.
    end_turn(&h, 1, "Step 1 done.");
    let s = until(&h.core, "coder idle", |s| {
        s.conversations
            .get(&ConversationId(2))
            .is_some_and(|x| x.status == Status::Idle)
    })
    .await;
    assert_eq!(
        s.conversations[&ConversationId(2)].conversation.started_by,
        Some(planner)
    );
    end_turn(&h, 0, "Waiting for the coder.");
    assert_eq!(
        next_command(&h, 0).await,
        BackendCommand::Send("[From coder · conversation 2]\n\nStep 1 done.".into())
    );
    let s = until(&h.core, "answer in the planner", |s| {
        entries(s, planner).last()
            == Some(&Entry::FromAgent {
                from: ConversationId(2),
                text: "Step 1 done.".into(),
                answer: true,
            })
    })
    .await;
    // Answers between agents are not the user's: no notification for them.
    assert_eq!(s.flows[0].status, Status::Running);
}

#[tokio::test(flavor = "multi_thread")]
async fn bad_tool_calls_explain_what_is_wrong() {
    let h = harness();
    let planner = new_flow(&h, BackendKind::Codex).await;
    let cases = [
        (
            AppToolCall::StartAgent {
                agent: "reviewer".into(),
                prompt: "Review".into(),
                working_directory: None,
            },
            "No agent named \"reviewer\". Agents: planner, coder.",
        ),
        (
            AppToolCall::StartAgent {
                agent: "coder".into(),
                prompt: "Go".into(),
                working_directory: Some(PathBuf::from("relative/dir")),
            },
            "relative/dir is not an absolute path.",
        ),
        (
            AppToolCall::MessageAgent {
                conversation: ConversationId(99),
                text: "hi".into(),
            },
            "Conversation 99 is not in this flow.",
        ),
        (
            AppToolCall::MessageAgent {
                conversation: planner,
                text: "hi".into(),
            },
            "A conversation cannot message itself.",
        ),
    ];
    for (tool, expected) in cases {
        assert_eq!(call(&h, planner, tool).await, Err(expected.to_owned()));
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn an_agent_can_rename_the_flow() {
    let h = harness();
    let planner = new_flow(&h, BackendKind::Codex).await;
    let rename = AppToolCall::RenameFlow {
        title: " Webhook retries ".into(),
    };
    assert!(call(&h, planner, rename).await.is_ok());
    until(&h.core, "renamed", |s| {
        s.flows[0].flow.title == "Webhook retries"
    })
    .await;
}

/// With several agents, only the one whose answer the user has not read is
/// to review; reading its tab reads the flow (NAV-4, FLOW-4).
#[tokio::test(flavor = "multi_thread")]
async fn only_the_agent_that_answered_you_is_to_review() {
    let h = harness();
    // The user reads the planner's tab (intents reach the core in order).
    h.core.send(Intent::Read(Some(ConversationId(1))));
    let planner = new_flow(&h, BackendKind::ClaudeCode).await;
    next_command(&h, 0).await;
    let start = AppToolCall::StartAgent {
        agent: "coder".into(),
        prompt: "Implement step 1".into(),
        working_directory: None,
    };
    call(&h, planner, start).await.unwrap();
    started(&h, 2).await;
    next_command(&h, 1).await;
    end_turn(&h, 1, "Step 1 done.");
    end_turn(&h, 0, "Waiting for the coder.");
    // The user asks the coder directly; its answer comes while the planner's
    // tab is shown.
    let coder = ConversationId(2);
    until(&h.core, "coder idle", |s| {
        s.conversations
            .get(&coder)
            .is_some_and(|x| x.status == Status::Idle)
    })
    .await;
    h.core.send(Intent::Send {
        conversation: coder,
        text: "Also step 2".into(),
    });
    next_command(&h, 1).await;
    end_turn(&h, 1, "Step 2 done.");
    let s = until(&h.core, "coder to review", |s| {
        s.conversations[&coder].conversation.unread
    })
    .await;
    assert!(!s.conversations[&planner].conversation.unread);
    assert!(s.flows[0].flow.unread);
    h.core.send(Intent::Read(Some(coder)));
    let s = until(&h.core, "read", |s| !s.flows[0].flow.unread).await;
    assert!(!s.conversations[&coder].conversation.unread);
}

/// An agent waiting for the agents it asked is not to review, and does not
/// notify: only its turn after the last report is (NAV-4, FLOW-4, NOTIF-1).
#[tokio::test(flavor = "multi_thread")]
async fn an_agent_is_to_review_once_every_report_is_back() {
    let h = harness();
    let planner = new_flow(&h, BackendKind::ClaudeCode).await;
    next_command(&h, 0).await;
    for (count, prompt) in [(2, "Implement step 1"), (3, "Implement step 2")] {
        let start = AppToolCall::StartAgent {
            agent: "coder".into(),
            prompt: prompt.into(),
            working_directory: None,
        };
        call(&h, planner, start).await.unwrap();
        started(&h, count).await;
        next_command(&h, count - 1).await;
    }
    let not_to_review = |s: &Snapshot| {
        !s.conversations[&planner].conversation.unread
            && !s.flows[0].flow.unread
            && s.notifications.is_empty()
    };
    // The first coder answers while the planner is in its turn: its report
    // waits, and goes to the planner as soon as that turn ends.
    end_turn(&h, 1, "Step 1 done.");
    until(&h.core, "first coder idle", |s| {
        s.conversations[&ConversationId(2)].status == Status::Idle
    })
    .await;
    end_turn(&h, 0, "Waiting for the coders.");
    next_command(&h, 0).await;
    let s = until(&h.core, "planner reads the report", |s| {
        s.conversations[&planner].status == Status::Running
    })
    .await;
    assert!(not_to_review(&s));
    // The second coder still works: the planner waits for it.
    end_turn(&h, 0, "Waiting for the second coder.");
    let s = until(&h.core, "planner idle", |s| {
        s.conversations[&planner].status == Status::Idle
    })
    .await;
    assert!(not_to_review(&s));
    // Its report is the last one: the planner's next answer is to review.
    end_turn(&h, 2, "Step 2 done.");
    next_command(&h, 0).await;
    end_turn(&h, 0, "Both steps are done.");
    let s = until(&h.core, "planner to review", |s| {
        s.conversations[&planner].conversation.unread
    })
    .await;
    assert!(s.flows[0].flow.unread);
    assert_eq!(s.notifications.len(), 1);
}

/// The user starts a second conversation in the same flow, in the folder
/// they picked (FLOW-7).
#[tokio::test(flavor = "multi_thread")]
async fn the_user_starts_another_flow_in_a_flow() {
    let h = harness();
    new_flow(&h, BackendKind::ClaudeCode).await;
    let flow = until(&h.core, "flow", |s| !s.flows.is_empty()).await.flows[0]
        .flow
        .id;
    h.core.send(Intent::NewConversation {
        flow,
        settings: settings(BackendKind::Codex),
        working_dir: PathBuf::from("/code/api/.worktrees/fix"),
        text: "Review the branch".into(),
    });
    started(&h, 2).await;
    assert_eq!(
        next_command(&h, 1).await,
        BackendCommand::Send("Review the branch".into())
    );
    let s = until(&h.core, "two conversations", |s| {
        s.flows[0].conversations.len() == 2
    })
    .await;
    let second = &s.conversations[&s.flows[0].conversations[1]].conversation;
    assert_eq!(second.started_by, None);
    assert_eq!(second.settings.backend, BackendKind::Codex);
    assert_eq!(
        second.working_dir,
        PathBuf::from("/code/api/.worktrees/fix")
    );
    assert_eq!(s.flows.len(), 1);
}
