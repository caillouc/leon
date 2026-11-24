//! A flow's to-dos (TODO-1): added by the user or an agent, marked done by
//! the user, kept in storage.

use std::sync::Arc;

use leon_domain::{ConversationId, FlowId, Todo, TodoId, open_todos, todo_flows};

use crate::app::{Core, blocking};

impl Core {
    /// Reads every to-do (startup).
    pub(crate) fn load_todos(&mut self) {
        let storage = Arc::clone(&self.ports.storage);
        let todos = Self::report(blocking(|| storage.todos())).unwrap_or_default();
        self.state.todos = Arc::new(todos);
    }

    /// Adds a to-do to flow `flow` — when, what — by the agent of
    /// conversation `by` or the user; says what it did, for an agent's tool
    /// call.
    pub(crate) fn add_todo(
        &mut self,
        flow: FlowId,
        (when, what): (&str, &str),
        by: Option<ConversationId>,
    ) -> Result<String, String> {
        let (when, what) = (when.trim(), what.trim());
        if what.is_empty() {
            return Err("`what` is empty: say what the user will do.".to_owned());
        }
        let storage = Arc::clone(&self.ports.storage);
        let now = self.now_ms();
        let todo = blocking(|| storage.add_todo(flow, (when, what), by, now))
            .map_err(|e| e.to_string())?;
        let shown = open_todos(&self.state.todos, Some(flow)) > 0;
        Arc::make_mut(&mut self.state.todos).push(todo);
        self.show_last(flow, shown);
        Ok("Added the to-do: Leon shows it to the user in the flow's Info.".to_owned())
    }

    /// Flow `flow`'s to-dos for an agent: open then done, each with who
    /// added it.
    pub(crate) fn list_todos(&self, flow: FlowId) -> String {
        let (done, open): (Vec<&Todo>, Vec<&Todo>) = self
            .state
            .todos
            .iter()
            .filter(|t| t.flow == flow)
            .partition(|t| t.done);
        if open.is_empty() && done.is_empty() {
            return "This flow has no to-do.".to_owned();
        }
        let line = |t: &&Todo| {
            let by = t.by.map_or_else(
                || "the user".to_owned(),
                |c| self.state.conversation_name(c),
            );
            format!(
                "- [{}] {} (added by {by})",
                if t.done { "x" } else { " " },
                t.line()
            )
        };
        let mut lines = vec!["The flow's to-dos, open then done:".to_owned()];
        lines.extend(open.iter().chain(&done).map(line));
        lines.join("\n")
    }

    /// Changes a to-do's when and what; a what left empty keeps the old one.
    pub(crate) fn edit_todo(&mut self, id: TodoId, (when, what): (&str, &str)) {
        let (when, what) = (when.trim(), what.trim());
        if what.is_empty() {
            return;
        }
        let storage = Arc::clone(&self.ports.storage);
        if Self::report(blocking(|| storage.edit_todo(id, (when, what)))).is_some()
            && let Some(todo) = Arc::make_mut(&mut self.state.todos)
                .iter_mut()
                .find(|t| t.id == id)
        {
            when.clone_into(&mut todo.when);
            what.clone_into(&mut todo.what);
        }
    }

    /// Removes a to-do for good.
    pub(crate) fn remove_todo(&mut self, id: TodoId) {
        let storage = Arc::clone(&self.ports.storage);
        if Self::report(blocking(|| storage.remove_todo(id))).is_some() {
            Arc::make_mut(&mut self.state.todos).retain(|t| t.id != id);
        }
    }

    /// Puts a to-do back as it was before the user ticked or removed it,
    /// in its place: an undo, so the To do page's order stays (TODO-1).
    pub(crate) fn restore_todo(&mut self, todo: Todo) {
        let storage = Arc::clone(&self.ports.storage);
        if Self::report(blocking(|| storage.restore_todo(&todo))).is_some() {
            put_back(Arc::make_mut(&mut self.state.todos), todo);
        }
    }

    /// Marks a to-do done, or open again.
    pub(crate) fn set_todo_done(&mut self, id: TodoId, done: bool) {
        let Some(flow) = self.state.todos.iter().find(|t| t.id == id).map(|t| t.flow) else {
            return;
        };
        let shown = open_todos(&self.state.todos, Some(flow)) > 0;
        let storage = Arc::clone(&self.ports.storage);
        if Self::report(blocking(|| storage.set_todo_done(id, done))).is_some()
            && let Some(todo) = Arc::make_mut(&mut self.state.todos)
                .iter_mut()
                .find(|t| t.id == id)
        {
            todo.done = done;
            if !done {
                self.show_last(flow, shown);
            }
        }
    }

    /// Puts the To do page's flow groups in `order`, the user's (TODO-3).
    pub(crate) fn order_todo_flows(&mut self, order: &[FlowId]) {
        let storage = Arc::clone(&self.ports.storage);
        if Self::report(blocking(|| storage.set_todo_order(order))).is_some() {
            for flow in &mut self.state.flows {
                if let Some(at) = order.iter().position(|id| *id == flow.id) {
                    flow.todo_position = i64::try_from(at).unwrap_or(i64::MAX);
                }
            }
        }
    }

    /// Flow `flow` has an open to-do: unless the To do page `shown` it
    /// already, its group comes last there (TODO-3).
    fn show_last(&mut self, flow: FlowId, shown: bool) {
        if shown {
            return;
        }
        let mut order = todo_flows(&self.state.flows, &self.state.todos);
        order.retain(|f| *f != flow);
        order.push(flow);
        self.order_todo_flows(&order);
    }
}

/// Puts `todo` in `todos`, kept in the order added (by id), in place of the
/// one with its id.
pub(crate) fn put_back(todos: &mut Vec<Todo>, todo: Todo) {
    todos.retain(|t| t.id != todo.id);
    let at = todos.partition_point(|t| t.id < todo.id);
    todos.insert(at, todo);
}
