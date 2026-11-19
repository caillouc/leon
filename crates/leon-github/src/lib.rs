//! Talks to GitHub with the user's own `gh` login (P7): the pull request of
//! a branch, the user's open issues and pull requests, one pull request or
//! issue by number, a Copilot review request, a comment, a pull request
//! marked draft or ready for review.
//!
//! Every call runs the GitHub CLI (`gh`, checked on 2.101.0, 2026-11-17;
//! by-number reads, board fields and review threads on 2026-11-18) with an
//! argument list, never through a shell; reads are one `gh api graphql`
//! query each. Calls block: the core runs them off its async workers.
//!
//! Must not: read, store or log a token (authentication stays inside `gh`);
//! change anything on GitHub beyond the port's review request, comment and
//! draft state;
//! depend on another adapter or on the UI.

mod board;
mod checks;
mod parse;
mod query;
mod run;
mod threads;
mod time;

use std::fmt::Display;
use std::path::Path;

use leon_core::ports::{GitHub, PortError, SearchPath, inherited_path};
use leon_domain::{FileDiff, Issue, PullRequest, parse_diff};

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

    /// Runs `gh args…`, which changes pull request or issue `number` of
    /// `repo`; an error says `context` and which one.
    fn change(
        &self,
        context: &'static str,
        repo: &str,
        number: u64,
        args: &[String],
    ) -> Result<(), PortError> {
        gh((self.path)(), args, None)
            .map(drop)
            .map_err(|e| numbered(context, repo, number)(&e))
    }

    /// Runs a query with the issues' board fields. Those need the token's
    /// `read:project` scope; without it, runs it again without them.
    fn with_board(&self, args: impl Fn(bool) -> Vec<String>) -> Result<String, GhError> {
        match gh((self.path)(), &args(true), None) {
            Err(GhError::Failed(message)) if message.contains("scope") => {
                gh((self.path)(), &args(false), None)
            }
            other => other,
        }
    }
}

/// Why a repository given as text cannot be read.
const NOT_OWNER_NAME: &str = "the repository is not written `owner/name`";

/// An error about pull request or issue `number` of `repo`.
fn numbered<'a>(
    context: &'static str,
    repo: &'a str,
    number: u64,
) -> impl Fn(&dyn Display) -> PortError + 'a {
    move |cause| PortError::new(context, format!("{repo}#{number}: {cause}"))
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
        let error = |cause: &dyn Display| {
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
        let error = |cause: &dyn Display| PortError::new("listing my issues", cause);
        let json = self
            .with_board(|board| query::search(query::MY_ISSUES, board))
            .map_err(|e| error(&e))?;
        parse::search_issues(&json).map_err(|e| error(&e))
    }

    fn my_pull_requests(&self) -> Result<Vec<PullRequest>, PortError> {
        let error = |cause: &dyn Display| PortError::new("listing my pull requests", cause);
        let json = gh(
            (self.path)(),
            &query::search(query::MY_PULL_REQUESTS, false),
            None,
        )
        .map_err(|e| error(&e))?;
        parse::search_pull_requests(&json).map_err(|e| error(&e))
    }

    fn pull_request_by_number(&self, repo: &str, number: u64) -> Result<PullRequest, PortError> {
        let error = numbered("reading the pull request", repo, number);
        let (owner, name) = query::owner_and_name(repo).ok_or_else(|| error(&NOT_OWNER_NAME))?;
        let args = query::pull_request_by_number(owner, name, number);
        let json = gh((self.path)(), &args, None).map_err(|e| error(&e))?;
        parse::pull_request_by_number(&json)
            .map_err(|e| error(&e))?
            .ok_or_else(|| error(&"GitHub sent no pull request"))
    }

    fn issue(&self, repo: &str, number: u64) -> Result<Issue, PortError> {
        let error = numbered("reading the issue", repo, number);
        let (owner, name) = query::owner_and_name(repo).ok_or_else(|| error(&NOT_OWNER_NAME))?;
        let json = self
            .with_board(|board| query::issue(owner, name, number, board))
            .map_err(|e| error(&e))?;
        parse::issue(&json)
            .map_err(|e| error(&e))?
            .ok_or_else(|| error(&"GitHub sent no issue"))
    }

    fn pull_request_diff(&self, repo: &str, number: u64) -> Result<Vec<FileDiff>, PortError> {
        let args = query::pull_request_diff(repo, number);
        gh((self.path)(), &args, None)
            .map(|text| parse_diff(&text))
            .map_err(|e| numbered("reading the pull request's changes", repo, number)(&e))
    }

    fn request_copilot_review(&self, repo: &str, number: u64) -> Result<(), PortError> {
        let args = query::copilot_review(repo, number);
        self.change("requesting a Copilot review", repo, number, &args)
    }

    fn comment(&self, repo: &str, number: u64, body: &str) -> Result<(), PortError> {
        let args = query::comment(repo, number, body);
        self.change("posting a comment", repo, number, &args)
    }

    fn reply_to_thread(&self, thread: &str, body: &str) -> Result<(), PortError> {
        gh((self.path)(), &query::reply_to_thread(thread, body), None)
            .map(drop)
            .map_err(|e| PortError::new("answering a review thread", e))
    }

    fn resolve_thread(&self, thread: &str) -> Result<(), PortError> {
        gh((self.path)(), &query::resolve_thread(thread), None)
            .map(drop)
            .map_err(|e| PortError::new("resolving a review thread", e))
    }

    fn set_draft(&self, repo: &str, number: u64, draft: bool) -> Result<(), PortError> {
        let context = if draft {
            "converting the pull request to a draft"
        } else {
            "marking the pull request ready for review"
        };
        let args = query::set_draft(repo, number, draft);
        self.change(context, repo, number, &args)
    }
}
