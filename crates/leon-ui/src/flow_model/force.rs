//! The flow diagram's layout (FLOW-4, FLOW-6), after the "force" mode of CS
//! Academy's graph editor, live: boxes push each other away, the arrows
//! between them pull like springs, a light pull keeps everything near the
//! middle, and boxes never overlap. The forces keep acting: a box nudged a
//! little bounces back to its place, one dragged far enough finds a new
//! place among the others. It starts from the columns of who started whom
//! and keeps a pull toward them, so the diagram still reads left to right,
//! and it has no randomness: the same flow always settles the same way.

use std::collections::HashMap;

use super::layout::{Pt, columns, groups, rect};
use super::{FlowModel, Node};

/// Steps run at once when a flow first shows: it appears settled.
const SETTLE: usize = 400;
/// How far apart the middles of two boxes joined by arrows like to be.
const LENGTH: f32 = 380.;
/// The same, for two conversations of one agent: kept close together.
const SAME_AGENT: f32 = 170.;
/// Strength of the arrows' pull, per unit of stretch.
const SPRING: f32 = 0.06;
/// Strength of the push between any two boxes.
const PUSH: f32 = 90_000.;
/// Strength of the pull back toward a box's column.
const COLUMN: f32 = 0.02;
/// Strength of the pull toward the middle of everything.
const GRAVITY: f32 = 0.004;
/// How much of its speed a box keeps from one step to the next: below 1 it
/// settles, near 1 it bounces longer.
const KEEP: f32 = 0.8;
/// Fastest a box moves in one step.
const TOP_SPEED: f32 = 45.;
/// Below this speed everywhere, the flow is still.
const STILL: f32 = 0.05;
/// Smallest gap between two boxes.
const GAP: f32 = 44.;

/// A flow's boxes, moving under the forces.
#[derive(Debug, Clone)]
pub(crate) struct Sim {
    nodes: Vec<Node>,
    size: Vec<Pt>,
    /// Each box's middle in its column: what it is pulled back toward.
    home: Vec<Pt>,
    at: Vec<Pt>,
    speed: Vec<Pt>,
    springs: Vec<(usize, usize, f32)>,
}

impl Sim {
    /// The flow's boxes, settled.
    pub(crate) fn new(model: &FlowModel) -> Self {
        let mut sim = Self::empty();
        sim.update(model);
        sim.settle();
        sim
    }

    fn empty() -> Self {
        Self {
            nodes: Vec::new(),
            size: Vec::new(),
            home: Vec::new(),
            at: Vec::new(),
            speed: Vec::new(),
            springs: Vec::new(),
        }
    }

    /// Follows a change of the flow: a new box starts at its column place,
    /// a box gone leaves, the others stay where they are.
    pub(crate) fn update(&mut self, model: &FlowModel) {
        let start = columns(model);
        let nodes: Vec<Node> = model
            .nodes
            .iter()
            .copied()
            .filter(|n| start.contains_key(n))
            .collect();
        if nodes == self.nodes && springs(model, &nodes) == self.springs {
            return;
        }
        let (mut size, mut home, mut at, mut speed) = (vec![], vec![], vec![], vec![]);
        for n in &nodes {
            let r = rect(*n, start[n]);
            let middle = Pt {
                x: r.x + r.w / 2.,
                y: r.y + r.h / 2.,
            };
            let old = self.nodes.iter().position(|m| m == n);
            size.push(Pt { x: r.w, y: r.h });
            home.push(middle);
            at.push(old.map_or(middle, |i| self.at[i]));
            speed.push(old.map_or(Pt { x: 0., y: 0. }, |i| self.speed[i]));
        }
        self.springs = springs(model, &nodes);
        (self.nodes, self.size, self.home, self.at, self.speed) = (nodes, size, home, at, speed);
    }

    /// Runs the forces until the flow is still.
    pub(crate) fn settle(&mut self) {
        for _ in 0..SETTLE {
            if !self.tick(None) {
                break;
            }
        }
    }

    /// Whether some box still moves.
    pub(crate) fn moving(&self) -> bool {
        self.speed.iter().any(|v| v.x.hypot(v.y) > STILL)
    }

    /// One step of the forces; `held` is a box the user drags, kept under
    /// the mouse. Whether anything still moves.
    pub(crate) fn tick(&mut self, held: Option<(Node, Pt)>) -> bool {
        let held = held.and_then(|(node, top_left)| {
            let i = self.nodes.iter().position(|n| *n == node)?;
            Some((
                i,
                Pt {
                    x: top_left.x + self.size[i].x / 2.,
                    y: top_left.y + self.size[i].y / 2.,
                },
            ))
        });
        let force = self.forces();
        let middle = mean(&self.home);
        for i in 0..self.nodes.len() {
            if let Some((h, at)) = held
                && h == i
            {
                self.at[i] = at;
                self.speed[i] = Pt { x: 0., y: 0. };
                continue;
            }
            let pull = Pt {
                x: force[i].x
                    + COLUMN * (self.home[i].x - self.at[i].x)
                    + GRAVITY * (middle.x - self.at[i].x),
                y: force[i].y + GRAVITY * (middle.y - self.at[i].y),
            };
            let mut v = Pt {
                x: (self.speed[i].x + pull.x) * KEEP,
                y: (self.speed[i].y + pull.y) * KEEP,
            };
            let fast = v.x.hypot(v.y);
            if fast > TOP_SPEED {
                v = Pt {
                    x: v.x / fast * TOP_SPEED,
                    y: v.y / fast * TOP_SPEED,
                };
            }
            self.speed[i] = v;
            self.at[i].x += v.x;
            self.at[i].y += v.y;
        }
        let fixed: Vec<bool> = (0..self.nodes.len())
            .map(|i| held.is_some_and(|(h, _)| h == i))
            .collect();
        apart(&mut self.at, &self.size, &fixed);
        held.is_some() || self.moving()
    }

