//! The flow diagram's layout (FLOW-4, FLOW-6): the "force" mode of CS
//! Academy's graph editor, with its own rules and strengths
//! (<https://csacademy.com/app/graph_editor>, `CSAForceLayout` in its `bundle.js`
//! v742, read 2026-11-18). Every frame each box moves by the sum of:
//! - along each arrow, `EDGE · (d − IDEAL) / d` toward the other end;
//! - from each box it has no arrow with and is nearer than `IDEAL`,
//!   `CHARGE · (IDEAL − d) / d` away.
//!
//! CS Academy also pulls every node toward the middle of its canvas; Leon
//! leaves that out, so the diagram stays where its boxes are put.
//!
//! No momentum: a box moves by what pulls it, so it glides back to its place
//! when let go; as boxes only push the ones near them, one dragged far
//! enough finds a new place among the others. It is still once no box moves
//! more than `STILL` a frame. CS Academy's nodes are circles of radius 19
//! with arrows 140 long between their middles; boxes here are wide, so `d`
//! is the gap between two borders plus two of those radii, the whole scaled
//! so an arrow is [`LENGTH`] long between borders. Boxes never overlap.

use std::collections::HashMap;

use super::layout::{Pt, groups, reach, size_of};
use super::start::around;
use super::{FlowModel, Node};

/// CS Academy's `edgeStrength`.
const EDGE: f32 = 15.;
/// CS Academy's `chargeStrength`.
const CHARGE: f32 = 10.;
/// CS Academy's `idealEdgeDistance`, also its `repulsionDistance`.
const IDEAL: f32 = 140.;
/// CS Academy's node radius.
const RADIUS: f32 = 19.;
/// CS Academy's `idlePauseThreshold`.
const STILL: f32 = 0.05;
/// How long an arrow is, from border to border.
const LENGTH: f32 = 230.;
/// From CS Academy's units to the diagram's.
const SCALE: f32 = LENGTH / (IDEAL - 2. * RADIUS);
/// Most steps run at once when a flow first shows: it appears settled.
const SETTLE: usize = 3000;
/// Smallest gap between two boxes.
const GAP: f32 = 44.;

/// A flow's boxes, moving under the forces.
#[derive(Debug, Clone)]
pub(crate) struct Sim {
    nodes: Vec<Node>,
    size: Vec<Pt>,
    at: Vec<Pt>,
    /// How far each box moved in the last step, the held one aside.
    step: Vec<Pt>,
    /// Pairs of boxes joined by an arrow.
    springs: Vec<(usize, usize)>,
}

impl Sim {
    /// The flow's boxes, settled.
    pub(crate) fn new(model: &FlowModel) -> Self {
        let mut sim = Self {
            nodes: Vec::new(),
            size: Vec::new(),
            at: Vec::new(),
            step: Vec::new(),
            springs: Vec::new(),
        };
        sim.update(model);
        sim.settle();
        sim
    }

