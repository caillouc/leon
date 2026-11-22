//! A flow's to-dos (TODO-1).

use super::agents::call;
use super::*;
use crate::ports::AppToolCall;

/// The user and an agent each add one — when, what — the user checks one
/// off and back, an agent reads them, the user removes one.
#[tokio::test(flavor = "multi_thread")]
async fn the_user_and_an_agent_add_todos_the_user_checks_them() {
    let h = harness();
    let planner = new_flow(&h, BackendKind::ClaudeCode).await;
    let flow = until(&h.core, "flow", |s| !s.flows.is_empty()).await.flows[0]
        .flow
        .id;
    // The user may leave the when out.
    h.core.send(Intent::AddTodo {
        flow,
        when: String::new(),
        what: "  Run the migration in prod  ".into(),
    });
    until(&h.core, "the user's to-do", |s| s.todos.len() == 1).await;
    let added = |when: &str, what: &str| AppToolCall::AddTodo {
        when: when.into(),
        what: what.into(),
    };
    let reply = call(&h, planner, added("Once QA is green", "Merge the PR"))
        .await
        .unwrap();
    assert!(reply.starts_with("Added the to-do"), "{reply}");
    // An agent's to-do waits for something: one for now is refused.
    let refused = call(&h, planner, added(" ", "Open the PR"))
        .await
        .unwrap_err();
    assert!(refused.contains("goes in your answer"), "{refused}");
    assert!(call(&h, planner, added("Monday", " ")).await.is_err());
    let s = until(&h.core, "two to-dos", |s| s.todos.len() == 2).await;
    let shown: Vec<_> = s
        .todos
        .iter()
        .map(|t| (t.flow, t.when.as_str(), t.what.as_str(), t.by, t.done))
        .collect();
    assert_eq!(
        shown,
        [
            (flow, "", "Run the migration in prod", None, false),
            (
                flow,
                "Once QA is green",
                "Merge the PR",
                Some(planner),
                false
            ),
        ]
    );
    let first = s.todos[0].id;
    h.core.send(Intent::SetTodoDone(first, true));
    until(&h.core, "done", |s| s.todos[0].done).await;
    h.core.send(Intent::SetTodoDone(first, false));
    let s = until(&h.core, "open again", |s| !s.todos[0].done).await;
    // An agent reads them, open then done, with who added each.
    h.core.send(Intent::SetTodoDone(first, true));
    until(&h.core, "done again", |s| s.todos[0].done).await;
    let name = s.conversations[&planner].name.clone();
    assert_eq!(
        call(&h, planner, AppToolCall::ListTodos).await.unwrap(),
        format!(
            "The flow's to-dos, open then done:\n\
             - [ ] Once QA is green → Merge the PR (added by {name})\n\
             - [x] Run the migration in prod (added by the user)"
        )
    );
    // The user edits one; an empty what keeps the old one.
    let second = s.todos[1].id;
    h.core.send(Intent::EditTodo {
        id: second,
        when: " Once QA is green and the PM agrees ".into(),
        what: "Merge it".into(),
    });
    until(&h.core, "edited", |s| s.todos[1].what == "Merge it").await;
    h.core.send(Intent::EditTodo {
        id: second,
        when: String::new(),
        what: "  ".into(),
    });
    let s = until(&h.core, "still there", |s| s.todos.len() == 2).await;
    assert_eq!(
        (s.todos[1].when.as_str(), s.todos[1].what.as_str()),
        ("Once QA is green and the PM agrees", "Merge it")
    );
    // The user removes one for good.
    h.core.send(Intent::RemoveTodo(first));
    let s = until(&h.core, "removed", |s| s.todos.len() == 1).await;
    assert_eq!(s.todos[0].what, "Merge it");
}

/// The To do page's flow groups keep the order the user gives them; a flow
/// given its first open to-do comes last (TODO-3).
#[tokio::test(flavor = "multi_thread")]
async fn the_to_do_page_keeps_the_users_order_a_new_flow_comes_last() {
    let storage = Arc::new(FakeStorage::default());
    storage
        .add_project("api", Path::new("/code/api"), 0)
        .unwrap();
    let [a, b, c] =
        ["A", "B", "C"].map(|title| storage.create_flow(ProjectId(1), title, 0).unwrap().id);
    let core = start(
        &tokio::runtime::Handle::current(),
        ports(
            Arc::clone(&storage) as Arc<dyn Storage>,
            Arc::new(FakeBackends::default()) as Arc<dyn Backends>,
        ),
        Settings::default(),
    );
    until(&core, "loaded", |s| s.loaded).await;
    let page = |s: &Snapshot| leon_domain::todo_flows(s.flows.iter().map(|f| &f.flow), &s.todos);
    let add = |flow| {
        core.send(Intent::AddTodo {
            flow,
            when: "Monday".into(),
            what: "Merge".into(),
        });
    };
    add(a);
    add(b);
    until(&core, "in the order they came", |s| page(s) == [a, b]).await;
    core.send(Intent::OrderTodoFlows(vec![b, a]));
    until(&core, "the user's order", |s| page(s) == [b, a]).await;
    add(c);
    add(a);
    let s = until(&core, "four to-dos", |s| s.todos.len() == 4).await;
    assert_eq!(page(&s), [b, a, c]);
    // Done, then open again: last.
    let of_b = s.todos.iter().find(|t| t.flow == b).unwrap().id;
    core.send(Intent::SetTodoDone(of_b, true));
    until(&core, "done", |s| page(s) == [a, c]).await;
    core.send(Intent::SetTodoDone(of_b, false));
    until(&core, "open again, last", |s| page(s) == [a, c, b]).await;
    let stored = leon_domain::todo_flows(&storage.list_flows().unwrap(), &storage.todos().unwrap());
    assert_eq!(stored, [a, c, b]);
}
