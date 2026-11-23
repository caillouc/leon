//! A holder and Leon's connection to it, with `cat` as the CLI: what Leon
//! sends comes back as the CLI's output. Temporary folders only.

use super::{Holders, Opening, discard, ended, find, turn_ended, turn_started};
use crate::holder::{EXIT, hold};

/// A holder for `cat` in `dir`, run in this test's runtime (Leon starts a
/// separate `leon hold` process; the logic is the same).
fn holding(dir: &std::path::Path) -> tokio::task::JoinHandle<std::io::Result<()>> {
    let dir = dir.to_owned();
    tokio::spawn(async move { hold(&dir, &["cat".to_owned()]).await })
}

#[tokio::test(flavor = "multi_thread")]
async fn lines_go_through_the_holder_and_back() {
    let runs = tempfile::tempdir().unwrap();
    let dir = runs.path().join("7");
    let holder = holding(&dir);
    let (mut input, mut output) = Opening::Held {
        dir: dir.clone(),
        from: 0,
    }
    .connect()
    .await
    .unwrap();
    assert!(input.send(r#"{"n":1}"#).await);
    assert!(input.send(r#"{"n":2}"#).await);
    assert_eq!(output.next_line().await.as_deref(), Some(r#"{"n":1}"#));
    assert_eq!(output.next_line().await.as_deref(), Some(r#"{"n":2}"#));
    // Stopped: the CLI ends, the holder marks it, nothing more to read.
    input.stop().await;
    assert_eq!(output.next_line().await, None);
    holder.await.unwrap().unwrap();
    assert!(dir.join(EXIT).exists());
    let (ok, _) = ended(input, output).await;
    assert!(!ok, "a stopped CLI did not succeed");
}

#[tokio::test(flavor = "multi_thread")]
async fn coming_back_replays_the_turn_that_was_running() {
    let runs = tempfile::tempdir().unwrap();
    let holders = Holders {
        exe: "leon".into(),
        runs: runs.path().to_owned(),
    };
    let dir = holders.dir("7");
    let holder = holding(&dir);
    {
        // Leon, before it quits: one turn done, a second one running.
        let (mut input, mut output) = Opening::Held {
            dir: dir.clone(),
            from: 0,
        }
        .connect()
        .await
        .unwrap();
        turn_started(&dir);
        input.send("first turn").await;
        assert_eq!(output.next_line().await.as_deref(), Some("first turn"));
        turn_ended(&dir);
        turn_started(&dir);
        input.send("second turn").await;
        assert_eq!(output.next_line().await.as_deref(), Some("second turn"));
    }
    // Leon is back: the holder still runs, its turn too.
    let (opening, busy) = find(&holders, "7").expect("the holder is found");
    assert!(busy);
    let (mut input, mut output) = opening.connect().await.unwrap();
    assert_eq!(output.next_line().await.as_deref(), Some("second turn"));
    assert!(input.send("third").await, "the CLI still takes input");
    assert_eq!(output.next_line().await.as_deref(), Some("third"));
    input.stop().await;
    holder.await.unwrap().unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn an_idle_cli_ends_when_leon_goes_away() {
    let runs = tempfile::tempdir().unwrap();
    let holders = Holders {
        exe: "leon".into(),
        runs: runs.path().to_owned(),
    };
    let dir = holders.dir("8");
    let holder = holding(&dir);
    let (mut input, mut output) = Opening::Held {
        dir: dir.clone(),
        from: 0,
    }
    .connect()
    .await
    .unwrap();
    input.send("done").await;
    assert_eq!(output.next_line().await.as_deref(), Some("done"));
    // Leon quits between turns: the holder ends the CLI, then itself.
    drop((input, output));
    holder.await.unwrap().unwrap();
    assert!(dir.join(EXIT).exists());
    // Nothing to come back to: forgotten, its folder removed.
    assert!(find(&holders, "8").is_none());
    assert!(!dir.exists());
}

/// A deleted conversation's holder ends its CLI, even mid-turn, and its
/// folder goes (NAV-8).
#[tokio::test(flavor = "multi_thread")]
async fn a_discarded_conversation_stops_and_leaves_no_folder() {
    let runs = tempfile::tempdir().unwrap();
    let holders = Holders {
        exe: std::path::PathBuf::from("leon"),
        runs: runs.path().to_owned(),
    };
    let dir = holders.dir("7");
    let holder = holding(&dir);
    for _ in 0..200 {
        if dir.join(super::SOCK).exists() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    turn_started(&dir);
    discard(&holders, "7");
    assert!(!dir.exists());
    // It ends (failing to note its end in the folder now gone).
    tokio::time::timeout(std::time::Duration::from_secs(5), holder)
        .await
        .expect("the holder ends")
        .unwrap()
        .ok();
    // Nothing left to discard: nothing happens.
    discard(&holders, "8");
}