    /// The pushes and the springs' pulls on every box.
    fn forces(&self) -> Vec<Pt> {
        let mut force = vec![Pt { x: 0., y: 0. }; self.nodes.len()];
        for i in 0..self.nodes.len() {
            for j in i + 1..self.nodes.len() {
                let (dx, dy) = (self.at[i].x - self.at[j].x, self.at[i].y - self.at[j].y);
                let d2 = (dx * dx + dy * dy).max(100.);
                let d = d2.sqrt();
                let f = PUSH / d2;
                add(&mut force[i], dx / d * f, dy / d * f);
                add(&mut force[j], -dx / d * f, -dy / d * f);
            }
        }
        for &(i, j, length) in &self.springs {
            let (dx, dy) = (self.at[j].x - self.at[i].x, self.at[j].y - self.at[i].y);
            let d = dx.hypot(dy).max(1.);
            let f = SPRING * (d - length);
            add(&mut force[i], dx / d * f, dy / d * f);
            add(&mut force[j], -dx / d * f, -dy / d * f);
        }
        force
    }

    /// Each box's top-left corner.
    pub(crate) fn positions(&self) -> HashMap<Node, Pt> {
        self.nodes
            .iter()
            .enumerate()
            .map(|(i, n)| {
                (
                    *n,
                    Pt {
                        x: self.at[i].x - self.size[i].x / 2.,
                        y: self.at[i].y - self.size[i].y / 2.,
                    },
                )
            })
            .collect()
    }
}

/// The springs: one per pair of boxes joined by arrows or by "started", and
/// a short one between conversations of the same agent, which then stay
/// together.
fn springs(model: &FlowModel, nodes: &[Node]) -> Vec<(usize, usize, f32)> {
    let index: HashMap<Node, usize> = nodes.iter().enumerate().map(|(i, n)| (*n, i)).collect();
    let mut out: Vec<(usize, usize, f32)> = Vec::new();
    let mut add = |a: Node, b: Node, length: f32| {
        let (Some(&i), Some(&j)) = (index.get(&a), index.get(&b)) else {
            return;
        };
        let pair = (i.min(j), i.max(j));
        if i != j && !out.iter().any(|(x, y, _)| (*x, *y) == pair) {
            out.push((pair.0, pair.1, length));
        }
    };
    for group in groups(model) {
        for w in group.windows(2) {
            if w[0] != Node::You {
                add(w[0], w[1], SAME_AGENT);
            }
        }
    }
    let mut parents: Vec<(&Node, &Node)> = model.parent.iter().collect();
    parents.sort_by_key(|(child, _)| index.get(child).copied());
    for (child, parent) in parents {
        add(*parent, *child, LENGTH);
    }
    for e in &model.edges {
        add(e.from, e.to, LENGTH);
    }
    out
}

/// Pushes overlapping boxes apart, along the axis where they overlap
/// least, keeping `GAP` between them; a fixed box does not move.
fn apart(at: &mut [Pt], size: &[Pt], fixed: &[bool]) {
    for i in 0..at.len() {
        for j in i + 1..at.len() {
            let (dx, dy) = (at[j].x - at[i].x, at[j].y - at[i].y);
            let over_x = f32::midpoint(size[i].x, size[j].x) + GAP - dx.abs();
            let over_y = f32::midpoint(size[i].y, size[j].y) + GAP - dy.abs();
            if over_x <= 0. || over_y <= 0. {
                continue;
            }
            let (mx, my) = if over_x < over_y {
                (over_x * if dx >= 0. { 1. } else { -1. }, 0.)
            } else {
                (0., over_y * if dy >= 0. { 1. } else { -1. })
            };
            let (share_i, share_j) = match (fixed[i], fixed[j]) {
                (true, true) => (0., 0.),
                (true, false) => (0., 1.),
                (false, true) => (1., 0.),
                (false, false) => (0.5, 0.5),
            };
            at[i].x -= mx * share_i;
            at[i].y -= my * share_i;
            at[j].x += mx * share_j;
            at[j].y += my * share_j;
        }
    }
}

fn add(p: &mut Pt, x: f32, y: f32) {
    p.x += x;
    p.y += y;
}

#[allow(clippy::cast_precision_loss, reason = "a handful of boxes")]
fn mean(points: &[Pt]) -> Pt {
    let n = points.len().max(1) as f32;
    Pt {
        x: points.iter().map(|p| p.x).sum::<f32>() / n,
        y: points.iter().map(|p| p.y).sum::<f32>() / n,
    }
}

/// No two boxes overlap.
#[cfg(test)]
pub(crate) fn overlapping(rects: &[super::layout::Rect]) -> bool {
    rects.iter().enumerate().any(|(i, a)| {
        rects[i + 1..]
            .iter()
            .any(|b| a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h)
    })
}
