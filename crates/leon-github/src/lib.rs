//! Talks to GitHub with the user's own `gh` login (P7): the pull request of
//! a branch, the user's open issues and pull requests, a Copilot review
//! request, a comment.
//!
//! Every call runs the GitHub CLI (`gh`, checked on 2.101.0, 2026-11-17)
//! with an argument list, never through a shell; reads are one `gh api
//! graphql` query each. Calls block: the core runs them off its async
//! workers.
//!
//! Must not: read, store or log a token (authentication stays inside `gh`);
//! change anything on GitHub beyond the port's review request and comment;
//! depend on another adapter or on the UI.

mod checks;
mod parse;
mod query;
mod run;
mod time;

use std::path::Path;

use leon_core::ports::{GitHub, PortError, SearchPath, inherited_path};
use leon_domain::{Issue, PullRequest};

use crate::run::{GhError, gh};

/// [`GitHub`] through the `gh` command line and the login the user gave it.
#[derive(Debug, Clone)]
pub struct GhCli {
    path: SearchPath,
}

impl GhCli {
    /// Runs `gh` with the `PATH` that `path` gives.
    pub fn new(path: SearchPath) -> Self {
        Self { path }
    }
}

impl Default for GhCli {
    /// Runs `gh` with the inherited `PATH`.
    fn default() -> Self {
        Self::new(inherited_path)
    }
}

impl GitHub for GhCli {
    fn doctor(&self) -> leon_domain::DoctorSection {
        use leon_domain::{DoctorCheck, Health, version_check};
        let path = (self.path)();
        let args =
            |list: &[&str]| -> Vec<String> { list.iter().map(|s| (*s).to_owned()).collect() };
        let mut checks = Vec::new();
        match gh(path, &args(&["--version"]), None) {
            Err(GhError::Missing) => checks.push(
                DoctorCheck::new(Health::Problem, "Installed", "`gh` not found").fix(
                    "Install the GitHub CLI (`gh`): Leon reads pull requests and issues with it",
                ),
            ),
            Err(error) => checks.push(DoctorCheck::new(
                Health::Problem,
                "Installed",
                error.to_string(),
            )),
            Ok(out) => {
                let found = out.lines().next().unwrap_or_default().trim();
                checks.push(version_check("Version", found, "2.101.0"));
                // Exit status 0 when an account is logged in (gh 2.101.0).
                checks.push(match gh(path, &args(&["auth", "status"]), None) {
                    Ok(_) => DoctorCheck::new(Health::Ok, "Logged in", "yes"),
                    Err(_) => DoctorCheck::new(Health::Problem, "Logged in", "no")
                        .fix("Run `gh auth login` in a terminal"),
                });
            }
        }
        leon_domain::DoctorSection {
            title: "GitHub CLI".to_owned(),
            checks,
        }
    }

    fn pull_request(&self, dir: &Path, branch: &str) -> Result<Option<PullRequest>, PortError> {
        let error = |cause: &dyn std::fmt::Display| {
            PortError::new(
                "reading the pull request",
                format!("branch {branch} in {}: {cause}", dir.display()),
            )
        };
        match gh((self.path)(), &query::pull_request(branch), Some(dir)) {
            Ok(json) => parse::pull_request(&json, branch).map_err(|e| error(&e)),
            Err(GhError::NoGitHubRepo) => Ok(None),
            Err(e) => Err(error(&e)),
        }
    }

    fn my_issues(&self) -> Result<Vec<Issue>, PortError> {
        let error = |cause: &dyn std::fmt::Display| PortError::new("listing my issues", cause);
        // Board columns need the `read:project` scope; without it, the
        // issues come without their column.
        let json = match gh((self.path)(), &query::search(query::MY_ISSUES, true), None) {
            Err(GhError::Failed(message)) if message.contains("scope") => {
                gh((self.path)(), &query::search(query::MY_ISSUES, false), None)
            }
            other => other,
        }
        .map_err(|e| error(&e))?;
        parse::search_issues(&json).map_err(|e| error(&e))
    }

    fn my_pull_requests(&self) -> Result<Vec<PullRequest>, PortError> {
        let error =
            |cause: &dyn std::fmt::Display| PortError::new("listing my pull requests", cause);
        let json = gh(
            (self.path)(),
            &query::search(query::MY_PULL_REQUESTS, false),
            None,
        )
        .map_err(|e| error(&e))?;
        parse::search_pull_requests(&json).map_err(|e| error(&e))
    }

    fn request_copilot_review(&self, repo: &str, number: u64) -> Result<(), PortError> {
        gh((self.path)(), &query::copilot_review(repo, number), None)
            .map(drop)
            .map_err(|e| {
                PortError::new(
                    "requesting a Copilot review",
                    format!("{repo}#{number}: {e}"),
                )
            })
    }

    fn comment(&self, repo: &str, number: u64, body: &str) -> Result<(), PortError> {
        gh((self.path)(), &query::comment(repo, number, body), None)
            .map(drop)
            .map_err(|e| PortError::new("posting a comment", format!("{repo}#{number}: {e}")))
    }
}
