//! What the flow diagram shows (FLOW-4, FLOW-5), built from what the agent
//! conversations actually did: boxes, arrows with their messages, and where each
//! box sits. No rendering here.

use std::collections::HashMap;
use std::sync::Arc;

use leon_core::{ConversationView, TranscriptView};
use leon_domain::{ConversationId, Entry, Status};

/// A box: the user, or an conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Node {
    You,
    Conversation(ConversationId),
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

/// Boxes, arrows, and who started each conversation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FlowModel {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    /// The box that started each conversation's box: "You" or another conversation.
    pub parent: HashMap<Node, Node>,
    /// Each conversation's agent (its CLI's name without one): the layout keeps
    /// an agent's conversations together.
    pub agent: HashMap<Node, String>,
}

/// Builds the flow of one flow from its conversations (in order) and
/// their loaded transcripts.
pub(crate) fn build(
    conversations: &[&ConversationView],
    transcripts: &HashMap<ConversationId, Arc<TranscriptView>>,
) -> FlowModel {
    let mut nodes = vec![Node::You];
    let mut parent = HashMap::new();
    let mut agent = HashMap::new();
    for s in conversations {
        let me = Node::Conversation(s.conversation.id);
        parent.insert(
            me,
            s.conversation
                .started_by
                .map_or(Node::You, Node::Conversation),
        );
        let settings = &s.conversation.settings;
        agent.insert(
            me,
            settings
                .agent
                .clone()
                .unwrap_or_else(|| settings.backend.display_name().to_owned()),
        );
        nodes.push(me);
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
    for s in conversations {
        let me = Node::Conversation(s.conversation.id);
        let entries = transcripts
            .get(&s.conversation.id)
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
                    add(Node::Conversation(*from), me, kind, item(e.at, label, text));
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
        parent,
        agent,
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
pub(crate) fn status_text(s: &ConversationView, waiting_for: Option<&str>) -> String {
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
        // Room above for the totals bar and the "+ New conversation" button,
        // below for the legend and the zoom.
        let (pad_x, top, bottom) = (36., 110., 76.);
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

pub(crate) mod arrows;
pub(crate) mod force;
pub(crate) mod layout;
mod start;
#[cfg(test)]
mod tests;

pub(crate) use arrows::draw;
pub(crate) use force::Sim;
pub(crate) use layout::{Pt, Rect, Step, bends, bounds, rect};
