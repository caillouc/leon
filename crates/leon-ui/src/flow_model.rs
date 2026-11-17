//! What the flow diagram shows (FLOW-4, FLOW-5), built from what the agent
//! sessions actually did: boxes, arrows with their messages, and where each
//! box sits. No rendering here.

use std::collections::HashMap;
use std::sync::Arc;

use leon_core::{SessionView, TranscriptView};
use leon_domain::{AgentSessionId, Entry, Status};

/// A box: the user, or an agent session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Node {
    You,
    Session(AgentSessionId),
}

/// What an arrow carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum EdgeKind {
    /// Messages asking for work: yours, or requests between agents.
    Prompt,
    /// Answers.
    Report,
    /// A question waiting for you.
    Question,
}

/// One message on an arrow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Item {
    pub at: i64,
    pub label: &'static str,
    pub text: String,
}

/// An arrow: every message of one kind from one box to another.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Edge {
    pub from: Node,
    pub to: Node,
    pub kind: EdgeKind,
    pub items: Vec<Item>,
}

impl Edge {
    /// Short text on the arrow.
    pub(crate) fn label(&self) -> String {
        let n = self.items.len();
        let (one, many) = match (self.kind, self.from, self.to) {
            (EdgeKind::Question, _, _) => return "question".to_owned(),
            (EdgeKind::Prompt, Node::You, _) => ("your message", "messages"),
            (EdgeKind::Prompt, _, _) => ("prompt", "prompts"),
            (EdgeKind::Report, _, Node::You) => ("answer", "answers"),
            (EdgeKind::Report, _, _) => ("report", "reports"),
        };
        if n > 1 {
            format!("{n} {many}")
        } else {
            one.to_owned()
        }
    }
}

/// Boxes, arrows and how deep each box is (who started whom).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FlowModel {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub depth: HashMap<Node, usize>,
}

/// Builds the flow of one conversation from its sessions (in order) and
/// their loaded transcripts.
pub(crate) fn build(
    sessions: &[&SessionView],
    transcripts: &HashMap<AgentSessionId, Arc<TranscriptView>>,
) -> FlowModel {
    let mut nodes = vec![Node::You];
    let mut depth = HashMap::from([(Node::You, 0)]);
    for s in sessions {
        let parent = s.session.started_by.map_or(Node::You, Node::Session);
        let d = depth.get(&parent).copied().unwrap_or(0) + 1;
        depth.insert(Node::Session(s.session.id), d);
        nodes.push(Node::Session(s.session.id));
    }
    let mut edges: Vec<Edge> = Vec::new();
    let mut add = |from: Node, to: Node, kind: EdgeKind, item: Item| match edges
        .iter_mut()
        .find(|e| e.from == from && e.to == to && e.kind == kind)
    {
        Some(edge) => edge.items.push(item),
        None => edges.push(Edge {
            from,
            to,
            kind,
            items: vec![item],
        }),
    };
    for s in sessions {
        let me = Node::Session(s.session.id);
        let entries = transcripts
            .get(&s.session.id)
            .map_or_default(|t| t.entries.as_slice());
        // Who the current turn answers: the user, or the agent that asked.
        let mut asked_by_you = false;
        let mut answer: Option<(i64, &str)> = None;
        for e in entries {
            match &e.entry {
                Entry::User { text } => {
                    asked_by_you = true;
                    answer = None;
                    add(
                        Node::You,
                        me,
                        EdgeKind::Prompt,
                        item(e.at, "Your message", text),
                    );
                }
                Entry::FromAgent {
                    from,
                    text,
                    answer: is_answer,
                } => {
                    asked_by_you = *is_answer;
                    answer = None;
                    let (kind, label) = if *is_answer {
                        (EdgeKind::Report, "Answer")
                    } else {
                        (EdgeKind::Prompt, "Request")
                    };
                    add(Node::Session(*from), me, kind, item(e.at, label, text));
                }
                Entry::Agent { text } => answer = Some((e.at, text)),
                Entry::TurnEnd { .. } => {
                    if let Some((at, text)) = answer.take().filter(|_| asked_by_you) {
                        add(
                            me,
                            Node::You,
                            EdgeKind::Report,
                            item(at, "Answer to you", text),
                        );
                    }
                }
                Entry::Tool { .. } | Entry::Notice { .. } | Entry::Error { .. } => {}
            }
        }
        if let Some(q) = &s.question {
            let at = entries.last().map_or(0, |e| e.at);
            add(
                me,
                Node::You,
                EdgeKind::Question,
                item(at, "Question to you", &q.title),
            );
        }
    }
    FlowModel {
        nodes,
        edges,
        depth,
    }
}

