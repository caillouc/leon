use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use leon_core::{ConversationView, Question, TranscriptView};
use leon_domain::{
    BackendKind, Conversation, ConversationId, ConversationSettings, Entry, FlowId, Status,
    TimedEntry,
};

use super::layout::columns;
use super::{EdgeKind, FlowModel, Node, Pt, Rect, Step, View, bends, build};

fn conversation(id: i64, started_by: Option<i64>) -> ConversationView {
    ConversationView {
        conversation: Conversation {
            id: ConversationId(id),
            flow: FlowId(1),
            started_by: started_by.map(ConversationId),
            settings: ConversationSettings {
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
fn arrows_come_from_what_the_conversations_did() {
    let (planner, coder) = (
        Node::Conversation(ConversationId(1)),
        Node::Conversation(ConversationId(2)),
    );
    let mut asking = conversation(2, Some(1));
    asking.question = Some(Question {
        id: "q".into(),
        title: "Run cargo test?".into(),
        detail: String::new(),
        options: vec![],
    });
    let conversations = [conversation(1, None), asking];
    let agent = |t: &str| Entry::Agent { text: t.into() };
    let end = || Entry::TurnEnd { usage: None };
    let transcripts = HashMap::from([
        (
            ConversationId(1),
            transcript(vec![
                Entry::User {
                    text: "Fix it".into(),
                },
                agent("Here is the plan"),
                end(),
                Entry::FromAgent {
                    from: ConversationId(2),
                    text: "Done".into(),
                    answer: true,
                },
                agent("All good"),
                end(),
            ]),
        ),
        (
            ConversationId(2),
            transcript(vec![
                Entry::FromAgent {
                    from: ConversationId(1),
                    text: "Do step 1".into(),
                    answer: false,
                },
                agent("Done"),
                end(),
            ]),
        ),
    ]);
    let refs: Vec<&ConversationView> = conversations.iter().collect();
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

    // You above; both are the CLI's own agent, so one region: the one
    // started second below the first.
    let pos = columns(&model);
    assert!(pos[&Node::You].y < pos[&planner].y && pos[&planner].y < pos[&coder].y);
    assert!((pos[&planner].x - pos[&coder].x).abs() < 1.);
    assert_eq!(model.agent[&coder], "Codex");
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

/// A model from (conversation, its agent, who started it).
fn model(conversations: &[(i64, &str, Option<i64>)]) -> FlowModel {
    let node = |id| {
        if id == 0 {
            Node::You
        } else {
            Node::Conversation(ConversationId(id))
        }
    };
    FlowModel {
        nodes: std::iter::once(Node::You)
            .chain(conversations.iter().map(|(id, _, _)| node(*id)))
            .collect(),
        edges: vec![],
        parent: conversations
            .iter()
            .map(|(id, _, by)| (node(*id), by.map_or(Node::You, node)))
            .collect(),
        agent: conversations
            .iter()
            .map(|(id, agent, _)| (node(*id), (*agent).to_owned()))
            .collect(),
    }
}

/// The planner, two coders and a reviewer: You above the planner, then the
/// agents it started in the next column, each agent's conversations
/// together.
#[test]
fn columns_follow_who_started_whom_each_agent_in_one_region() {
    let s = |id| Node::Conversation(ConversationId(id));
    let m = model(&[
        (1, "planner", None),
        (2, "coder", Some(1)),
        (3, "reviewer", Some(1)),
        (4, "coder", Some(2)),
    ]);
    let pos = columns(&m);
    // "You" above what it started.
    assert!(pos[&Node::You].y < pos[&s(1)].y, "{pos:?}");
    assert!(
        pos[&Node::You].x > pos[&s(1)].x && pos[&Node::You].x < pos[&s(1)].x + 250.,
        "{pos:?}"
    );
    assert!(pos[&s(1)].x < pos[&s(2)].x, "{pos:?}");
    assert!((pos[&s(2)].x - pos[&s(3)].x).abs() < 1., "{pos:?}");
    // The coder the first coder started stays in the coders' region.
    assert!((pos[&s(2)].x - pos[&s(4)].x).abs() < 1., "{pos:?}");
    assert!(pos[&s(4)].y > pos[&s(2)].y, "{pos:?}");
}

/// Two agents each start a helper: the helpers sit in their starters'
/// order, so the "started" links do not cross.
#[test]
fn columns_are_ordered_so_links_do_not_cross() {
    let s = |id| Node::Conversation(ConversationId(id));
    // The second helper's starter comes first in its column.
    let m = model(&[
        (1, "b", None),
        (2, "a", None),
        (3, "helper-a", Some(2)),
        (4, "helper-b", Some(1)),
    ]);
    let pos = columns(&m);
    let above = |x: i64, y: i64| pos[&s(x)].y < pos[&s(y)].y;
    assert_eq!(above(1, 2), above(4, 3), "{pos:?}");
}

/// The forces keep boxes apart and settle the same flow the same way; a box
/// nudged a little bounces back, held by the user it stays under the mouse.
#[test]
fn boxes_settle_apart_and_bounce_back() {
    let s = |id| Node::Conversation(ConversationId(id));
    let m = model(&[
        (1, "planner", None),
        (2, "coder", Some(1)),
        (3, "coder", Some(1)),
        (4, "reviewer", Some(1)),
        (5, "reviewer", Some(1)),
    ]);
    let sim = super::Sim::new(&m);
    let pos = sim.positions();
    let rects: Vec<Rect> = pos.iter().map(|(n, p)| super::rect(*n, *p)).collect();
    assert!(!super::force::overlapping(&rects), "{pos:?}");
    assert!(!sim.moving());
    assert_eq!(pos, super::Sim::new(&m).positions());
    // Nudged 40 units and let go: back near its place.
    let mut nudged = sim.clone();
    let home = pos[&s(2)];
    let near = Pt {
        x: home.x + 40.,
        y: home.y + 30.,
    };
    nudged.tick(Some((s(2), near)));
    assert_eq!(nudged.positions()[&s(2)], near);
    nudged.settle();
    let back = nudged.positions()[&s(2)];
    assert!(
        (back.x - home.x).abs() < 15. && (back.y - home.y).abs() < 15.,
        "{home:?} → {back:?}"
    );
}

/// An arrow runs straight between the boxes, from edge to edge.
#[test]
fn arrows_run_straight_from_edge_to_edge() {
    let r = |x, y| Rect {
        x,
        y,
        w: 100.,
        h: 50.,
    };
    let line = super::arrows::line(r(0., 0.), r(300., 0.), 0.);
    assert_eq!(line.len(), 2);
    assert!(
        (line[0].x - 103.).abs() < 0.5 && (line[0].y - 25.).abs() < 0.5,
        "{line:?}"
    );
    assert!(
        (line[1].x - 297.).abs() < 0.5 && (line[1].y - 25.).abs() < 0.5,
        "{line:?}"
    );
    // Two arrows of one bundle run side by side.
    let beside = super::arrows::line(r(0., 0.), r(300., 0.), 8.);
    assert!((beside[0].y - 33.).abs() < 0.5, "{beside:?}");
}

/// Four conversations of one agent stack three high, then start a new column.
#[test]
fn a_busy_agent_stacks_its_conversations() {
    let s = |id| Node::Conversation(ConversationId(id));
    let m = model(&[
        (1, "coder", None),
        (2, "coder", None),
        (3, "coder", None),
        (4, "coder", None),
    ]);
    let pos = columns(&m);
    assert!((pos[&s(1)].x - pos[&s(3)].x).abs() < 1., "{pos:?}");
    assert!(
        pos[&s(3)].y > pos[&s(2)].y && pos[&s(2)].y > pos[&s(1)].y,
        "{pos:?}"
    );
    assert!(pos[&s(4)].x > pos[&s(1)].x, "{pos:?}");
    assert!((pos[&s(4)].y - pos[&s(1)].y).abs() < 1., "{pos:?}");
}
