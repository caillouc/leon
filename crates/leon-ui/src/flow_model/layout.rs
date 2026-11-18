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
/// Space between two regions: room for arrows and their labels.
const GAP: Pt = Pt { x: 190., y: 90. };
/// Space between two boxes of one region: room for an arrow's label.
const INNER: Pt = Pt { x: 40., y: 64. };
/// Most boxes stacked in one column of a region.
const STACK: usize = 3;

fn size_of(node: Node) -> Pt {
    match node {
        Node::You => YOU,
        Node::Conversation(_) => NODE,
    }
}

/// Where the forces start from (FLOW-4): columns by who started whom — what you
/// started, then what those started — with one region per agent, its
/// conversations stacked together in the column of its first one, and "You"
/// above the first column. Within each column the regions are ordered so
/// that arrows cross as little as possible.
pub(crate) fn columns(model: &FlowModel) -> HashMap<Node, Pt> {
    let groups = groups(model);
    let region_of: HashMap<Node, usize> = groups
        .iter()
        .enumerate()
        .flat_map(|(r, g)| g.iter().map(move |n| (*n, r)))
        .collect();
    let depth = depths(model);
    let layer: Vec<usize> = groups
        .iter()
        .map(|g| {
            g.iter()
                .filter_map(|n| depth.get(n))
                .min()
                .copied()
                .unwrap_or(0)
        })
        .collect();
    // Columns in layer order, empty layers dropped; "You" (region 0) apart.
    let mut layers: Vec<usize> = layer[1..].to_vec();
    layers.sort_unstable();
    layers.dedup();
    let mut columns: Vec<Vec<usize>> = layers
        .iter()
        .map(|l| (1..groups.len()).filter(|r| layer[*r] == *l).collect())
        .collect();
    let links = links(model, &region_of);
    untangle(&mut columns, &links);
    place(&groups, &columns)
}

/// How many "started" steps each box is from "You".
fn depths(model: &FlowModel) -> HashMap<Node, usize> {
    let mut depth = HashMap::from([(Node::You, 0)]);
    // Parents come before the conversations they start.
    for node in &model.nodes {
        if *node != Node::You {
            let parent = model.parent.get(node).copied().unwrap_or(Node::You);
            let d = depth.get(&parent).copied().unwrap_or(0) + 1;
            depth.insert(*node, d);
        }
    }
    depth
}

/// Pairs of regions joined by an arrow or a "started" link, once per link.
fn links(model: &FlowModel, region_of: &HashMap<Node, usize>) -> Vec<(usize, usize)> {
    let started = model.parent.iter().map(|(child, parent)| (*parent, *child));
    let arrows = model.edges.iter().map(|e| (e.from, e.to));
    started
        .chain(arrows)
        .filter_map(|(a, b)| Some((*region_of.get(&a)?, *region_of.get(&b)?)))
        .filter(|(a, b)| a != b)
        .collect()
}

/// Orders the regions of each column by the mean place of what they link
/// to (barycentre sweeps, both ways), keeping the order with the fewest
/// crossings between neighbouring columns.
#[allow(clippy::cast_precision_loss, reason = "a handful of boxes")]
fn untangle(columns: &mut [Vec<usize>], links: &[(usize, usize)]) {
    let place_of = |columns: &[Vec<usize>]| {
        // "You" (region 0) sits above the first column: what you talk to
        // is pulled up, next to it.
        let mut at: HashMap<usize, (usize, f32)> = HashMap::from([(0, (0, -0.5))]);
        for (c, column) in columns.iter().enumerate() {
            for (i, r) in column.iter().enumerate() {
                at.insert(*r, (c, (i as f32 + 0.5) / column.len() as f32));
            }
        }
        at
    };
    let mut best = (crossings(columns, links), columns.to_vec());
    for _ in 0..6 {
        for forward in [true, false] {
            let order: Vec<usize> = if forward {
                (1..columns.len()).collect()
            } else {
                (0..columns.len().saturating_sub(1)).rev().collect()
            };
            for c in order {
                let at = place_of(columns);
                let key = |r: usize| {
                    let near: Vec<f32> = links
                        .iter()
                        .filter_map(|(a, b)| match (*a == r, *b == r) {
                            (true, _) => Some(*b),
                            (_, true) => Some(*a),
                            _ => None,
                        })
                        .filter_map(|n| at.get(&n))
                        .filter(|(nc, _)| if forward { *nc < c } else { *nc > c } || (*nc == 0 && c > 0))
                        .map(|(_, p)| *p)
                        .collect();
                    if near.is_empty() {
                        at.get(&r).map_or(0., |(_, p)| *p)
                    } else {
                        near.iter().sum::<f32>() / near.len() as f32
                    }
                };
                columns[c].sort_by(|a, b| key(*a).total_cmp(&key(*b)));
            }
            let now = crossings(columns, links);
            if now < best.0 {
                best = (now, columns.to_vec());
            }
        }
    }
    columns.clone_from_slice(&best.1);
}

/// Crossings between the links of neighbouring columns, "You" (region 0)
/// counted at the top of the first column.
fn crossings(columns: &[Vec<usize>], links: &[(usize, usize)]) -> usize {
    let mut at: HashMap<usize, (usize, usize)> = HashMap::from([(0, (0, 0))]);
    for (c, column) in columns.iter().enumerate() {
        for (i, r) in column.iter().enumerate() {
            at.insert(*r, (c, i + usize::from(c == 0)));
        }
    }
    let spans: Vec<(usize, usize, usize)> = links
        .iter()
        .filter_map(|(a, b)| {
            let ((ca, ia), (cb, ib)) = (*at.get(a)?, *at.get(b)?);
            match ca.abs_diff(cb) {
                1 if ca < cb => Some((ca, ia, ib)),
                1 => Some((cb, ib, ia)),
                _ => None,
            }
        })
        .collect();
    let mut count = 0;
    for (i, (c1, a1, b1)) in spans.iter().enumerate() {
        for (c2, a2, b2) in &spans[i + 1..] {
            if c1 == c2 && ((a1 < a2 && b1 > b2) || (a1 > a2 && b1 < b2)) {
                count += 1;
            }
        }
    }
    count
}

/// Where each box goes: columns left to right, each column's regions top
/// to bottom and centred on the same line, "You" above the first column.
#[allow(clippy::cast_precision_loss, reason = "a handful of boxes")]
fn place(groups: &[Vec<Node>], columns: &[Vec<usize>]) -> HashMap<Node, Pt> {
    let mut pos = HashMap::from([(Node::You, Pt { x: 0., y: 0. })]);
    let sizes: Vec<Pt> = groups.iter().map(|g| region_size(g)).collect();
    let widths: Vec<f32> = columns
        .iter()
        .map(|c| c.iter().map(|r| sizes[*r].x).fold(0., f32::max))
        .collect();
    let xs = starts(&widths, GAP.x);
    for (c, column) in columns.iter().enumerate() {
        let heights: Vec<f32> = column.iter().map(|r| sizes[*r].y).collect();
        let total = heights.iter().sum::<f32>() + GAP.y * heights.len().saturating_sub(1) as f32;
        if c == 0 {
            pos.insert(
                Node::You,
                Pt {
                    x: (widths[0] - YOU.x) / 2.,
                    y: -total / 2. - GAP.y - YOU.y,
                },
            );
        }
        let ys = starts(&heights, GAP.y);
        for (r, y) in column.iter().zip(ys) {
            let origin = Pt {
                x: xs[c] + (widths[c] - sizes[*r].x) / 2.,
                y: y - total / 2.,
            };
            for (k, node) in groups[*r].iter().enumerate() {
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
    }
    pos
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
