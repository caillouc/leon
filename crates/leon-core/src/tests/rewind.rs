//! Going back to an earlier message (CONV-7, CONV-8) and suggested replies
//! (CONV-10).

use super::*;

fn answer(h: &Harness, index: usize, text: &str) {
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
async fn going_back_drops_later_entries_and_restarts_with_context() {
    let h = harness();
    let id = new_flow(&h, BackendKind::Codex).await;
    next_command(&h, 0).await;
    emit(
        &h,
        0,
        BackendEvent::Ready {
            cli_session: "thread-1".into(),
        },
    );
    answer(&h, 0, "First answer");
    until(&h.core, "idle", |s| {
        s.conversations[&id].status == Status::Idle
    })
    .await;
    h.core.send(Intent::Send {
        conversation: id,
        text: "Second try".into(),
        images: Vec::new(),
    });
    next_command(&h, 0).await;
    answer(&h, 0, "Second answer");
    let s = until(&h.core, "two turns", |s| entries(s, id).len() == 6).await;
    assert_eq!(
        entries(&s, id)[3],
        Entry::User {
            text: "Second try".into()
        }
    );

    h.core.send(Intent::Rewind {
        conversation: id,
        entry: 3,
        text: "Other idea".into(),
        images: Vec::new(),
    });
    started(&h, 2).await;
    let restarted = h.backends.started.lock().unwrap()[1].spec.clone();
    assert_eq!(restarted.resume, None);
    let BackendCommand::Send(crate::ports::Prompt { text: message, .. }) =
        next_command(&h, 1).await
    else {
        panic!("expected a message");
    };
    assert!(
        message.starts_with("Context:")
            && message.contains("First answer")
            && message.ends_with("Other idea")
    );
    assert!(!message.contains("Second answer"));
    let s = until(&h.core, "rewound", |s| entries(s, id).len() == 5).await;
    let kept = entries(&s, id);
    assert_eq!(kept[2], Entry::TurnEnd { usage: None });
    assert!(
        matches!(&kept[3], Entry::Notice { text } if text.starts_with("Went back: 3 later entries"))
    );
    assert_eq!(
        kept[4],
        Entry::User {
            text: "Other idea".into()
        }
    );
    let stored = h.storage.entries(id).unwrap();
    assert_eq!(stored.len(), 5);
}

#[tokio::test(flavor = "multi_thread")]
async fn suggestions_show_until_the_next_message() {
    let h = harness();
    let id = new_flow(&h, BackendKind::ClaudeCode).await;
    next_command(&h, 0).await;
    answer(&h, 0, "Plan ready");
    emit(&h, 0, BackendEvent::Suggestion("Go ahead".into()));
    let s = until(&h.core, "suggestion", |s| {
        !s.conversations[&id].suggestions.is_empty()
    })
    .await;
    assert_eq!(
        s.conversations[&id].suggestions,
        vec!["Go ahead".to_owned()]
    );
    h.core.send(Intent::Send {
        conversation: id,
        text: "Go ahead".into(),
        images: Vec::new(),
    });
    until(&h.core, "cleared", |s| {
        s.conversations[&id].suggestions.is_empty()
    })
    .await;
}
