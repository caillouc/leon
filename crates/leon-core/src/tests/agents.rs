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
    fn server(&self, session: AgentSessionId) -> McpServer {
        McpServer {
            name: "leon".into(),
            command: PathBuf::from("/bin/leon"),
            args: vec!["mcp".into()],
            env: vec![("LEON_SESSION".into(), session.0.to_string())],
            trusted: true,
        }
    }
    fn instructions(&self) -> &'static str {
        "Keep Leon up to date."
    }
}

pub(super) async fn call(
    h: &Harness,
    session: AgentSessionId,
    call: AppToolCall,
) -> Result<String, String> {
    let (reply, answer) = oneshot::channel();
    h.core
        .app_tools()
        .send(AppToolRequest {
            session,
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
    let planner = new_conversation(&h, BackendKind::ClaudeCode).await;
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
    assert!(reply.starts_with("Started coder as session 2."), "{reply}");
    started(&h, 2).await;
    let coder = h.backends.started.lock().unwrap()[1].spec.clone();
    assert_eq!(coder.settings.agent.as_deref(), Some("coder"));
    assert_eq!(coder.working_dir, PathBuf::from("/code/api"));
    assert_eq!(
        next_command(&h, 1).await,
        BackendCommand::Send("[From planner · session 1]\n\nImplement step 1".into())
    );

    // The planner is still in its turn: the coder's answer waits for it.
    end_turn(&h, 1, "Step 1 done.");
    let s = until(&h.core, "coder idle", |s| {
        s.sessions
            .get(&AgentSessionId(2))
            .is_some_and(|x| x.status == Status::Idle)
    })
    .await;
    assert_eq!(
        s.sessions[&AgentSessionId(2)].session.started_by,
        Some(planner)
    );
    end_turn(&h, 0, "Waiting for the coder.");
    assert_eq!(
        next_command(&h, 0).await,
        BackendCommand::Send("[From coder · session 2]\n\nStep 1 done.".into())
    );
    let s = until(&h.core, "answer in the planner", |s| {
        entries(s, planner).last()
            == Some(&Entry::FromAgent {
                from: AgentSessionId(2),
                text: "Step 1 done.".into(),
                answer: true,
            })
    })
    .await;
    // Answers between agents are not the user's: no notification for them.
    assert_eq!(s.conversations[0].status, Status::Running);
}

#[tokio::test(flavor = "multi_thread")]
async fn bad_tool_calls_explain_what_is_wrong() {
    let h = harness();
    let planner = new_conversation(&h, BackendKind::Codex).await;
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
                session: AgentSessionId(99),
                text: "hi".into(),
            },
            "Session 99 is not in this conversation.",
        ),
        (
            AppToolCall::MessageAgent {
                session: planner,
                text: "hi".into(),
            },
            "A session cannot message itself.",
        ),
    ];
    for (tool, expected) in cases {
        assert_eq!(call(&h, planner, tool).await, Err(expected.to_owned()));
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn an_agent_can_rename_the_conversation() {
    let h = harness();
    let planner = new_conversation(&h, BackendKind::Codex).await;
    let rename = AppToolCall::RenameConversation {
        title: " Webhook retries ".into(),
    };
    assert!(call(&h, planner, rename).await.is_ok());
    until(&h.core, "renamed", |s| {
        s.conversations[0].conversation.title == "Webhook retries"
    })
    .await;
}

/// With several agents, only the one whose answer the user has not read is
/// to review; reading its tab reads the conversation (NAV-4, FLOW-4).
#[tokio::test(flavor = "multi_thread")]
async fn only_the_agent_that_answered_you_is_to_review() {
    let h = harness();
    // The user reads the planner's tab (intents reach the core in order).
    h.core.send(Intent::Read(Some(AgentSessionId(1))));
    let planner = new_conversation(&h, BackendKind::ClaudeCode).await;
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
    let coder = AgentSessionId(2);
    until(&h.core, "coder idle", |s| {
        s.sessions
            .get(&coder)
            .is_some_and(|x| x.status == Status::Idle)
    })
    .await;
    h.core.send(Intent::Send {
        session: coder,
        text: "Also step 2".into(),
    });
    next_command(&h, 1).await;
    end_turn(&h, 1, "Step 2 done.");
    let s = until(&h.core, "coder to review", |s| {
        s.sessions[&coder].session.unread
    })
    .await;
    assert!(!s.sessions[&planner].session.unread);
    assert!(s.conversations[0].conversation.unread);
    h.core.send(Intent::Read(Some(coder)));
    let s = until(&h.core, "read", |s| !s.conversations[0].conversation.unread).await;
    assert!(!s.sessions[&coder].session.unread);
}
