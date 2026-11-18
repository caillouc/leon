use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use leon_core::{Question, SessionView, TranscriptView};
use leon_domain::{
    AgentSession, AgentSessionId, BackendKind, ConversationId, Entry, SessionSettings, Status,
    TimedEntry,
};

use super::layout::rings;
use super::{Edge, EdgeKind, FlowModel, Node, Pt, Rect, Step, View, bends, build, layout, route};

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
        suggestions: Vec::new(),
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
    assert_eq!(model.parent[&coder], planner);

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

#[test]
fn bends_are_rounded_within_their_segments() {
    let p = |x, y| Pt { x, y };
    let route = [p(0., 0.), p(100., 0.), p(100., 10.), p(200., 10.)];
    assert_eq!(
        bends(&route, 0.),
        [
            Step::Line(p(100., 0.)),
            Step::Line(p(100., 10.)),
            Step::Line(p(200., 10.))
        ]
    );
    // The 10-long middle segment limits each bend to a radius of 5.
    assert_eq!(
        bends(&route, 16.),
        [
            Step::Line(p(95., 0.)),
            Step::Curve {
                to: p(100., 5.),
                ctrl: p(100., 0.)
            },
            Step::Line(p(100., 5.)),
            Step::Curve {
                to: p(105., 10.),
                ctrl: p(100., 10.)
            },
            Step::Line(p(200., 10.)),
        ]
    );
    assert_eq!(
        bends(&[p(0., 0.), p(50., 0.)], 16.),
        [Step::Line(p(50., 0.))]
    );
}

/// A model from (session, who started it) and (from, to) prompts.
fn model(sessions: &[(i64, Option<i64>)], messages: &[(i64, i64)]) -> FlowModel {
    let node = |id| {
        if id == 0 {
            Node::You
        } else {
            Node::Session(AgentSessionId(id))
        }
    };
    FlowModel {
        nodes: std::iter::once(Node::You)
            .chain(sessions.iter().map(|(id, _)| node(*id)))
            .collect(),
        edges: messages
            .iter()
            .map(|(from, to)| Edge {
                from: node(*from),
                to: node(*to),
                kind: EdgeKind::Prompt,
                items: vec![],
            })
            .collect(),
        parent: sessions
            .iter()
            .map(|(id, by)| (node(*id), by.map_or(Node::You, node)))
            .collect(),
    }
}

type Rings = Vec<(Node, Vec<Node>)>;

#[test]
fn agents_messaging_in_a_loop_make_a_ring() {
    let s = |id| Node::Session(AgentSessionId(id));
    let chain = [(1, None), (2, Some(1)), (3, Some(2))];
    let cases: Vec<(FlowModel, Rings)> = vec![
        // Only who-started-whom messages, and yours: no ring.
        (
            model(&chain, &[(0, 1), (1, 2), (2, 3), (3, 2), (0, 3)]),
            vec![],
        ),
        // planner → coder → reviewer, and the reviewer back to the planner.
        (
            model(&chain, &[(1, 2), (2, 3), (3, 1)]),
            vec![(s(1), vec![s(2), s(3)])],
        ),
        // The planner straight to the reviewer the coder started.
        (model(&chain, &[(1, 3)]), vec![(s(1), vec![s(2), s(3)])]),
        // Two agents the planner started, talking to each other.
        (
            model(&[(1, None), (2, Some(1)), (3, Some(1))], &[(2, 3)]),
            vec![(s(1), vec![s(2), s(3)])],
        ),
    ];
    for (model, expected) in cases {
        assert_eq!(rings(&model), expected, "{:?}", model.edges);
    }
}

#[test]
fn a_ring_is_laid_out_as_a_circle() {
    let s = |id| Node::Session(AgentSessionId(id));
    let centre = |p: Pt| Pt {
        x: p.x + 100.,
        y: p.y + 48.,
    };
    // planner (1) → coder (2) → reviewer (3) → back to planner; the reviewer
    // started a tester (4).
    let m = model(
        &[(1, None), (2, Some(1)), (3, Some(2)), (4, Some(3))],
        &[(1, 2), (2, 3), (3, 1), (3, 4)],
    );
    let pos = layout(&m);
    let (planner, coder, reviewer, tester) = (
        centre(pos[&s(1)]),
        centre(pos[&s(2)]),
        centre(pos[&s(3)]),
        centre(pos[&s(4)]),
    );
    // A triangle: the planner on the left, the coder above, the reviewer
    // below, both to its right.
    assert!(coder.y < planner.y && reviewer.y > planner.y, "{pos:?}");
    assert!(coder.x > planner.x && reviewer.x > planner.x, "{pos:?}");
    assert!((coder.x - reviewer.x).abs() < 1., "{pos:?}");
    // What comes after the ring starts right of the whole circle.
    assert!(tester.x - 100. > coder.x + 100., "{pos:?}");

    // Four in a loop: one above, one opposite, one below.
    let m = model(
        &[(1, None), (2, Some(1)), (3, Some(2)), (4, Some(3))],
        &[(1, 2), (2, 3), (3, 4), (4, 1)],
    );
    let pos = layout(&m);
    let (first, top, far, bottom) = (
        centre(pos[&s(1)]),
        centre(pos[&s(2)]),
        centre(pos[&s(3)]),
        centre(pos[&s(4)]),
    );
    assert!(top.y < first.y && bottom.y > first.y, "{pos:?}");
    assert!((far.y - first.y).abs() < 1. && far.x > top.x, "{pos:?}");
    assert!((top.x - bottom.x).abs() < 1., "{pos:?}");
}
