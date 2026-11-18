use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use leon_core::{ConversationView, Question, TranscriptView};
use leon_domain::{
    BackendKind, Conversation, ConversationId, ConversationSettings, Entry, FlowId, Status,
    TimedEntry,
};

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

/// The forces keep boxes apart and settle the same flow the same way. As in
/// CS Academy's force mode, a box pulled away along its arrow glides back,
/// one dragged to the other side of what it talks to stays there, and a box
/// held by the user stays under the mouse.
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
    let (hub, home) = (pos[&s(1)], pos[&s(4)]);
    let away = |k: f32| Pt {
        x: hub.x + (home.x - hub.x) * k,
        y: hub.y + (home.y - hub.y) * k,
    };
    // Pulled away along its arrow and let go: back near its place.
    let mut pulled = sim.clone();
    pulled.tick(Some((s(4), away(1.4))));
    assert_eq!(pulled.positions()[&s(4)], away(1.4));
    pulled.settle();
    let back = pulled.positions()[&s(4)];
    assert!(
        (back.x - home.x).abs() < 30. && (back.y - home.y).abs() < 30.,
        "{home:?} → {back:?}"
    );
    // Dragged around the planner to its other side: it stays on that side.
    let mut dragged = sim.clone();
    for k in 0..=30 {
        #[allow(clippy::cast_precision_loss, reason = "a test")]
        let (sin, cos) = (std::f32::consts::PI * k as f32 / 30.).sin_cos();
        let (dx, dy) = (home.x - hub.x, home.y - hub.y);
        let at = Pt {
            x: hub.x + dx * cos - dy * sin,
            y: hub.y + dx * sin + dy * cos,
        };
        dragged.tick(Some((s(4), at)));
    }
    dragged.settle();
    let (now, there) = (dragged.positions()[&s(1)], dragged.positions()[&s(4)]);
    let side = (there.x - now.x) * (home.x - hub.x) + (there.y - now.y) * (home.y - hub.y);
    assert!(side < 0., "{hub:?} {home:?} → {now:?} {there:?}");
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

/// As in CS Academy's "force" mode, the boxes a conversation talks to spread
/// all around it, not to one side.
#[test]
fn a_hub_has_its_boxes_all_around_it() {
    let s = |id| Node::Conversation(ConversationId(id));
    let m = model(&[
        (1, "planner", None),
        (2, "coder", Some(1)),
        (3, "coder", Some(1)),
        (4, "reviewer", Some(1)),
        (5, "reviewer", Some(1)),
    ]);
    let pos = super::Sim::new(&m).positions();
    let middle = |n: Node| {
        let r = super::rect(n, pos[&n]);
        (r.x + r.w / 2., r.y + r.h / 2.)
    };
    let hub = middle(s(1));
    let mut angles: Vec<f32> = [Node::You, s(2), s(3), s(4), s(5)]
        .into_iter()
        .map(|n| {
            let (x, y) = middle(n);
            (y - hub.1).atan2(x - hub.0).to_degrees()
        })
        .collect();
    angles.sort_by(f32::total_cmp);
    let widest = angles
        .windows(2)
        .map(|w| w[1] - w[0])
        .chain([360. - angles[angles.len() - 1] + angles[0]])
        .fold(0., f32::max);
    // No half of the space around it is empty (in columns, everything
    // stood on its right).
    assert!(widest < 170., "{angles:?}");
}
