//! What each CLI last said of its plan, on SQLite (DOCTOR-2, USE-1): one
//! row per CLI, its windows as JSON.

use leon_core::ports::PortError;
use leon_domain::{BackendKind, PlanUsage};
use rusqlite::{Connection, params};

fn err(context: &'static str) -> impl Fn(rusqlite::Error) -> PortError {
    move |e| PortError::new(context, e)
}

/// Every CLI's, in `BackendKind::ALL` order; a row of a CLI or JSON this
/// build does not know is left out.
pub(crate) fn plan_usage(conn: &Connection) -> Result<Vec<PlanUsage>, PortError> {
    const C: &str = "reading the plans' usage";
    let rows: Vec<(String, String, Option<String>, i64)> = conn
        .prepare_cached("SELECT backend, windows, credits, at FROM plan_usage")
        .map_err(err(C))?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .map_err(err(C))?
        .collect::<Result<_, _>>()
        .map_err(err(C))?;
    let mut usage: Vec<PlanUsage> = rows
        .into_iter()
        .filter_map(|(backend, windows, credits, at)| {
            Some(PlanUsage {
                backend: BackendKind::from_id(&backend)?,
                windows: serde_json::from_str(&windows).ok()?,
                credits,
                at,
            })
        })
        .collect();
    usage.sort_by_key(|u| BackendKind::ALL.iter().position(|b| *b == u.backend));
    Ok(usage)
}

pub(crate) fn save(conn: &Connection, usage: &PlanUsage) -> Result<(), PortError> {
    const C: &str = "saving a plan's usage";
    let windows = serde_json::to_string(&usage.windows).map_err(|e| PortError::new(C, e))?;
    conn.execute(
        "INSERT OR REPLACE INTO plan_usage (backend, windows, credits, at) VALUES (?1, ?2, ?3, ?4)",
        params![usage.backend.id(), windows, usage.credits, usage.at],
    )
    .map(drop)
    .map_err(err(C))
}

#[cfg(test)]
mod tests {
    use leon_core::ports::Storage;
    use leon_domain::{BackendKind, LimitWindow, PlanUsage};

    use crate::Store;

    /// Saved, read back after a restart; saved again, it replaces the one
    /// before.
    #[test]
    fn a_plans_usage_is_kept_one_per_cli() {
        let store = Store::open_in_memory().unwrap();
        let usage = |backend, used, credits: Option<&str>, at| PlanUsage {
            backend,
            windows: vec![
                LimitWindow {
                    minutes: 300,
                    used,
                    resets_at: Some(1_000),
                    model: None,
                },
                // A model's own window keeps its model.
                LimitWindow {
                    minutes: 10_080,
                    used,
                    resets_at: Some(1_000),
                    model: Some("Fable".into()),
                },
            ],
            credits: credits.map(str::to_owned),
            at,
        };
        store
            .save_plan_usage(&usage(BackendKind::Codex, 40, Some("1250"), 1))
            .unwrap();
        store
            .save_plan_usage(&usage(BackendKind::ClaudeCode, 10, None, 2))
            .unwrap();
        store
            .save_plan_usage(&usage(BackendKind::Codex, 55, None, 3))
            .unwrap();
        assert_eq!(
            store.plan_usage().unwrap(),
            [
                usage(BackendKind::ClaudeCode, 10, None, 2),
                usage(BackendKind::Codex, 55, None, 3)
            ]
        );
    }
}
