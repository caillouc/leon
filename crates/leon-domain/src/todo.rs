//! A flow's to-dos (TODO-1): later steps for the user, each waiting for
//! something — when it happens, what to do — not an agent's own plan while
//! it works (CONV-14).

use std::collections::HashSet;

use crate::{ConversationId, Flow, FlowId};

/// Identifier of a to-do in Leon's store.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TodoId(pub i64);

/// A to-do of a flow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Todo {
    /// Its id.
    pub id: TodoId,
    /// The flow it belongs to.
    pub flow: FlowId,
    /// When to do it: what it waits for (a deploy, a date, an answer);
    /// empty when the user left it out.
    pub when: String,
    /// What to do then.
    pub what: String,
    /// Done: the user checked it.
    pub done: bool,
    /// The conversation whose agent added it; `None` when the user did.
    pub by: Option<ConversationId>,
    /// When it was added, in milliseconds since the epoch.
    pub added_at: i64,
}

impl Todo {
    /// On one line, as it reads: "<when> → <what>", or what alone.
    pub fn line(&self) -> String {
        if self.when.trim().is_empty() {
            self.what.clone()
        } else {
            format!("{} → {}", self.when, self.what)
        }
    }
}

/// How many of `todos` are still open, of flow `flow` or of every flow.
pub fn open_todos(todos: &[Todo], flow: Option<FlowId>) -> usize {
    todos
        .iter()
        .filter(|t| !t.done && flow.is_none_or(|f| t.flow == f))
        .count()
}

/// The flows the To do page shows, in its order (TODO-3): those with an
/// open to-do, by the place the user gave their group, then the oldest
/// first.
pub fn todo_flows<'a>(flows: impl IntoIterator<Item = &'a Flow>, todos: &[Todo]) -> Vec<FlowId> {
    let open: HashSet<FlowId> = todos.iter().filter(|t| !t.done).map(|t| t.flow).collect();
    let mut shown: Vec<&Flow> = flows.into_iter().filter(|f| open.contains(&f.id)).collect();
    shown.sort_by_key(|f| (f.todo_position, f.id));
    shown.into_iter().map(|f| f.id).collect()
}

#[cfg(test)]
mod tests {
    use super::{Todo, TodoId, open_todos, todo_flows};
    use crate::{Flow, FlowId, ProjectId};

    #[test]
    fn a_todo_reads_when_then_what() {
        let todo = |when: &str| Todo {
            id: TodoId(1),
            flow: FlowId(1),
            when: when.into(),
            what: "Run the backfill".into(),
            done: false,
            by: None,
            added_at: 0,
        };
        assert_eq!(
            todo("Once the deploy is green").line(),
            "Once the deploy is green → Run the backfill"
        );
        assert_eq!(todo("  ").line(), "Run the backfill");
    }

    #[test]
    fn open_todos_count_per_flow_or_all() {
        let todo = |id, flow, done| Todo {
            id: TodoId(id),
            flow: FlowId(flow),
            when: String::new(),
            what: String::new(),
            done,
            by: None,
            added_at: 0,
        };
        let todos = [todo(1, 1, false), todo(2, 1, true), todo(3, 2, false)];
        let cases = [
            (None, 2),
            (Some(FlowId(1)), 1),
            (Some(FlowId(2)), 1),
            (Some(FlowId(3)), 0),
        ];
        for (flow, expected) in cases {
            assert_eq!(open_todos(&todos, flow), expected, "{flow:?}");
        }
    }

    #[test]
    fn the_to_do_page_shows_flows_with_an_open_to_do_in_their_place() {
        type Case<'a> = (&'a [(i64, i64)], &'a [(i64, bool)], &'a [i64]);
        let flow = |id, todo_position| Flow {
            id: FlowId(id),
            project: ProjectId(1),
            title: String::new(),
            pinned: false,
            archived: false,
            unread: false,
            updated_at: 0,
            folders: Vec::new(),
            links: Vec::new(),
            todo_position,
        };
        let todo = |flow, done| Todo {
            id: TodoId(flow),
            flow: FlowId(flow),
            when: String::new(),
            what: String::new(),
            done,
            by: None,
            added_at: 0,
        };
        // Flows as (id, place), their to-dos as (flow, done), the page's
        // order.
        let cases: [Case; 4] = [
            (
                &[(1, 2), (2, 0), (3, 1)],
                &[(1, false), (2, false), (3, false)],
                &[2, 3, 1],
            ),
            // Same place: the oldest first.
            (&[(2, 0), (1, 0)], &[(1, false), (2, false)], &[1, 2]),
            // Only done ones, or none: not shown.
            (&[(1, 0), (2, 1), (3, 2)], &[(1, true), (3, false)], &[3]),
            (&[(1, 0)], &[], &[]),
        ];
        for (flows, todos, expected) in cases {
            let flows: Vec<Flow> = flows.iter().map(|&(id, at)| flow(id, at)).collect();
            let todos: Vec<Todo> = todos.iter().map(|&(f, done)| todo(f, done)).collect();
            let shown: Vec<i64> = todo_flows(&flows, &todos).iter().map(|f| f.0).collect();
            assert_eq!(shown, expected, "{flows:?}");
        }
    }
}
