//! The folders, pull requests and issues of a flow follow what its
//! agents do (CTX-2).

use super::*;

#[tokio::test(flavor = "multi_thread")]
async fn folders_join_a_flow_where_its_agents_change_files() {
    let h = harness();
    let conversation = new_flow(&h, BackendKind::Codex).await;
    // The repository's own checkout it starts in: no work there yet, not
    // shown.
    let s = h.core.snapshot();
    assert_eq!(s.flows[0].folders, [] as [PathBuf; 0]);
    let id = s.flows[0].flow.id;
    // A worktree, then another repository (relative to the conversation's
    // folder, as some CLIs give paths); a file outside any repository adds
    // nothing; the same worktree again adds it once.
    for paths in [
        vec!["/code/api/.worktrees/fix/src/retry.rs"],
        vec!["../web/src/app.ts", "/tmp/notes.md"],
        vec!["/code/api/.worktrees/fix/src/lib.rs"],
    ] {
        emit(
            &h,
            0,
            BackendEvent::FilesChanged(paths.into_iter().map(PathBuf::from).collect()),
        );
    }
    let s = until(&h.core, "both pull requests", |s| {
        let c = &s.flows[0];
        c.flow.folders.len() == 2 && s.prs_of(c).len() == 2
    })
    .await;
    let c = &s.flows[0];
    // Both kept; the two lookups may end in either order.
    let mut kept = c.flow.folders.clone();
    kept.sort();
    assert_eq!(
        kept,
        [
            PathBuf::from("/code/api/.worktrees/fix"),
            PathBuf::from("/code/web")
        ]
    );
    // Shown: only the worktree its agent works in now; the one it left
    // keeps its pull request.
    assert_eq!(c.folders, [PathBuf::from("/code/api/.worktrees/fix")]);
    // Each folder's branch brings its pull request, and each pull request
    // the issues it closes, after the one the flow started for.
    let prs: Vec<(&str, u64)> = s
        .prs_of(c)
        .iter()
        .map(|p| (p.repo.as_str(), p.number))
        .collect();
    assert_eq!(prs, [("acme/api", 7), ("acme/web", 3)]);
    assert_eq!(
        c.issues,
        [
            leon_domain::GhRef::issue("acme/api", 12),
            leon_domain::GhRef::issue("acme/web", 5)
        ]
    );
    // The branch diff of a folder whose pull request goes to `master` starts
    // there, not at the remote's default branch, as GitHub shows it (GIT-3).
    let s = until(&h.core, "diff since master", |s| {
        s.git
            .get(Path::new("/code/web"))
            .is_some_and(|g| g.base.as_deref() == Some("origin/master"))
    })
    .await;
    let web = &s.git[Path::new("/code/web")];
    assert_eq!(web.branch_diff.len(), 1);
    assert_eq!(
        s.git[Path::new("/code/api/.worktrees/fix")].base.as_deref(),
        Some("origin/main")
    );
    // Kept: the folders are saved with the flow.
    let saved = h.storage.flows.lock().unwrap()[0].folders.clone();
    assert_eq!(saved, c.flow.folders);
    assert_eq!(id, c.flow.id);
    assert_eq!(conversation, c.conversations[0]);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_agent_saying_where_it_works_adds_that_folder() {
    let h = harness();
    let conversation = new_flow(&h, BackendKind::Codex).await;
    let dir = std::env::temp_dir();
    let answer = agents::call(
        &h,
        conversation,
        crate::ports::AppToolCall::SetWorkingDirectory { path: dir.clone() },
    )
    .await;
    assert!(answer.is_ok(), "{answer:?}");
    // Outside a repository too: the agent said so.
    until(&h.core, "the folder", |s| {
        s.flows[0].folders == [dir.clone()]
    })
    .await;
}

/// Several agents in several folders: each conversation knows the folder it
/// works in, which the right pane shows on its tab (CTX-2).
#[tokio::test(flavor = "multi_thread")]
async fn each_agent_has_the_folder_it_works_in() {
    let h = harness();
    let planner = new_flow(&h, BackendKind::Codex).await;
    next_command(&h, 0).await;
    // Before any change, in the repository's own checkout: no folder yet.
    assert_eq!(h.core.snapshot().conversations[&planner].folder, None);
    let start = crate::ports::AppToolCall::StartAgent {
        agent: "coder".into(),
        prompt: "Fix it in the worktree".into(),
        working_directory: None,
        for_user: false,
    };
    agents::call(&h, planner, start).await.unwrap();
    started(&h, 2).await;
    emit(
        &h,
        0,
        BackendEvent::FilesChanged(vec![PathBuf::from("/code/web/src/app.ts")]),
    );
    emit(
        &h,
        1,
        BackendEvent::FilesChanged(vec![PathBuf::from("/code/api/.worktrees/fix/src/retry.rs")]),
    );
    let coder = ConversationId(2);
    let s = until(&h.core, "each in its folder", |s| {
        // The coder may not be in the snapshot yet.
        let folder = |id| s.conversations.get(&id).and_then(|c| c.folder.clone());
        folder(coder).as_deref() == Some(Path::new("/code/api/.worktrees/fix"))
            && folder(planner).as_deref() == Some(Path::new("/code/web"))
    })
    .await;
    // Both shown, each with its agent, in the order Leon saw them.
    let mut shown = s.flows[0].folders.clone();
    shown.sort();
    assert_eq!(
        shown,
        [
            PathBuf::from("/code/api/.worktrees/fix"),
            PathBuf::from("/code/web")
        ]
    );
    // Saved: it survives a restart (I20).
    let saved = h.storage.conversations.lock().unwrap().clone();
    let folder = |id: ConversationId| {
        saved
            .iter()
            .find(|c| c.id == id)
            .and_then(|c| c.folder.clone())
    };
    assert_eq!(
        folder(coder),
        Some(PathBuf::from("/code/api/.worktrees/fix"))
    );
}

/// A repository's own checkout shows once an agent works in it — changes a
/// file there or says it works there — on any branch, its default one too;
/// an agent merely started there adds nothing (CTX-2).
#[tokio::test(flavor = "multi_thread")]
async fn a_main_checkout_shows_once_an_agent_works_in_it() {
    let h = harness();
    let planner = new_flow(&h, BackendKind::Codex).await;
    next_command(&h, 0).await;
    // A repository's own checkout on disk: start_agent wants a folder.
    let (docs, web) = (PathBuf::from("/code/docs"), std::env::temp_dir());
    // A coder started in the web checkout, by name: nothing joins.
    let start = crate::ports::AppToolCall::StartAgent {
        agent: "coder".into(),
        prompt: "Look at the web app".into(),
        working_directory: Some(web.clone()),
        for_user: false,
    };
    agents::call(&h, planner, start).await.unwrap();
    started(&h, 2).await;
    // The planner edits the docs checkout, on its default branch.
    emit(
        &h,
        0,
        BackendEvent::FilesChanged(vec![docs.join("README.md")]),
    );
    let s = until(&h.core, "the docs checkout", |s| {
        s.flows[0].folders == [docs.clone()]
    })
    .await;
    assert_eq!(s.conversations[&planner].folder, Some(docs.clone()));
    assert_eq!(h.storage.flows.lock().unwrap()[0].folders, [docs]);
    // Its pull request is none of the flow's: the web checkout's branch has
    // one, not followed.
    assert_eq!(s.prs_of(&s.flows[0]).len(), 0);
    // The coder says it works in the web checkout: now it shows.
    let said = agents::call(
        &h,
        ConversationId(2),
        crate::ports::AppToolCall::SetWorkingDirectory { path: web.clone() },
    )
    .await;
    assert!(said.is_ok(), "{said:?}");
    let s = until(&h.core, "the web checkout", |s| {
        s.flows[0].folders.contains(&web)
    })
    .await;
    assert_eq!(s.conversations[&ConversationId(2)].folder, Some(web));
}

#[tokio::test(flavor = "multi_thread")]
async fn every_conversation_is_told_to_keep_leon_up_to_date() {
    let h = harness();
    new_flow(&h, BackendKind::Codex).await;
    let spec = h.backends.started.lock().unwrap()[0].spec.clone();
    assert_eq!(spec.app_instructions, Some("Keep Leon up to date."));
}