    /// Follows a change of the flow: a new box starts next to the box it
    /// links to, as placed around it, a box gone leaves, the others stay
    /// where they are.
    pub(crate) fn update(&mut self, model: &FlowModel) {
        let nodes = model.nodes.clone();
        let index: HashMap<Node, usize> = nodes.iter().enumerate().map(|(i, n)| (*n, i)).collect();
        let springs = links(model, &index);
        if nodes == self.nodes && springs == self.springs {
            return;
        }
        // Each agent's conversations next to each other around a box.
        let mut agent = vec![0; nodes.len()];
        for (g, group) in groups(model).iter().enumerate() {
            for n in group {
                agent[index[n]] = g;
            }
        }
        let (start, from) = around(&nodes, &springs, &agent, LENGTH);
        let old = |n: &Node| self.nodes.iter().position(|m| m == n);
        let at: Vec<Pt> = (0..nodes.len())
            .map(|i| match (old(&nodes[i]), from[i]) {
                (Some(o), _) => self.at[o],
                (None, Some(p)) => old(&nodes[p]).map_or(start[i], |o| Pt {
                    x: self.at[o].x + start[i].x - start[p].x,
                    y: self.at[o].y + start[i].y - start[p].y,
                }),
                (None, None) => start[i],
            })
            .collect();
        self.size = nodes.iter().map(|n| size_of(*n)).collect();
        self.step = vec![Pt { x: 0., y: 0. }; nodes.len()];
        (self.nodes, self.at, self.springs) = (nodes, at, springs);
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
        self.step.iter().any(|v| v.x.hypot(v.y) > STILL * SCALE)
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
        let before = self.at.clone();
        let force = self.forces();
        for i in 0..self.nodes.len() {
            match held {
                Some((h, at)) if h == i => self.at[i] = at,
                _ => add(&mut self.at[i], force[i].x, force[i].y),
            }
        }
        let fixed: Vec<bool> = (0..self.nodes.len())
            .map(|i| held.is_some_and(|(h, _)| h == i))
            .collect();
        apart(&mut self.at, &self.size, &fixed);
        // What moved, once kept apart: forces held still by another box
        // move nothing.
        self.step = self
            .at
            .iter()
            .zip(&before)
            .zip(&fixed)
            .map(|((now, was), fixed)| {
                if *fixed {
                    Pt { x: 0., y: 0. }
                } else {
                    Pt {
                        x: now.x - was.x,
                        y: now.y - was.y,
                    }
                }
            })
            .collect();
        held.is_some() || self.moving()
    }

    /// How far the forces move each box this step.
    fn forces(&self) -> Vec<Pt> {
        let mut step = vec![Pt { x: 0., y: 0. }; self.nodes.len()];
        for i in 0..self.nodes.len() {
            for j in i + 1..self.nodes.len() {
                let (ux, uy, gap) = self.between(i, j);
                let d = units(gap);
                let pull = if self.springs.contains(&(i, j)) {
                    EDGE * (d - IDEAL) / d
                } else if d < IDEAL {
                    CHARGE * (d - IDEAL) / d
                } else {
                    0.
                } * SCALE;
                add(&mut step[i], ux * pull, uy * pull);
                add(&mut step[j], -ux * pull, -uy * pull);
            }
        }
        step
    }

    /// The direction from box `i` to box `j`, and the length of a straight
    /// line between their borders along it (negative when they overlap).
    fn between(&self, i: usize, j: usize) -> (f32, f32, f32) {
        let (dx, dy) = (self.at[j].x - self.at[i].x, self.at[j].y - self.at[i].y);
        let d = dx.hypot(dy);
        // Two boxes at the same spot: apart along x, the same way each time.
        let (ux, uy) = if d < 0.01 { (1., 0.) } else { (dx / d, dy / d) };
        let gap = d - reach(self.size[i], ux, uy) - reach(self.size[j], ux, uy);
        (ux, uy, gap)
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

/// A gap between two borders, as a distance between the middles of two of
/// CS Academy's nodes; never nearer than one radius.
fn units(gap: f32) -> f32 {
    (gap / SCALE + 2. * RADIUS).max(RADIUS)
}

/// Pairs of boxes joined by arrows or by "started", once each.
fn links(model: &FlowModel, index: &HashMap<Node, usize>) -> Vec<(usize, usize)> {
    let mut parents: Vec<(&Node, &Node)> = model.parent.iter().collect();
    parents.sort_by_key(|(child, _)| index.get(child).copied());
    let mut out: Vec<(usize, usize)> = Vec::new();
    let pairs = parents
        .into_iter()
        .map(|(child, parent)| (*parent, *child))
        .chain(model.edges.iter().map(|e| (e.from, e.to)));
    for (a, b) in pairs {
        let (Some(&i), Some(&j)) = (index.get(&a), index.get(&b)) else {
            continue;
        };
        let pair = (i.min(j), i.max(j));
        if i != j && !out.contains(&pair) {
            out.push(pair);
        }
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

/// No two boxes overlap.
#[cfg(test)]
pub(crate) fn overlapping(rects: &[super::layout::Rect]) -> bool {
    rects.iter().enumerate().any(|(i, a)| {
        rects[i + 1..]
            .iter()
            .any(|b| a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h)
    })
}
