//! Where the flow diagram's boxes sit and how its arrows run (FLOW-4).

use std::collections::HashMap;

use super::{FlowModel, Node};

/// A point or a size in the diagram's own units (before zoom).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Pt {
    pub x: f32,
    pub y: f32,
}

/// A box's place in the diagram.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

const NODE: Pt = Pt { x: 200., y: 96. };
const YOU: Pt = Pt { x: 140., y: 56. };
const COL_GAP: f32 = 150.;
const ROW_GAP: f32 = 64.;
/// How far a ring's middle box sits right of its column.
const BOW: f32 = 90.;

fn size_of(node: Node) -> Pt {
    match node {
        Node::You => YOU,
        Node::Session(_) => NODE,
    }
}

/// Columns by who started whom, boxes centred on each column (the automatic
/// layout). Agents that message each other in a ring sit together in the
/// column after the ring's first agent, bowed out, so the ring reads as a
/// loop rather than a long line.
#[allow(clippy::cast_precision_loss, reason = "a handful of boxes")]
pub(crate) fn layout(model: &FlowModel) -> HashMap<Node, Pt> {
    let rings = rings(model);
    let ring_of = |node: &Node| rings.iter().position(|(_, members)| members.contains(node));
    // Display column of each box, parents before children.
    let mut column_of: HashMap<Node, usize> = HashMap::from([(Node::You, 0)]);
    for node in &model.nodes {
        let starter = match ring_of(node) {
            Some(r) => rings[r].0,
            None => model.parent.get(node).copied().unwrap_or(Node::You),
        };
        if *node != Node::You {
            let c = column_of.get(&starter).copied().unwrap_or(0) + 1;
            column_of.insert(*node, c);
        }
    }
    // Boxes per column, each ring's members together and in ring order.
    let mut columns: Vec<Vec<Node>> = Vec::new();
    let mut placed: Vec<Node> = Vec::new();
    for node in &model.nodes {
        if placed.contains(node) {
            continue;
        }
        let group = ring_of(node).map_or_else(|| vec![*node], |r| rings[r].1.clone());
        for member in group {
            let c = column_of.get(&member).copied().unwrap_or(0);
            if columns.len() <= c {
                columns.resize(c + 1, Vec::new());
            }
            columns[c].push(member);
            placed.push(member);
        }
    }
    let mut pos = HashMap::new();
    for (c, column) in columns.iter().enumerate() {
        let total: f32 = column.iter().map(|n| size_of(*n).y).sum::<f32>()
            + ROW_GAP * column.len().saturating_sub(1) as f32;
        let mut y = -total / 2.;
        for node in column {
            let size = size_of(*node);
            let x = c as f32 * (NODE.x + COL_GAP) + (NODE.x - size.x) / 2. + bow(&rings, *node);
            pos.insert(*node, Pt { x, y });
            y += size.y + ROW_GAP;
        }
    }
    pos
}

/// How far right a ring member sits, so the ring's column bows out like an
/// arc: most in the middle, least at its ends.
#[allow(clippy::cast_precision_loss, reason = "a handful of boxes")]
fn bow(rings: &[(Node, Vec<Node>)], node: Node) -> f32 {
    rings
        .iter()
        .find_map(|(_, members)| {
            let i = members.iter().position(|m| *m == node)?;
            let t = (i + 1) as f32 / (members.len() + 1) as f32;
            Some(BOW * (std::f32::consts::PI * t).sin())
        })
        .unwrap_or(0.)
}

/// Rings of agents: a message between two sessions where neither started
/// the other closes a loop through whoever started them both. Each ring is
/// that first agent and the others in loop order. A session belongs to one
/// ring at most.
pub(crate) fn rings(model: &FlowModel) -> Vec<(Node, Vec<Node>)> {
    let starters = |mut node: Node| {
        let mut chain = vec![node];
        while let Some(p) = model.parent.get(&node) {
            chain.push(*p);
            node = *p;
        }
        chain
    };
    let mut rings: Vec<(Node, Vec<Node>)> = Vec::new();
    for edge in &model.edges {
        let (Node::Session(_), Node::Session(_)) = (edge.from, edge.to) else {
            continue;
        };
        let related = |a, b| model.parent.get(&a) == Some(&b);
        if edge.from == edge.to || related(edge.from, edge.to) || related(edge.to, edge.from) {
            continue;
        }
        let (from_chain, to_chain) = (starters(edge.from), starters(edge.to));
        let Some(root) = from_chain.iter().find(|n| to_chain.contains(n)).copied() else {
            continue;
        };
        // From the root down to a session, root excluded.
        let down = |chain: &[Node]| {
            let mut path: Vec<Node> = chain.iter().take_while(|n| **n != root).copied().collect();
            path.reverse();
            path
        };
        let (to_from, to_to) = (down(&from_chain), down(&to_chain));
        let members: Vec<Node> = if to_from.is_empty() {
            to_to
        } else {
            to_from.into_iter().chain(to_to.into_iter().rev()).collect()
        };
        let taken = |n: &Node| rings.iter().any(|(_, m)| m.contains(n));
        if members.len() >= 2 && !members.iter().any(taken) {
            rings.push((root, members));
        }
    }
    rings
}

