//! Questions from a CLI (CONV-9), and what the end of a turn does to what
//! still waits in it.

use super::*;

fn question(id: &str) -> BackendEvent {
    BackendEvent::Question {
        id: id.into(),
        title: "Access paths outside trusted directories".into(),
        detail: String::new(),
        options: vec!["Allow once".into(), "Deny".into()],
    }
}

fn shown(s: &Snapshot, id: ConversationId) -> Option<&str> {
    s.conversations[&id]
        .question
        .as_ref()
        .map(|q| q.id.as_str())
}

/// Two questions at once — two tools run side by side — are both asked, one
/// after the other: answering the first shows the second, and the
/// conversation needs the user until both are answered.
#[tokio::test(flavor = "multi_thread")]
async fn questions_asked_together_are_shown_one_after_the_other() {
    let h = harness();
    let id = new_flow(&h, BackendKind::CopilotCli).await;
    next_command(&h, 0).await;
    emit(&h, 0, question("q1"));
    emit(&h, 0, question("q2"));
    // Events arrive in order: once this one shows, both were asked.
    emit(&h, 0, BackendEvent::TextDelta("Checking".into()));
    let s = until(&h.core, "asked", |s| {
        s.transcripts[&id].partial.as_deref() == Some("Checking")
    })
    .await;
    assert_eq!(shown(&s, id), Some("q1"));
    for (answered, next, status) in [
        ("q1", Some("q2"), Status::NeedsInput),
        ("q2", None, Status::Running),
    ] {
        h.core.send(Intent::Answer {
            conversation: id,
            question: answered.into(),
            answer: "Allow once".into(),
        });
        assert_eq!(
            next_command(&h, 0).await,
            BackendCommand::Answer {
                id: answered.into(),
                answer: "Allow once".into()
            }
        );
        let s = until(&h.core, "answered", |s| shown(s, id) == next).await;
        assert_eq!(s.flows[0].status, status, "after {answered}");
    }
}

/// A turn that ends while a card waits — the user stopped it — takes the
/// card away: nothing waits for its answer any more (CONV-9).
#[tokio::test(flavor = "multi_thread")]
async fn a_stopped_turn_takes_its_question_away() {
    let h = harness();
    let id = new_flow(&h, BackendKind::ClaudeCode).await;
    next_command(&h, 0).await;
    emit(
        &h,
        0,
        BackendEvent::Question {
            id: "q1".into(),
            title: "Pick a colour".into(),
            detail: String::new(),
            options: vec!["Red".into(), "Blue".into()],
        },
    );
    until(&h.core, "card", |s| s.conversations[&id].question.is_some()).await;
    emit(
        &h,
        0,
        BackendEvent::TurnEnded {
            stopped: true,
            usage: None,
        },
    );
    let s = until(&h.core, "idle", |s| s.flows[0].status == Status::Idle).await;
    assert_eq!(s.conversations[&id].question, None);
}

/// A tool still without a result when its turn ends — the user stopped it,
/// or the CLI never said — no longer shows as running, and is saved so.
#[tokio::test(flavor = "multi_thread")]
async fn a_tool_without_a_result_ends_with_its_turn() {
    let h = harness();
    let id = new_flow(&h, BackendKind::CopilotCli).await;
    next_command(&h, 0).await;
    emit(
        &h,
        0,
        BackendEvent::Tool {
            id: "t1".into(),
            name: "Read".into(),
            detail: "Searching for secrets".into(),
            purpose: None,
        },
    );
    emit(
        &h,
        0,
        BackendEvent::TurnEnded {
            stopped: true,
            usage: None,
        },
    );
    let s = until(&h.core, "idle", |s| s.flows[0].status == Status::Idle).await;
    let ended = Entry::Tool {
        id: "t1".into(),
        name: "Read".into(),
        detail: "Searching for secrets".into(),
        purpose: None,
        output: Some(leon_domain::NO_RESULT.into()),
        failed: false,
    };
    assert!(entries(&s, id).contains(&ended), "{:?}", entries(&s, id));
    let saved = h.storage.entries.lock().unwrap()[&id].clone();
    assert!(saved.iter().any(|e| e.entry == ended), "{saved:?}");
}