fn item(at: i64, label: &'static str, text: &str) -> Item {
    Item {
        at,
        label,
        text: text.to_owned(),
    }
}

/// Status line of a box.
pub(crate) fn status_text(s: &SessionView, waiting_for: Option<&str>) -> String {
    match s.status {
        Status::Running if s.subagents.1 > 0 => {
            let n = s.subagents.1;
            format!(
                "Working · {n} subagent{} running",
                if n == 1 { "" } else { "s" }
            )
        }
        Status::Running => "Working".to_owned(),
        Status::NeedsInput => "Waiting for your answer".to_owned(),
        Status::Error => "Stopped with an error".to_owned(),
        Status::Idle => {
            waiting_for.map_or_else(|| "Done".to_owned(), |name| format!("Waiting for {name}"))
        }
    }
}

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

pub(crate) const NODE: Pt = Pt { x: 200., y: 96. };
pub(crate) const YOU: Pt = Pt { x: 140., y: 56. };
const COL_GAP: f32 = 124.;
const ROW_GAP: f32 = 46.;

pub(crate) fn size_of(node: Node) -> Pt {
    match node {
        Node::You => YOU,
        Node::Session(_) => NODE,
    }
}

/// Columns by depth, boxes centred on each column (the automatic layout).
#[allow(clippy::cast_precision_loss, reason = "a handful of boxes")]
pub(crate) fn layout(model: &FlowModel) -> HashMap<Node, Pt> {
    let mut columns: Vec<Vec<Node>> = Vec::new();
    for node in &model.nodes {
        let d = model.depth.get(node).copied().unwrap_or(0);
        if columns.len() <= d {
            columns.resize(d + 1, Vec::new());
        }
        columns[d].push(*node);
    }
    let mut pos = HashMap::new();
    for (d, column) in columns.iter().enumerate() {
        let total: f32 = column.iter().map(|n| size_of(*n).y).sum::<f32>()
            + ROW_GAP * column.len().saturating_sub(1) as f32;
        let mut y = -total / 2.;
        for node in column {
            let size = size_of(*node);
            let x = d as f32 * (NODE.x + COL_GAP) + (NODE.x - size.x) / 2.;
            pos.insert(*node, Pt { x, y });
            y += size.y + ROW_GAP;
        }
    }
    pos
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

/// Pan and zoom: screen = world × scale + offset.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct View {
    pub scale: f32,
    pub tx: f32,
    pub ty: f32,
}

pub(crate) const MIN_ZOOM: f32 = 0.25;
pub(crate) const MAX_ZOOM: f32 = 2.5;

impl View {
    /// Everything visible in a `width` × `height` area, centred, not bigger
    /// than 110 %.
    pub(crate) fn fit(world: Rect, width: f32, height: f32) -> Self {
        let (pad_x, top, bottom) = (36., 40., 76.);
        let scale = ((width - pad_x * 2.) / world.w)
            .min((height - top - bottom) / world.h)
            .clamp(MIN_ZOOM, 1.1);
        Self {
            scale,
            tx: (width - world.w * scale) / 2. - world.x * scale,
            ty: top + (height - top - bottom - world.h * scale) / 2. - world.y * scale,
        }
    }

    /// Zooms by `factor` keeping the point under (`mx`, `my`) still.
    pub(crate) fn zoom_at(self, mx: f32, my: f32, factor: f32) -> Self {
        let scale = (self.scale * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        let f = scale / self.scale;
        Self {
            scale,
            tx: mx - (mx - self.tx) * f,
            ty: my - (my - self.ty) * f,
        }
    }

    pub(crate) fn to_screen(self, p: Pt) -> Pt {
        Pt {
            x: p.x * self.scale + self.tx,
            y: p.y * self.scale + self.ty,
        }
    }
}

#[cfg(test)]
mod tests;
