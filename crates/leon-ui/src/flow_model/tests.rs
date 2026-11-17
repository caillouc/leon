use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use leon_core::{Question, SessionView, TranscriptView};
use leon_domain::{
    AgentSession, AgentSessionId, BackendKind, ConversationId, Entry, SessionSettings, Status,
    TimedEntry,
};

use super::{EdgeKind, Node, Pt, Rect, View, build, layout, route};

fn session(id: i64, started_by: Option<i64>) -> SessionView {
    SessionView {
        session: AgentSession {
            id: AgentSessionId(id),
            conversation: ConversationId(1),
            started_by: started_by.map(AgentSessionId),
            settings: SessionSettings {
                backend: BackendKind::Codex,
                agent: None,
                model: None,
                effort: None,
                permission: "auto".into(),
            },
            working_dir: PathBuf::from("/code"),
            cli_session: None,
        },
        status: Status::Idle,
        subagents: (0, 0),
        question: None,
    }
}

fn transcript(entries: Vec<Entry>) -> Arc<TranscriptView> {
    Arc::new(TranscriptView {
        entries: Arc::new(
            (0..)
                .zip(entries)
                .map(|(at, entry)| TimedEntry { at, entry })
                .collect(),
        ),
        partial: None,
    })
}

fn texts(model: &super::FlowModel, from: Node, to: Node, kind: EdgeKind) -> Vec<String> {
    model
        .edges
        .iter()
        .find(|e| e.from == from && e.to == to && e.kind == kind)
        .map_or_default(|e| e.items.iter().map(|i| i.text.clone()).collect())
}

#[test]
fn arrows_come_from_what_the_sessions_did() {
    let (planner, coder) = (
        Node::Session(AgentSessionId(1)),
        Node::Session(AgentSessionId(2)),
    );
    let mut asking = session(2, Some(1));
    asking.question = Some(Question {
        id: "q".into(),
        title: "Run cargo test?".into(),
        detail: String::new(),
        options: vec![],
    });
    let sessions = [session(1, None), asking];
    let agent = |t: &str| Entry::Agent { text: t.into() };
    let end = || Entry::TurnEnd { usage: None };
    let transcripts = HashMap::from([
        (
            AgentSessionId(1),
            transcript(vec![
                Entry::User {
                    text: "Fix it".into(),
                },
                agent("Here is the plan"),
                end(),
                Entry::FromAgent {
                    from: AgentSessionId(2),
                    text: "Done".into(),
                    answer: true,
                },
                agent("All good"),
                end(),
            ]),
        ),
        (
            AgentSessionId(2),
            transcript(vec![
                Entry::FromAgent {
                    from: AgentSessionId(1),
                    text: "Do step 1".into(),
                    answer: false,
                },
                agent("Done"),
                end(),
            ]),
        ),
    ]);
    let refs: Vec<&SessionView> = sessions.iter().collect();
    let model = build(&refs, &transcripts);

    let cases = [
        (Node::You, planner, EdgeKind::Prompt, vec!["Fix it"]),
        (
            planner,
            Node::You,
            EdgeKind::Report,
            vec!["Here is the plan", "All good"],
        ),
        (planner, coder, EdgeKind::Prompt, vec!["Do step 1"]),
        (coder, planner, EdgeKind::Report, vec!["Done"]),
        (coder, Node::You, EdgeKind::Report, vec![]),
        (
            coder,
            Node::You,
            EdgeKind::Question,
            vec!["Run cargo test?"],
        ),
    ];
    for (from, to, kind, expected) in cases {
        assert_eq!(
            texts(&model, from, to, kind),
            expected,
            "{from:?} → {to:?} {kind:?}"
        );
    }
    assert_eq!(model.depth[&coder], 2);

    let pos = layout(&model);
    assert!(pos[&Node::You].x < pos[&planner].x && pos[&planner].x < pos[&coder].x);
}

#[test]
fn routes_go_straight_when_boxes_line_up() {
    let a = Rect {
        x: 0.,
        y: 0.,
        w: 100.,
        h: 50.,
    };
    let b = Rect {
        x: 300.,
        y: 0.,
        w: 100.,
        h: 50.,
    };
    assert_eq!(
        route(a, b, 0.),
        vec![Pt { x: 100., y: 25. }, Pt { x: 300., y: 25. }]
    );
    let below = Rect {
        x: 300.,
        y: 200.,
        w: 100.,
        h: 50.,
    };
    assert_eq!(route(a, below, 0.).len(), 4);
}

#[test]
fn zoom_keeps_the_point_under_the_cursor() {
    let view = View {
        scale: 1.,
        tx: 10.,
        ty: 20.,
    };
    let world = Pt { x: 50., y: 60. };
    let before = view.to_screen(world);
    let zoomed = view.zoom_at(before.x, before.y, 2.);
    let after = zoomed.to_screen(world);
    assert!((after.x - before.x).abs() < 0.01 && (after.y - before.y).abs() < 0.01);
    assert!((View { scale: 2.5, ..view }.zoom_at(0., 0., 2.).scale - 2.5).abs() < f32::EPSILON);
    let fit = View::fit(
        Rect {
            x: 0.,
            y: 0.,
            w: 400.,
            h: 100.,
        },
        900.,
        600.,
    );
    assert!((fit.scale - 1.1).abs() < f32::EPSILON);
}
