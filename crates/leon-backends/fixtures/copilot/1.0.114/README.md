# Copilot CLI 1.0.114 over ACP

`permission-paths.jsonl`: recorded from Copilot CLI 1.0.114 (`copilot --acp`)
on 2026-11-22. Copilot asks permission to read paths outside its trusted
folders for a shell command it announced just before, with the same tool call
id: the command is in the announcement's `rawInput.command`, the paths in the
request's `rawInput.path` and `locations` (one comma-separated string).

Changes from the recording: only the lines of that tool call are kept; the
`session/new` answer and the final `session/prompt` answer are the ACP spec
examples' (`../../acp/1.32.0/permission.jsonl`), with request ids in the order
Leon sends them; paths, branch names, command output, session and tool call
ids are replaced by neutral ones.

`session-new.jsonl`: the `session/new` answer of Copilot CLI 1.0.114
(`copilot --acp`) on 2026-11-26, cwd a temporary folder, no MCP server: its
`models` (available ones, the current one) and `configOptions`. Changes from
the recording: the session id is `sess_1`.
