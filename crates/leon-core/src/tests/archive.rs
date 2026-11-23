//! The archive and deletion (NAV-8): a flow archived over a month ago is
//! deleted; a deleted flow leaves nothing behind.

use super::*;

/// At launch, a flow in the archive for more than a month is deleted, its
/// to-dos with it; a newer one stays. Archiving notes when, from the clock.
#[tokio::test(flavor = "multi_thread")]
async fn a_flow_archived_over_a_month_ago_is_deleted() {
    let storage = Arc::new(FakeStorage::default());
    storage
        .add_project("api", Path::new("/code/api"), 0)
        .unwrap();
    // The test clock reads 1 000.
    let day = 24 * 60 * 60 * 1000;
    let [old, recent, live] =
        ["Old", "Recent", "Live"].map(|title| storage.create_flow(ProjectId(1), title, 0).unwrap());
    for (mut flow, archived) in [
        (old.clone(), Some(1_000 - 31 * day)),
        (recent, Some(1_000 - 29 * day)),
    ] {
        flow.archived = archived;
        storage.update_flow(&flow).unwrap();
    }
    storage
        .add_todo(old.id, ("Monday", "Merge"), None, 0)
        .unwrap();
    let core = start(
        &tokio::runtime::Handle::current(),
        ports(
            Arc::clone(&storage) as Arc<dyn Storage>,
            Arc::new(FakeBackends::default()) as Arc<dyn Backends>,
        ),
        Settings::default(),
    );
    let s = until(&core, "loaded", |s| s.loaded).await;
    let mut titles: Vec<&str> = s.flows.iter().map(|f| f.flow.title.as_str()).collect();
    titles.sort_unstable();
    assert_eq!(titles, ["Live", "Recent"]);
    assert!(s.todos.is_empty());
    assert_eq!(storage.list_flows().unwrap().len(), 2);
    let archived_of = |s: &Snapshot| s.flow(live.id).and_then(|f| f.flow.archived);
    core.send(Intent::Archive(live.id, true));
    until(&core, "archived now", |s| archived_of(s) == Some(1_000)).await;
    core.send(Intent::Archive(live.id, false));
    until(&core, "back", |s| {
        s.flow(live.id).is_some_and(|f| f.flow.archived.is_none())
    })
    .await;
}

/// Deleting a flow removes what its conversation leaves (its CLI, Leon's
/// log, its CLI session); at launch, the run folder of a conversation
/// deleted meanwhile goes too.
#[tokio::test(flavor = "multi_thread")]
async fn a_deleted_flow_leaves_nothing_behind() {
    let h = harness();
    let id = new_flow(&h, BackendKind::ClaudeCode).await;
    emit(
        &h,
        0,
        BackendEvent::Ready {
            cli_session: "session-1".into(),
        },
    );
    let s = until(&h.core, "its session", |s| {
        s.conversations[&id].conversation.cli_session.is_some()
    })
    .await;
    h.core.send(Intent::Delete(s.flows[0].flow.id));
    let discarded = || h.backends.discarded.lock().unwrap().clone();
    for _ in 0..200 {
        if !discarded().is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(
        discarded(),
        [(id, Some((BackendKind::ClaudeCode, "session-1".to_owned())))]
    );

    let backends = Arc::new(FakeBackends::default());
    backends
        .held
        .lock()
        .unwrap()
        .push((ConversationId(99), false));
    let core = start(
        &tokio::runtime::Handle::current(),
        ports(
            Arc::new(FakeStorage::default()) as Arc<dyn Storage>,
            Arc::clone(&backends) as Arc<dyn Backends>,
        ),
        Settings::default(),
    );
    until(&core, "loaded", |s| s.loaded).await;
    for _ in 0..200 {
        if !backends.discarded.lock().unwrap().is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(
        *backends.discarded.lock().unwrap(),
        [(ConversationId(99), None)]
    );
}
