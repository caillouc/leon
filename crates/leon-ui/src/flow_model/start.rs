//! Where the forces start from (FLOW-4): from "You", each box's linked
//! boxes spread evenly all around it, the way back to the box it was placed
//! from kept free, as CS Academy's graph editor shows a star. The same way
//! each time, where CS Academy starts from random places.

use std::collections::VecDeque;
use std::f32::consts::{FRAC_PI_2, TAU};

use super::Node;
use super::layout::{Pt, reach, size_of};

/// The middle of each of `nodes`, `links` being pairs of their indexes,
/// `agent` each one's agent, whose conversations sit next to each other
/// around a box, and `length` the gap along an arrow between two boxes'
/// borders; and from which box each one was placed. Boxes linked to
/// nothing placed yet start a new star on the right.
pub(crate) fn around(
    nodes: &[Node],
    links: &[(usize, usize)],
    agent: &[usize],
    length: f32,
) -> (Vec<Pt>, Vec<Option<usize>>) {
    let near: Vec<Vec<usize>> = (0..nodes.len())
        .map(|i| {
            let mut near: Vec<usize> = links
                .iter()
                .filter_map(|&(a, b)| match (a == i, b == i) {
                    (true, false) => Some(b),
                    (false, true) => Some(a),
                    _ => None,
                })
                .collect();
            near.sort_unstable_by_key(|j| (agent[*j], *j));
            near.dedup();
            near
        })
        .collect();
    let mut at: Vec<Option<Pt>> = vec![None; nodes.len()];
    let mut from: Vec<Option<usize>> = vec![None; nodes.len()];
    for root in 0..nodes.len() {
        if at[root].is_some() {
            continue;
        }
        let right = at
            .iter()
            .zip(nodes)
            .filter_map(|(p, n)| p.map(|p| p.x + size_of(*n).x / 2.))
            .fold(None, |r: Option<f32>, x| Some(r.map_or(x, |r| r.max(x))));
        at[root] = Some(Pt {
            x: right.map_or(0., |r| r + length + size_of(nodes[root]).x / 2.),
            y: 0.,
        });
        let mut queue = VecDeque::from([root]);
        while let Some(i) = queue.pop_front() {
            let Some(here) = at[i] else { continue };
            let todo: Vec<usize> = near[i]
                .iter()
                .copied()
                .filter(|j| at[*j].is_none())
                .collect();
            let back = from[i]
                .and_then(|p| at[p])
                .map(|p| (p.y - here.y).atan2(p.x - here.x));
            let taken = usize::from(back.is_some());
            #[allow(clippy::cast_precision_loss, reason = "a handful of boxes")]
            let slot = TAU / (todo.len() + taken) as f32;
            // The first star hangs below "You".
            let first = back.unwrap_or(FRAC_PI_2);
            for (k, j) in todo.into_iter().enumerate() {
                #[allow(clippy::cast_precision_loss, reason = "a handful of boxes")]
                let angle = first + slot * (k + taken) as f32;
                let (ux, uy) = (angle.cos(), angle.sin());
                let d =
                    reach(size_of(nodes[i]), ux, uy) + length + reach(size_of(nodes[j]), ux, uy);
                at[j] = Some(Pt {
                    x: here.x + ux * d,
                    y: here.y + uy * d,
                });
                from[j] = Some(i);
                queue.push_back(j);
            }
        }
    }
    (
        at.into_iter()
            .map(|p| p.unwrap_or(Pt { x: 0., y: 0. }))
            .collect(),
        from,
    )
}
