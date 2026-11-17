use std::sync::Arc;

use leon_domain::{Project, Settings};
use tokio::runtime::Handle;
use tokio::sync::watch;

use crate::ports::ProjectRepository;

/// What the UI renders: an immutable copy of the application state.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Snapshot {
    /// `false` until the first load from the store finished.
    pub loaded: bool,
    /// Projects, sorted by name (NAV-2).
    pub projects: Vec<Project>,
    /// The user's settings.
    pub settings: Settings,
}

/// The UI's handle on the core: read the latest snapshot, wait for the next.
///
/// The channel is runtime-agnostic, so the UI can await it on its own
/// executor (P8).
#[derive(Debug, Clone)]
pub struct CoreHandle {
    snapshots: watch::Receiver<Arc<Snapshot>>,
}

impl CoreHandle {
    /// The latest snapshot.
    pub fn snapshot(&self) -> Arc<Snapshot> {
        Arc::clone(&self.snapshots.borrow())
    }

    /// A receiver that wakes up each time a new snapshot is published.
    pub fn subscribe(&self) -> watch::Receiver<Arc<Snapshot>> {
        self.snapshots.clone()
    }
}

/// Starts the core on `runtime` and returns the UI's handle.
///
/// Loads the projects off the async workers; a failure is logged and the app
/// keeps running with an empty list.
pub fn start(
    runtime: &Handle,
    projects: Arc<dyn ProjectRepository>,
    settings: Settings,
) -> CoreHandle {
    let initial = Snapshot {
        loaded: false,
        projects: Vec::new(),
        settings,
    };
    let (publish, snapshots) = watch::channel(Arc::new(initial));
    runtime.spawn(async move {
        let loaded = tokio::task::spawn_blocking(move || projects.list_projects()).await;
        let list = match loaded {
            Ok(Ok(list)) => list,
            Ok(Err(error)) => {
                tracing::error!(%error, "could not load projects");
                Vec::new()
            }
            Err(error) => {
                tracing::error!(%error, "project loading task failed");
                Vec::new()
            }
        };
        publish.send_modify(|snapshot| {
            let mut next = Snapshot::clone(snapshot);
            next.loaded = true;
            next.projects = list;
            *snapshot = Arc::new(next);
        });
    });
    CoreHandle { snapshots }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::Duration;

    use leon_domain::{Project, ProjectId, Settings};

    use super::{CoreHandle, start};
    use crate::ports::{PortError, ProjectRepository};

    struct FakeRepo(Result<Vec<Project>, PortError>);

    impl ProjectRepository for FakeRepo {
        fn list_projects(&self) -> Result<Vec<Project>, PortError> {
            self.0.clone()
        }
    }

    fn project(id: i64, name: &str) -> Project {
        Project {
            id: ProjectId(id),
            name: name.to_owned(),
            path: PathBuf::from("/tmp").join(name),
        }
    }

    async fn loaded(handle: &CoreHandle) -> Arc<super::Snapshot> {
        let mut rx = handle.subscribe();
        tokio::time::timeout(Duration::from_secs(2), rx.wait_for(|s| s.loaded))
            .await
            .expect("core did not publish in time")
            .expect("core stopped")
            .clone()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn publishes_projects_from_the_repository() {
        let repo = FakeRepo(Ok(vec![project(1, "api"), project(2, "web")]));
        let handle = start(
            &tokio::runtime::Handle::current(),
            Arc::new(repo),
            Settings::default(),
        );
        let snapshot = loaded(&handle).await;
        assert_eq!(
            snapshot.projects,
            vec![project(1, "api"), project(2, "web")]
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_repository_error_leaves_an_empty_list() {
        let repo = FakeRepo(Err(PortError::new("listing projects", "disk full")));
        let handle = start(
            &tokio::runtime::Handle::current(),
            Arc::new(repo),
            Settings::default(),
        );
        let snapshot = loaded(&handle).await;
        assert_eq!(snapshot.projects, Vec::<Project>::new());
    }
}
