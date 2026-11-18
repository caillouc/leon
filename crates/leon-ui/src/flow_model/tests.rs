use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use leon_core::{Question, SessionView, TranscriptView};
use leon_domain::{
    AgentSession, AgentSessionId, BackendKind, ConversationId, Entry, SessionSettings, Status,
    TimedEntry,
};

use super::{EdgeKind, FlowModel, Node, Pt, Rect, Step, View, bends, build, layout, route};

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
            unread: false,
        },
        status: Status::Idle,
        subagents: (0, 0),
        plan: Vec::new(),
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

    // You, the planner to the right, the coder it started below it.
    let pos = layout(&model);
    assert!(pos[&Node::You].x < pos[&planner].x && pos[&planner].y < pos[&coder].y);
    assert_eq!(model.agent[&coder], "Codex");
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

/// A model from (session, its agent, who started it).
fn model(sessions: &[(i64, &str, Option<i64>)]) -> FlowModel {
    let node = |id| {
        if id == 0 {
            Node::You
        } else {
            Node::Session(AgentSessionId(id))
        }
    };
    FlowModel {
        nodes: std::iter::once(Node::You)
            .chain(sessions.iter().map(|(id, _, _)| node(*id)))
            .collect(),
        edges: vec![],
        parent: sessions
            .iter()
            .map(|(id, _, by)| (node(*id), by.map_or(Node::You, node)))
            .collect(),
        agent: sessions
            .iter()
            .map(|(id, agent, _)| (node(*id), (*agent).to_owned()))
            .collect(),
    }
}

/// The planner, two coders and a reviewer: regions on a 2 × 2 grid that
/// snakes — You, then the planner to its right, the coders under the
/// planner, the reviewer back on the left — each agent's sessions together.
#[test]
fn agents_fill_a_grid_each_in_its_own_region() {
    let s = |id| Node::Session(AgentSessionId(id));
    let m = model(&[
        (1, "planner", None),
        (2, "coder", Some(1)),
        (3, "reviewer", Some(1)),
        (4, "coder", Some(1)),
    ]);
    let pos = layout(&m);
    let centre = |node: Node| {
        let r = super::rect(node, pos[&node]);
        Pt {
            x: r.x + r.w / 2.,
            y: r.y + r.h / 2.,
        }
    };
    let (you, planner, coder, reviewer, coder_2) = (
        centre(Node::You),
        centre(s(1)),
        centre(s(2)),
        centre(s(3)),
        centre(s(4)),
    );
    assert!(
        (you.y - planner.y).abs() < 1. && planner.x > you.x,
        "{pos:?}"
    );
    assert!(
        (coder.x - planner.x).abs() < 1. && coder.y > planner.y,
        "{pos:?}"
    );
    assert!(
        (coder.x - coder_2.x).abs() < 1. && coder_2.y > coder.y,
        "{pos:?}"
    );
    assert!(
        (reviewer.x - you.x).abs() < 1. && reviewer.y > planner.y,
        "{pos:?}"
    );
    // About as tall as wide, not a long line.
    let world = super::bounds(pos.iter().map(|(n, p)| super::rect(*n, *p))).unwrap();
    assert!(world.w < 2. * world.h, "{world:?}");
}

/// Four sessions of one agent stack three high, then start a new column.
#[test]
fn a_busy_agent_stacks_its_sessions() {
    let s = |id| Node::Session(AgentSessionId(id));
    let m = model(&[
        (1, "coder", None),
        (2, "coder", None),
        (3, "coder", None),
        (4, "coder", None),
    ]);
    let pos = layout(&m);
    assert!((pos[&s(1)].x - pos[&s(3)].x).abs() < 1., "{pos:?}");
    assert!(
        pos[&s(3)].y > pos[&s(2)].y && pos[&s(2)].y > pos[&s(1)].y,
        "{pos:?}"
    );
    assert!(pos[&s(4)].x > pos[&s(1)].x, "{pos:?}");
    assert!((pos[&s(4)].y - pos[&s(1)].y).abs() < 1., "{pos:?}");
}
