//! The arrows of the flow diagram (FLOW-4): straight lines from box to box,
//! as in a force layout. Every arrow between the same two boxes — the
//! prompts one way, the reports back — runs alongside the others as one
//! bundle under one label, put where it covers no box and no other label.
//! A box sitting on a bundle pushes it into a curve that goes round it, on
//! the other side, as CS Academy's graph editor bends an edge a node sits
//! on (a quadratic curve, its middle moved sideways; `getPathString` and
//! `getEdgeBendingTransitions` in its `bundle.js` v742, read 2026-11-18).

use std::collections::HashMap;

use super::layout::{Pt, Rect, distance};
use super::{EdgeKind, FlowModel, Node};

/// Room between the arrows of one bundle.
const SPACING: f32 = 8.;
/// Room between an arrow's end and its box.
const CLEAR: f32 = 3.;
/// Room a curve leaves round a box it goes round.
const ROUND: f32 = 24.;
/// Points drawn along a curve.
const SAMPLES: usize = 20;

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
        let others: Vec<Rect> = rects
            .iter()
            .filter(|(node, _)| **node != bundle.from && **node != bundle.to)
            .map(|(_, r)| *r)
            .collect();
        let bend = bulge(*a, *b, &others);
        for (k, &e) in bundle.edges.iter().enumerate() {
            let shift = (k as f32 - (n - 1.) / 2.) * SPACING;
            let mut points = path(*a, *b, shift, bend);
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
        let spot = label_spot(&path(*a, *b, 0., bend), width, &taken);
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

/// How far sideways a box sitting on the line between `a` and `b` pushes
/// its middle: toward the left of the line when positive, as in [`path`];
/// 0 when no box of `others` touches it. The box pushing most wins.
pub(crate) fn bulge(a: Rect, b: Rect, others: &[Rect]) -> f32 {
    let (ca, cb) = (middle(a), middle(b));
    let (dx, dy) = (cb.x - ca.x, cb.y - ca.y);
    let len = dx.hypot(dy);
    if len < 1. {
        return 0.;
    }
    let (ux, uy) = (dx / len, dy / len);
    let (nx, ny) = (-uy, ux);
    let mut bend: f32 = 0.;
    for r in others {
        let c = middle(*r);
        let (rx, ry) = (c.x - ca.x, c.y - ca.y);
        // Where along the line it sits, and on which side.
        let t = (rx * ux + ry * uy) / len;
        let side = rx * nx + ry * ny;
        let across = r.w / 2. * nx.abs() + r.h / 2. * ny.abs();
        if !(0.1..=0.9).contains(&t) || side.abs() >= across + ROUND {
            continue;
        }
        // Past the box, on the side its middle is not on.
        let offset = if side > 0. {
            side - across - ROUND
        } else {
            side + across + ROUND
        };
        // A quadratic curve is moved 2t(1 − t) of its middle's move at t.
        let push = (offset / (2. * t * (1. - t))).clamp(-len, len);
        if push.abs() > bend.abs() {
            bend = push;
        }
    }
    bend
}

/// The arrow from `a` to `b`, moved sideways by `shift`: the straight
/// [`line`] when `bend` is 0, else a curve whose middle is moved `bend`
/// toward the left of the line, cut where it leaves `a` and enters `b`.
#[allow(clippy::cast_precision_loss, reason = "a few points")]
pub(crate) fn path(a: Rect, b: Rect, shift: f32, bend: f32) -> Vec<Pt> {
    if bend == 0. {
        return line(a, b, shift);
    }
    let (ca, cb) = (middle(a), middle(b));
    let (dx, dy) = (cb.x - ca.x, cb.y - ca.y);
    let len = dx.hypot(dy).max(f32::EPSILON);
    let (nx, ny) = (-dy / len, dx / len);
    let side = |p: Pt, by: f32| Pt {
        x: p.x + nx * by,
        y: p.y + ny * by,
    };
    let (pa, pb) = (side(ca, shift), side(cb, shift));
    let ctrl = side(
        Pt {
            x: f32::midpoint(ca.x, cb.x),
            y: f32::midpoint(ca.y, cb.y),
        },
        shift + bend,
    );
    let (start, end) = (exit(a, pa, ctrl), exit(b, pb, ctrl));
    (0..=SAMPLES)
        .map(|k| {
            let along = k as f32 / SAMPLES as f32;
            // The quadratic curve's weights for its start, middle and end.
            let (first, mid, last) = (
                (1. - along) * (1. - along),
                2. * along * (1. - along),
                along * along,
            );
            Pt {
                x: first * start.x + mid * ctrl.x + last * end.x,
                y: first * start.y + mid * ctrl.y + last * end.y,
            }
        })
        .collect()
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
    let total: f32 = line.windows(2).map(|p| distance(p[0], p[1])).sum();
    // The point `t` of the way along the line.
    let at = |t: f32| {
        let mut left = total * t;
        for p in line.windows(2) {
            let step = distance(p[0], p[1]);
            if step > 0. && left <= step {
                let k = left / step;
                return Pt {
                    x: p[0].x + (p[1].x - p[0].x) * k,
                    y: p[0].y + (p[1].y - p[0].y) * k,
                };
            }
            left -= step;
        }
        line[line.len() - 1]
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
