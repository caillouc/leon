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

const NODE: Pt = Pt { x: 250., y: 96. };
const YOU: Pt = Pt { x: 140., y: 56. };
/// Space between two regions: room for an arrow and its label.
const GAP: Pt = Pt { x: 170., y: 120. };
/// Space between two boxes of one region.
const INNER: Pt = Pt { x: 40., y: 36. };
/// Most boxes stacked in one column of a region.
const STACK: usize = 3;

fn size_of(node: Node) -> Pt {
    match node {
        Node::You => YOU,
        Node::Conversation(_) => NODE,
    }
}

/// The automatic layout: regions on a grid that fills the space instead of
/// a long line. "You" first, then one region per agent in the order each
/// first appeared; the rows snake (left to right, then right to left), so
/// each region sits next to the one before and a planner → coder →
/// reviewer → planner loop reads as a loop. An agent's conversations share its
/// region, stacked, a new column every `STACK` boxes.
#[allow(clippy::cast_precision_loss, reason = "a handful of boxes")]
pub(crate) fn layout(model: &FlowModel) -> HashMap<Node, Pt> {
    let groups = groups(model);
    let cols = grid_columns(groups.len());
    let rows = groups.len().div_ceil(cols);
    let cell = |ix: usize| {
        let (row, i) = (ix / cols, ix % cols);
        (row, if row % 2 == 1 { cols - 1 - i } else { i })
    };
    let sizes: Vec<Pt> = groups.iter().map(|g| region_size(g)).collect();
    let (mut widths, mut heights) = (vec![0f32; cols], vec![0f32; rows]);
    for (ix, size) in sizes.iter().enumerate() {
        let (row, col) = cell(ix);
        widths[col] = widths[col].max(size.x);
        heights[row] = heights[row].max(size.y);
    }
    let (xs, ys) = (starts(&widths, GAP.x), starts(&heights, GAP.y));
    let mut pos = HashMap::new();
    for (ix, group) in groups.iter().enumerate() {
        let (row, col) = cell(ix);
        let origin = Pt {
            x: xs[col] + (widths[col] - sizes[ix].x) / 2.,
            y: ys[row] + (heights[row] - sizes[ix].y) / 2.,
        };
        for (k, node) in group.iter().enumerate() {
            let size = size_of(*node);
            pos.insert(
                *node,
                Pt {
                    x: origin.x + (k / STACK) as f32 * (size.x + INNER.x),
                    y: origin.y + (k % STACK) as f32 * (size.y + INNER.y),
                },
            );
        }
    }
    pos
}

/// "You", then the conversations of each agent, agents in the order they first
/// appear.
fn groups(model: &FlowModel) -> Vec<Vec<Node>> {
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

/// The fewest grid columns that make the grid about square.
fn grid_columns(regions: usize) -> usize {
    (1..=regions).find(|c| c * c >= regions).unwrap_or(1)
}

/// The size of a region holding `group`'s boxes.
#[allow(clippy::cast_precision_loss, reason = "a handful of boxes")]
fn region_size(group: &[Node]) -> Pt {
    let size = group.first().map_or(NODE, |n| size_of(*n));
    let (cols, rows) = (group.len().div_ceil(STACK), group.len().min(STACK));
    Pt {
        x: cols as f32 * size.x + cols.saturating_sub(1) as f32 * INNER.x,
        y: rows as f32 * size.y + rows.saturating_sub(1) as f32 * INNER.y,
    }
}

/// Where each of `lengths` starts, laid end to end with `gap` between them.
fn starts(lengths: &[f32], gap: f32) -> Vec<f32> {
    let mut at = 0.;
    lengths
        .iter()
        .map(|length| {
            let start = at;
            at += length + gap;
            start
        })
        .collect()
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

/// Where an arrow's label sits: a third of the way along its first long
/// segment, so the labels of the two directions between the same boxes,
/// travelling opposite ways, never sit side by side.
pub(crate) fn label_at(points: &[Pt]) -> Pt {
    const ALONG: f32 = 0.35;
    let (a, b) = match points {
        [a, b] | [_, a, b, ..] => (*a, *b),
        [a] => (*a, *a),
        [] => (Pt { x: 0., y: 0. }, Pt { x: 0., y: 0. }),
    };
    Pt {
        x: a.x + (b.x - a.x) * ALONG,
        y: a.y + (b.y - a.y) * ALONG,
    }
}