pub(crate) fn rect(node: Node, at: Pt) -> Rect {
    let size = size_of(node);
    Rect {
        x: at.x,
        y: at.y,
        w: size.x,
        h: size.y,
    }
}

/// The smallest rectangle holding every box.
pub(crate) fn bounds(rects: impl IntoIterator<Item = Rect>) -> Option<Rect> {
    rects.into_iter().reduce(|a, b| {
        let x = a.x.min(b.x);
        let y = a.y.min(b.y);
        Rect {
            x,
            y,
            w: (a.x + a.w).max(b.x + b.w) - x,
            h: (a.y + a.h).max(b.y + b.h) - y,
        }
    })
}

/// An orthogonal route from box `a` to box `b`, shifted by `off` so that the
/// two directions between the same boxes do not overlap.
pub(crate) fn route(a: Rect, b: Rect, off: f32) -> Vec<Pt> {
    let (acx, acy) = (a.x + a.w / 2., a.y + a.h / 2.);
    let (bcx, bcy) = (b.x + b.w / 2., b.y + b.h / 2.);
    let gap_x = (bcx - acx).abs() - f32::midpoint(a.w, b.w);
    let gap_y = (bcy - acy).abs() - f32::midpoint(a.h, b.h);
    if gap_x >= gap_y {
        let right = bcx > acx;
        let (x1, x2) = if right {
            (a.x + a.w, b.x)
        } else {
            (a.x, b.x + b.w)
        };
        let (y1, y2) = (acy + off, bcy + off);
        if (y1 - y2).abs() < 2. {
            return vec![Pt { x: x1, y: y1 }, Pt { x: x2, y: y1 }];
        }
        let mx = f32::midpoint(x1, x2) + off;
        return vec![
            Pt { x: x1, y: y1 },
            Pt { x: mx, y: y1 },
            Pt { x: mx, y: y2 },
            Pt { x: x2, y: y2 },
        ];
    }
    let down = bcy > acy;
    let (y1, y2) = if down {
        (a.y + a.h, b.y)
    } else {
        (a.y, b.y + b.h)
    };
    let (x1, x2) = (acx + off, bcx + off);
    if (x1 - x2).abs() < 2. {
        return vec![Pt { x: x1, y: y1 }, Pt { x: x1, y: y2 }];
    }
    let my = f32::midpoint(y1, y2) + off;
    vec![
        Pt { x: x1, y: y1 },
        Pt { x: x1, y: my },
        Pt { x: x2, y: my },
        Pt { x: x2, y: y2 },
    ]
}

/// One piece of an arrow after its first point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Step {
    Line(Pt),
    /// A quadratic curve around a bend.
    Curve {
        to: Pt,
        ctrl: Pt,
    },
}

/// The path through `points` with each bend rounded by `radius`, less where
/// a segment is too short; straight bends when `radius` is 0.
pub(crate) fn bends(points: &[Pt], radius: f32) -> Vec<Step> {
    let mut steps = Vec::new();
    for (i, p) in points.iter().enumerate().skip(1) {
        let (Some(before), Some(after)) = (points.get(i - 1), points.get(i + 1)) else {
            steps.push(Step::Line(*p));
            continue;
        };
        let r = radius
            .min(distance(*before, *p) / 2.)
            .min(distance(*p, *after) / 2.);
        if r <= 0. {
            steps.push(Step::Line(*p));
            continue;
        }
        steps.push(Step::Line(toward(*p, *before, r)));
        steps.push(Step::Curve {
            to: toward(*p, *after, r),
            ctrl: *p,
        });
    }
    steps
}

fn distance(a: Pt, b: Pt) -> f32 {
    (b.x - a.x).hypot(b.y - a.y)
}

/// The point `d` away from `from`, toward `to`.
fn toward(from: Pt, to: Pt, d: f32) -> Pt {
    let len = distance(from, to).max(f32::EPSILON);
    Pt {
        x: from.x + (to.x - from.x) * d / len,
        y: from.y + (to.y - from.y) * d / len,
    }
}

/// Where an arrow's label sits: the middle of its first long segment.
pub(crate) fn label_at(points: &[Pt]) -> Pt {
    let (a, b) = match points {
        [a, b] | [_, a, b, ..] => (*a, *b),
        [a] => (*a, *a),
        [] => (Pt { x: 0., y: 0. }, Pt { x: 0., y: 0. }),
    };
    Pt {
        x: f32::midpoint(a.x, b.x),
        y: f32::midpoint(a.y, b.y),
    }
}
