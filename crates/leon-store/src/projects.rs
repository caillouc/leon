use std::path::PathBuf;

use leon_core::ports::{PortError, ProjectRepository};
use leon_domain::{Project, ProjectId};

use crate::Store;

impl ProjectRepository for Store {
    fn list_projects(&self) -> Result<Vec<Project>, PortError> {
        const CONTEXT: &str = "listing projects";
        let conn = self.lock();
        let mut stmt = conn
            .prepare_cached("SELECT id, name, path FROM projects ORDER BY name COLLATE NOCASE, id")
            .map_err(|e| PortError::new(CONTEXT, e))?;
        let rows = stmt
            .query_map([], |row| {
                Ok(Project {
                    id: ProjectId(row.get(0)?),
                    name: row.get(1)?,
                    path: PathBuf::from(row.get::<_, String>(2)?),
                })
            })
            .map_err(|e| PortError::new(CONTEXT, e))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| PortError::new(CONTEXT, e))
    }
}

#[cfg(test)]
mod tests {
    use leon_core::ports::ProjectRepository;

    use crate::Store;

    fn insert(store: &Store, name: &str, path: &str) {
        store
            .lock()
            .execute(
                "INSERT INTO projects (name, path, added_at) VALUES (?1, ?2, 0)",
                (name, path),
            )
            .unwrap();
    }

    #[test]
    fn empty_store_has_no_projects() {
        let store = Store::open_in_memory().unwrap();
        assert_eq!(
            store.list_projects().unwrap(),
            Vec::<leon_domain::Project>::new()
        );
    }

    #[test]
    fn projects_come_back_sorted_by_name() {
        let store = Store::open_in_memory().unwrap();
        insert(&store, "web", "/code/web");
        insert(&store, "Api", "/code/api");
        let names: Vec<_> = store
            .list_projects()
            .unwrap()
            .into_iter()
            .map(|p| p.name)
            .collect();
        assert_eq!(names, ["Api", "web"]);
    }

    #[test]
    fn data_survives_reopening_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.db");
        insert(&Store::open(&path).unwrap(), "api", "/code/api");
        let reopened = Store::open(&path).unwrap();
        assert_eq!(reopened.list_projects().unwrap().len(), 1);
    }
}
