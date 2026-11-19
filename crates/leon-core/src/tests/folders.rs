//! The folders, pull requests and issues of a flow follow what its
//! agents do (CTX-2).

use super::*;

#[tokio::test(flavor = "multi_thread")]
async fn folders_join_a_flow_where_its_agents_change_files() {
    let h = harness();
    let conversation = new_flow(&h, BackendKind::Codex).await;
    let s = until(&h.core, "the first folder", |s| {
        s.flows[0].folders == [PathBuf::from("/code/api")]
    })
    .await;
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
        c.folders.len() == 2 && s.prs_of(c).len() == 2
    })
    .await;
    let c = &s.flows[0];
    assert_eq!(
        c.folders,
        [
            PathBuf::from("/code/api/.worktrees/fix"),
            PathBuf::from("/code/web")
        ]
    );
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
    assert_eq!(saved, c.folders);
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
    // Before any change: the flow folder holding its working directory.
    until(&h.core, "the planner's folder", |s| {
        s.conversations[&planner].folder.as_deref() == Some(Path::new("/code/api"))
    })
    .await;
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
        s.conversations[&coder].folder.as_deref() == Some(Path::new("/code/api/.worktrees/fix"))
            && s.conversations[&planner].folder.as_deref() == Some(Path::new("/code/web"))
    })
    .await;
    assert_eq!(
        s.flows[0].folders,
        [
            PathBuf::from("/code/web"),
            PathBuf::from("/code/api/.worktrees/fix")
        ]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn every_conversation_is_told_to_keep_leon_up_to_date() {
    let h = harness();
    new_flow(&h, BackendKind::Codex).await;
    let spec = h.backends.started.lock().unwrap()[0].spec.clone();
    assert_eq!(spec.app_instructions, Some("Keep Leon up to date."));
}
