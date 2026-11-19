//! The flow diagram's geometry (FLOW-4): boxes' sizes and places, and how
//! its arrows run.

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

const NODE: Pt = Pt { x: 250., y: 112. };
const YOU: Pt = Pt { x: 140., y: 56. };

/// A box's width and height.
pub(crate) fn size_of(node: Node) -> Pt {
    match node {
        Node::You => YOU,
        Node::Conversation(_) => NODE,
    }
}

/// "You", then the conversations of each agent, agents in the order they first
/// appear.
pub(crate) fn groups(model: &FlowModel) -> Vec<Vec<Node>> {
    let mut groups: Vec<(Option<&str>, Vec<Node>)> = Vec::new();
    for node in &model.nodes {
        let agent = match node {
            Node::You => None,
            Node::Conversation(_) => Some(model.agent.get(node).map_or("", String::as_str)),
        };
        match groups
            .iter_mut()
            .find(|(a, _)| agent.is_some() && *a == agent)
        {
            Some((_, members)) => members.push(*node),
            None => groups.push((agent, vec![*node])),
        }
    }
    groups.into_iter().map(|(_, members)| members).collect()
}

/// How far the border of a box of `size` is from its middle, in direction
/// (`ux`, `uy`).
pub(crate) fn reach(size: Pt, ux: f32, uy: f32) -> f32 {
    let across = |half: f32, u: f32| {
        if u.abs() < 1e-6 {
            f32::INFINITY
        } else {
            half / u.abs()
        }
    };
    across(size.x / 2., ux).min(across(size.y / 2., uy))
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

pub(crate) fn distance(a: Pt, b: Pt) -> f32 {
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
