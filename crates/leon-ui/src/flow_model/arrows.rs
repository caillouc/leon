//! The arrows of the flow diagram (FLOW-4): straight lines from box to box,
//! as in a force layout. Every arrow between the same two boxes — the
//! prompts one way, the reports back — runs alongside the others as one
//! bundle under one label, put where it covers no box and no other label.

use std::collections::HashMap;

use super::layout::{Pt, Rect};
use super::{EdgeKind, FlowModel, Node};

/// Room between the arrows of one bundle.
const SPACING: f32 = 8.;
/// Room between an arrow's end and its box.
const CLEAR: f32 = 3.;

/// What to draw, in the diagram's units: each arrow (by its index in the
/// model's edges) with its points, and each bundle's label with its arrows.
pub(crate) struct Drawing {
    pub arrows: Vec<(usize, Vec<Pt>)>,
    pub labels: Vec<(Pt, Vec<usize>)>,
}

/// The arrows between two boxes, those leaving `from` first.
struct Bundle {
    from: Node,
    to: Node,
    edges: Vec<usize>,
}

/// Draws each bundle as parallel straight lines between its boxes, and
/// finds its label a free spot along them.
#[allow(clippy::cast_precision_loss, reason = "a handful of arrows")]
pub(crate) fn draw(model: &FlowModel, rects: &HashMap<Node, Rect>) -> Drawing {
    let mut drawing = Drawing {
        arrows: Vec::new(),
        labels: Vec::new(),
    };
    let mut taken: Vec<Rect> = rects.values().copied().collect();
    for bundle in bundles(model) {
        let (Some(a), Some(b)) = (rects.get(&bundle.from), rects.get(&bundle.to)) else {
            continue;
        };
        let n = bundle.edges.len() as f32;
        for (k, &e) in bundle.edges.iter().enumerate() {
            let shift = (k as f32 - (n - 1.) / 2.) * SPACING;
            let mut points = line(*a, *b, shift);
            if model.edges[e].from != bundle.from {
                points.reverse();
            }
            drawing.arrows.push((e, points));
        }
        let width: f32 = bundle
            .edges
            .iter()
            .map(|e| model.edges[*e].label().len() as f32 * 7. + 18.)
            .sum();
        let spot = label_spot(&line(*a, *b, 0.), width, &taken);
        taken.push(label_rect(spot, width));
        drawing.labels.push((spot, bundle.edges.clone()));
    }
    drawing
}

/// The straight line between the middles of `a` and `b`, moved sideways by
/// `shift`, cut where it leaves `a` and enters `b`.
pub(crate) fn line(a: Rect, b: Rect, shift: f32) -> Vec<Pt> {
    let (ca, cb) = (middle(a), middle(b));
    let (dx, dy) = (cb.x - ca.x, cb.y - ca.y);
    let len = dx.hypot(dy).max(f32::EPSILON);
    let (nx, ny) = (-dy / len * shift, dx / len * shift);
    let (pa, pb) = (
        Pt {
            x: ca.x + nx,
            y: ca.y + ny,
        },
        Pt {
            x: cb.x + nx,
            y: cb.y + ny,
        },
    );
    vec![exit(a, pa, pb), exit(b, pb, pa)]
}

fn middle(r: Rect) -> Pt {
    Pt {
        x: r.x + r.w / 2.,
        y: r.y + r.h / 2.,
    }
}

/// Where the line from `from` (inside `r`) toward `to` crosses `r`'s edge,
/// a little outside it.
fn exit(r: Rect, from: Pt, to: Pt) -> Pt {
    let (dx, dy) = (to.x - from.x, to.y - from.y);
    let c = middle(r);
    let (hw, hh) = (r.w / 2. + CLEAR, r.h / 2. + CLEAR);
    // How far along the line its first box edge is.
    let tx = if dx.abs() > f32::EPSILON {
        ((if dx > 0. { c.x + hw } else { c.x - hw }) - from.x) / dx
    } else {
        f32::MAX
    };
    let ty = if dy.abs() > f32::EPSILON {
        ((if dy > 0. { c.y + hh } else { c.y - hh }) - from.y) / dy
    } else {
        f32::MAX
    };
    let t = tx.min(ty).clamp(0., 1.);
    Pt {
        x: from.x + dx * t,
        y: from.y + dy * t,
    }
}

/// The label's box, centred on `at`.
fn label_rect(at: Pt, width: f32) -> Rect {
    Rect {
        x: at.x - width / 2.,
        y: at.y - 12.,
        w: width,
        h: 24.,
    }
}

/// The first spot along the line, from its middle outward, where a label of
/// `width` covers no box and no label already placed; the middle if none.
fn label_spot(line: &[Pt], width: f32, taken: &[Rect]) -> Pt {
    let (a, b) = (line[0], line[line.len() - 1]);
    let at = |t: f32| Pt {
        x: a.x + (b.x - a.x) * t,
        y: a.y + (b.y - a.y) * t,
    };
    let overlaps = |r: Rect| {
        taken
            .iter()
            .any(|o| r.x < o.x + o.w && o.x < r.x + r.w && r.y < o.y + o.h && o.y < r.y + r.h)
    };
    [0.5, 0.4, 0.6, 0.3, 0.7, 0.22, 0.78]
        .into_iter()
        .map(at)
        .find(|p| !overlaps(label_rect(*p, width)))
        .unwrap_or_else(|| at(0.5))
}

/// The arrows grouped by the two boxes they join, prompts before reports
/// before a question.
fn bundles(model: &FlowModel) -> Vec<Bundle> {
    let rank = |k: EdgeKind| match k {
        EdgeKind::Prompt => 0,
        EdgeKind::Report => 1,
        EdgeKind::Question => 2,
    };
    let mut order: Vec<usize> = (0..model.edges.len()).collect();
    order.sort_by_key(|&i| rank(model.edges[i].kind));
    let mut bundles: Vec<Bundle> = Vec::new();
    for i in order {
        let e = &model.edges[i];
        match bundles
            .iter_mut()
            .find(|b| (b.from, b.to) == (e.from, e.to) || (b.from, b.to) == (e.to, e.from))
        {
            Some(b) => b.edges.push(i),
            None => bundles.push(Bundle {
                from: e.from,
                to: e.to,
                edges: vec![i],
            }),
        }
    }
    bundles
}
