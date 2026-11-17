//! Conversation titles from a one-off CLI call (NAV-9, D1): the conversation's
//! own CLI, its cheapest settings, run outside any project.

use std::ffi::OsString;
use std::time::Duration;

use leon_core::ports::PortError;
use leon_domain::BackendKind;

use crate::process::{RunError, run};

const CONTEXT: &str = "writing a conversation title";

/// Longest time a title call may take.
const TIMEOUT: Duration = Duration::from_secs(60);

/// Most characters of the first message given to the CLI.
const MESSAGE_MAX: usize = 2_000;

fn prompt(message: &str) -> String {
    let message: String = message.chars().take(MESSAGE_MAX).collect();
    format!(
        "Write a title of 3 to 6 words for a coding conversation that starts with the message \
         below. Reply with the title only, without quotes.\n\nMessage:\n{message}"
    )
}

/// The command for `backend`, both run on 2026-11-17: Claude Code 2.1.288
/// `-p` prints the answer; Codex 0.160.0 `exec` prints the last message on
/// stdout.
fn args(backend: BackendKind, message: &str) -> Option<(&'static str, Vec<String>)> {
    let prompt = prompt(message);
    let list = |items: &[&str]| -> Vec<String> {
        items
            .iter()
            .map(|s| (*s).to_owned())
            .chain(std::iter::once(prompt.clone()))
            .collect()
    };
    match backend {
        BackendKind::ClaudeCode => Some((
            "claude",
            list(&["-p", "--model", "haiku", "--output-format", "text"]),
        )),
        BackendKind::Codex => Some((
            "codex",
            list(&[
                "exec",
                "--skip-git-repo-check",
                "--ephemeral",
                "-s",
                "read-only",
                "-c",
                "model_reasoning_effort=\"low\"",
            ]),
        )),
        BackendKind::CopilotCli => None,
    }
}

pub(crate) fn title(
    path: Option<&OsString>,
    backend: BackendKind,
    message: &str,
) -> Result<String, PortError> {
    let Some((program, args)) = args(backend, message) else {
        return Err(PortError::new(
            CONTEXT,
            "this CLI has no one-off mode Leon uses",
        ));
    };
    let out = run(path, program, &args, TIMEOUT).map_err(|e| {
        PortError::new(
            CONTEXT,
            match e {
                RunError::NotFound => format!("{program} is not installed"),
                RunError::TimedOut => format!("{program} took too long"),
                RunError::Other(message) => format!("{program}: {message}"),
            },
        )
    })?;
    if !out.ok {
        return Err(PortError::new(CONTEXT, format!("{program} failed")));
    }
    clean(&out.stdout).ok_or_else(|| PortError::new(CONTEXT, format!("{program} gave no title")))
}

/// The title in a CLI's answer: its last non-empty line, without quotes,
/// markdown or a final period; `None` when that is empty or too long.
fn clean(out: &str) -> Option<String> {
    let decoration = |c: char| matches!(c, '"' | '\'' | '`' | '*' | '#') || c.is_whitespace();
    let line = out.lines().map(str::trim).rfind(|l| !l.is_empty())?;
    let line = line.trim_matches(decoration);
    let title = line
        .strip_prefix("Title:")
        .unwrap_or(line)
        .trim_matches(decoration)
        .trim_end_matches('.')
        .trim();
    (!title.is_empty() && title.chars().count() <= 80).then(|| title.to_owned())
}

#[cfg(test)]
mod tests {
    use leon_domain::BackendKind;

    use super::{args, clean};

    #[test]
    fn titles_are_cleaned() {
        let cases = [
            (
                "Extend Webhook Retries to 15 Minutes\n",
                Some("Extend Webhook Retries to 15 Minutes"),
            ),
            (
                "Sure!\n\n\"Fix the login test.\"\n",
                Some("Fix the login test"),
            ),
            ("**Title:** Migrate config", Some("Migrate config")),
            (
                "Title: Migrate config to TOML",
                Some("Migrate config to TOML"),
            ),
            ("   \n", None),
        ];
        for (out, expected) in cases {
            assert_eq!(clean(out).as_deref(), expected, "{out:?}");
        }
        assert_eq!(clean(&"word ".repeat(30)), None);
    }

    #[test]
    fn each_cli_gets_its_one_off_command() {
        let (program, list) = args(BackendKind::Codex, "fix it").unwrap();
        assert_eq!(program, "codex");
        assert_eq!(list[0], "exec");
        assert!(list.last().unwrap().ends_with("Message:\nfix it"));
        assert_eq!(
            args(BackendKind::ClaudeCode, "x").unwrap().1[..3],
            ["-p", "--model", "haiku"]
        );
        assert!(args(BackendKind::CopilotCli, "x").is_none());
    }
}
