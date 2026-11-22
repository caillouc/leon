//! Claude Code's questions and approvals, on its recorded requests
//! (2.1.288, 2026-11-20).

use leon_core::ports::BackendEvent;
use serde_json::{Value, json};

use super::Asks;

fn recorded(file: &str) -> Value {
    let text = match file {
        "ask" => include_str!("../../fixtures/claude/2.1.288/ask-user-question.jsonl"),
        _ => include_str!("../../fixtures/claude/2.1.288/approval.jsonl"),
    };
    serde_json::from_str(text.trim()).unwrap()
}

fn card(id: &str, title: &str, detail: &str, options: &[&str]) -> BackendEvent {
    BackendEvent::Question {
        id: id.into(),
        title: title.into(),
        detail: detail.into(),
        options: options.iter().map(|o| (*o).to_owned()).collect(),
    }
}

/// The `response` of an answer line.
fn decision(line: Option<String>) -> Value {
    let v: Value = serde_json::from_str(line.unwrap().trim()).unwrap();
    assert_eq!(v["type"], "control_response");
    assert_eq!(v["response"]["subtype"], "success");
    v["response"].clone()
}

#[test]
fn a_question_shows_its_options_and_sends_the_pick_back() {
    let mut asks = Asks::default();
    let request = recorded("ask");
    let id = "00000000-0000-4000-8000-000000000003#0";
    assert_eq!(
        asks.request(&request),
        [card(
            id,
            "Pick a colour",
            "Red: The colour red\nBlue: The colour blue",
            &["Red", "Blue"]
        )]
    );
    assert!(asks.waiting());
    let (events, line) = asks.answer(Some(id), "Blue");
    assert_eq!(events, []);
    let response = decision(line);
    assert_eq!(
        response["request_id"],
        "00000000-0000-4000-8000-000000000003"
    );
    let mut input = request["request"]["input"].clone();
    input["answers"] = json!({ "Pick a colour": "Blue" });
    assert_eq!(
        response["response"],
        json!({ "behavior": "allow", "updatedInput": input })
    );
    assert!(!asks.waiting());
}

#[test]
fn several_questions_come_one_after_another() {
    let mut asks = Asks::default();
    let option = |label: &str| json!({ "label": label, "description": "" });
    let request = json!({
        "type": "control_request",
        "request_id": "r",
        "request": { "subtype": "can_use_tool", "tool_name": "AskUserQuestion", "input": { "questions": [
            { "question": "Base branch?", "options": [option("dev"), option("main")] },
            { "question": "Commit when?", "options": [option("When asked"), option("Always")] },
        ] } },
    });
    assert_eq!(
        asks.request(&request),
        [card("r#0", "Base branch?", "", &["dev", "main"])]
    );
    // A click on a card no longer shown does nothing.
    assert_eq!(asks.answer(Some("r#1"), "Always"), (vec![], None));
    assert_eq!(
        asks.answer(Some("r#0"), "dev"),
        (
            vec![card("r#1", "Commit when?", "", &["When asked", "Always"])],
            None
        )
    );
    // A typed message answers the card shown.
    let (_, line) = asks.answer(None, "only when I say so");
    assert_eq!(
        decision(line)["response"]["updatedInput"]["answers"],
        json!({ "Base branch?": "dev", "Commit when?": "only when I say so" })
    );
}

#[test]
fn an_approval_is_allowed_or_denied() {
    let request = recorded("approval");
    let id = "00000000-0000-4000-8000-000000000004";
    let shown = card(
        id,
        "Run this command?",
        "touch probe.txt",
        &["Allow once", "Deny"],
    );
    let cases = [
        (
            "Allow once",
            json!({ "behavior": "allow", "updatedInput": request["request"]["input"] }),
        ),
        (
            "Deny",
            json!({ "behavior": "deny", "message": "The user denied this." }),
        ),
        (
            "use mkdir instead",
            json!({ "behavior": "deny", "message": "The user denied this and said: use mkdir instead" }),
        ),
    ];
    for (answer, expected) in cases {
        let mut asks = Asks::default();
        assert_eq!(asks.request(&request), std::slice::from_ref(&shown));
        let (_, line) = asks.answer(Some(id), answer);
        assert_eq!(decision(line)["response"], expected, "{answer}");
    }
}

#[test]
fn other_control_requests_are_not_cards() {
    let mut asks = Asks::default();
    let other = json!({ "type": "control_request", "request_id": "x", "request": { "subtype": "hook_callback" } });
    assert_eq!(asks.request(&other), []);
    assert!(!asks.waiting());
    assert_eq!(asks.answer(None, "hi"), (vec![], None));
}
